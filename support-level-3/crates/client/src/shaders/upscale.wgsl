// Upscaling temporel « FSR 3 » : port WGSL du noyau d'upscaling de
// FidelityFX Super Resolution 2/3 (MIT — Copyright (c) AMD), adapté au moteur :
//   - vecteurs de mouvement générés par reprojection de la profondeur
//     (inv_vp courante -> monde -> vp précédente), jitter inclus ;
//   - dilatation des vecteurs (voisin le plus proche en profondeur) ;
//   - accumulation temporelle plein écran : Lanczos 2 (échantillon 3x3 avec
//     biais de noyau), boîte de rectification (variance YCoCg), reprojection
//     de l'historique (Lanczos 2 référence 4x4), clamp de l'historique,
//     facteur de réactivité temporelle stocké dans history.a ;
//   - RCAS (netteté FSR, avec débruitage) en espace perceptuel (sRGB).
// Simplifications assumées (scène LDR sans transparence) : pas de passe
// depth-clip (scatter), pas de masques réactifs, pas de locks, pas de
// momentum : la boîte de rectification et le clamp suffisent ici.
//
// Passes : fs_mv -> fs_dilate -> fs_accum (pleine résolution) -> fs_rcas.

const FSR2_EPSILON: f32 = 1e-3;
const FSR_RCAS_LIMIT: f32 = 0.25 - 1.0 / 16.0;
const MAX_ACCUM_LANCZOS: f32 = 1.0;
const AVG_LANCZOS_PER_FRAME: f32 = 0.74 * (1.0 / 12.0);
const PI: f32 = 3.14159265358979;

struct UpsU {
    // xy = taille de sortie (swapchain) px, zw = 1/xy.
    display_size: vec4<f32>,
    // xy = taille interne (basse résolution) px, zw = 1/xy.
    render_size: vec4<f32>,
    // xy = jitter en pixels internes (x, -y), z = reset (1.0 = repart à neuf),
    // w = netteté RCAS (stops ; grand = plus doux).
    jitter: vec4<f32>,
    // inverse(proj * vue) courante (jitterée).
    inv_vp_cur: mat4x4<f32>,
    // (proj * vue) de la frame précédente (jitterée).
    vp_prev: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u: UpsU;
// Passe mv : profondeur du monde (Depth24Plus, 0 = proche).
@group(0) @binding(1) var depth_tex: texture_depth_2d;
// Passe accumulate : échantillonnage bilinéaire/lanczos.
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var post_tex: texture_2d<f32>;
// Passe dilate : vecteurs bruts.
@group(0) @binding(4) var mv_raw_tex: texture_2d<f32>;
// Passe accumulate : vecteurs dilatés.
@group(0) @binding(5) var mv_dil_tex: texture_2d<f32>;
// Historique accumulé (rgb = couleur linéaire, a = facteur de réactivité).
@group(0) @binding(6) var hist_tex: texture_2d<f32>;

struct VSOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VSOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VSOut;
    out.clip = vec4<f32>(pos[vi], 0.0, 1.0);
    return out;
}

// ---------- utilitaires couleur / maths ----------

fn RGBToYCoCg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.25 * c.r + 0.5 * c.g + 0.25 * c.b,
        0.5 * c.r - 0.5 * c.b,
        -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}

fn YCoCgToRGB(y: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(y.x + y.y - y.z, y.x + y.z, y.x - y.y - y.z);
}

// Approximation de Lanczos-2 en x² (FSR1/FSR2, entrée <= 4).
fn Lanczos2ApproxSq(x2: f32) -> f32 {
    let x2c = min(x2, 4.0);
    let a = (2.0 / 5.0) * x2c - 1.0;
    let b = (1.0 / 4.0) * x2c - 1.0;
    return ((25.0 / 16.0) * a * a - (25.0 / 16.0 - 1.0)) * (b * b);
}

// Lanczos-2 de référence (sinc).
fn Lanczos2Ref(x: f32) -> f32 {
    let xa = min(abs(x), 2.0);
    if (xa < FSR2_EPSILON) {
        return 1.0;
    }
    let px = PI * xa;
    return (sin(px) / px) * (sin(0.5 * px) / (0.5 * px));
}

fn APrxLoRcp(a: f32) -> f32 {
    return bitcast<f32>(0x7ef19fffu - bitcast<u32>(a));
}

fn APrxMedRcp(a: f32) -> f32 {
    let b = APrxLoRcp(a);
    return b * (2.0 - b * a);
}

fn srgb_enc1(c: f32) -> f32 {
    return select(12.92 * c, 1.055 * pow(max(c, 0.0), 1.0 / 2.4) - 0.055, c > 0.0031308);
}

fn srgb_dec1(c: f32) -> f32 {
    return select(c / 12.92, pow((max(c, 0.0) + 0.055) / 1.055, 2.4), c > 0.04045);
}

fn srgb_enc(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(srgb_enc1(c.r), srgb_enc1(c.g), srgb_enc1(c.b));
}

fn srgb_dec(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(srgb_dec1(c.r), srgb_dec1(c.g), srgb_dec1(c.b));
}

// ---------- passe 1 : vecteurs de mouvement par reprojection ----------

@fragment
fn fs_mv(in: VSOut) -> @location(0) vec2<f32> {
    let rs = u.render_size.xy;
    let ip = vec2<i32>(floor(in.clip.xy));
    let cl = clamp(ip, vec2<i32>(0), vec2<i32>(rs) - vec2<i32>(1));
    let d = textureLoad(depth_tex, cl, 0);
    let ndc = vec3<f32>(
        (in.clip.x / rs.x) * 2.0 - 1.0,
        1.0 - 2.0 * (in.clip.y / rs.y),
        d,
    );
    let wp4 = u.inv_vp_cur * vec4<f32>(ndc, 1.0);
    if (wp4.w <= 1e-6) {
        return vec2<f32>(0.0);
    }
    let wp = wp4.xyz / wp4.w;
    let cp = u.vp_prev * vec4<f32>(wp, 1.0);
    if (cp.w <= 1e-6) {
        return vec2<f32>(0.0);
    }
    // uv précédente - uv courante (convention FSR2 : l'UV reprojecte
    // l'historique). v vers le bas : (ndc * (0.5, -0.5) + 0.5).
    let puv = (cp.xy / cp.w) * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let cuv = (vec2<f32>(ip) + 0.5) / rs;
    var mv = puv - cuv;
    // Frame de reset : historique invalide -> vecteurs nuls.
    if (u.jitter.z > 0.5) {
        mv = vec2<f32>(0.0);
    }
    return mv;
}

// ---------- passe 2 : dilatation (profondeur la plus proche du 3x3) ----------

@fragment
fn fs_dilate(in: VSOut) -> @location(0) vec2<f32> {
    let rs = u.render_size.xy;
    let ip = vec2<i32>(floor(in.clip.xy));
    let mx = vec2<i32>(rs) - vec2<i32>(1);
    var best_depth = textureLoad(depth_tex, clamp(ip, vec2<i32>(0), mx), 0);
    var best_coord = ip;
    for (var oy: i32 = -1; oy <= 1; oy++) {
        for (var ox: i32 = -1; ox <= 1; ox++) {
            let p = clamp(ip + vec2<i32>(ox, oy), vec2<i32>(0), mx);
            let d = textureLoad(depth_tex, p, 0);
            if (d < best_depth) {
                best_depth = d;
                best_coord = p;
            }
        }
    }
    // Vecteur du pixel le plus proche (bords / objets en mouvement).
    return textureLoad(mv_raw_tex, clamp(best_coord, vec2<i32>(0), mx), 0).xy;
}

// ---------- passe 3 : accumulation temporelle (le coeur FSR 2/3) ----------

struct RectBox {
    center: vec3<f32>,
    box_vec: vec3<f32>,
    aabb_min: vec3<f32>,
    aabb_max: vec3<f32>,
    w: f32,
};

fn box_reset(b: ptr<function, RectBox>) {
    (*b).center = vec3<f32>(0.0);
    (*b).box_vec = vec3<f32>(0.0);
    (*b).aabb_min = vec3<f32>(3.4e38);
    (*b).aabb_max = vec3<f32>(-3.4e38);
    (*b).w = 0.0;
}

fn box_add(b: ptr<function, RectBox>, c: vec3<f32>, w: f32, initial: bool) {
    if (initial) {
        (*b).aabb_min = c;
        (*b).aabb_max = c;
    } else {
        (*b).aabb_min = min((*b).aabb_min, c);
        (*b).aabb_max = max((*b).aabb_max, c);
    }
    (*b).center += c * w;
    (*b).box_vec += c * w;
    (*b).w += w;
}

fn box_finalize(b: ptr<function, RectBox>) {
    let w = select(1.0, (*b).w, abs((*b).w) >= FSR2_EPSILON);
    (*b).center /= w;
    (*b).box_vec /= w;
    let sd = sqrt(abs((*b).box_vec - (*b).center * (*b).center));
    (*b).box_vec = sd;
}

fn hist_load(ip: vec2<i32>) -> vec4<f32> {
    let hs = vec2<i32>(u.display_size.xy);
    return textureLoad(hist_tex, clamp(ip, vec2<i32>(0), hs - vec2<i32>(1)), 0);
}

// Rééchantillonnage de l'historique en Lanczos-2 (référence, 4x4, v-deringing).
fn history_sample(uv: vec2<f32>) -> vec4<f32> {
    let hs = vec2<i32>(u.display_size.xy);
    let px = uv * vec2<f32>(hs) - 0.5;
    let base = vec2<i32>(floor(px));
    let frac = px - floor(px);
    var col = vec4<f32>(0.0);
    for (var j: i32 = 0; j < 4; j++) {
        let wy = Lanczos2Ref(f32(j - 1) - frac.y);
        var row = vec4<f32>(0.0);
        for (var i: i32 = 0; i < 4; i++) {
            row += Lanczos2Ref(f32(i - 1) - frac.x) * hist_load(base + vec2<i32>(i - 1, j - 1));
        }
        col += wy * row;
    }
    // Deringing : clamp aux 4 échantillons centraux (comme FSR2).
    var dmin = hist_load(base);
    var dmax = dmin;
    for (var k: i32 = 0; k < 4; k++) {
        let o = vec2<i32>(k & 1, (k >> 1) & 1);
        let s = hist_load(base + o);
        dmin = min(dmin, s);
        dmax = max(dmax, s);
    }
    return clamp(col, dmin, dmax);
}

@fragment
fn fs_accum(in: VSOut) -> @location(0) vec4<f32> {
    let disp = u.display_size.xy;
    let rend = u.render_size.xy;
    let df = rend / disp; // facteur de réduction (DownscaleFactor)
    let ip = vec2<i32>(floor(in.clip.xy));

    // --- InitParams ---
    let f_hr_uv = (vec2<f32>(ip) + 0.5) / disp;
    let f_lr_uv_jit = clamp(f_hr_uv + u.jitter.xy / rend, vec2<f32>(0.0), vec2<f32>(1.0));
    let mxr = vec2<i32>(rend) - vec2<i32>(1);
    let lrip = clamp(vec2<i32>(f_lr_uv_jit * rend), vec2<i32>(0), mxr);
    let mv = textureLoad(mv_dil_tex, lrip, 0).xy;
    let f_hr_velocity = length(mv * disp);
    let reproj_uv = f_hr_uv + mv;
    let existing = (reproj_uv.x >= 0.0) && (reproj_uv.x <= 1.0)
        && (reproj_uv.y >= 0.0) && (reproj_uv.y <= 1.0);
    let reset_frame = u.jitter.z > 0.5;
    let is_new = (!existing) || reset_frame;

    // --- ReprojectHistoryColor ---
    var f_history = vec3<f32>(0.0);
    var f_reactive = 0.0;
    var in_motion_last = 0.0;
    if (existing && !reset_frame) {
        let h = history_sample(reproj_uv);
        f_history = RGBToYCoCg(h.rgb);
        f_reactive = clamp(abs(h.w), 0.0, 1.0);
        in_motion_last = select(0.0, 1.0, h.w < 0.0);
    }
    let this_frame_reactive = f_reactive; // facteur dilaté = 0 (pas de masques)

    // --- ComputeUpsampledColorAndWeight : Lanczos 3x3 + boîte de rectification ---
    let f_dst_pos = vec2<f32>(ip) + 0.5;
    let f_src_pos = f_dst_pos * df;
    let i_src_pos = floor(f_src_pos);
    let f_src_unjit = (i_src_pos + vec2<f32>(0.5)) - u.jitter.xy;
    let flip_col = f_src_unjit.x > f_src_pos.x;
    let flip_row = f_src_unjit.y > f_src_pos.y;
    var offset_tl = vec2<i32>(-1, -1);
    if (flip_col) { offset_tl.x = -2; }
    if (flip_row) { offset_tl.y = -2; }
    let f_offset_tl = vec2<f32>(offset_tl);

    let f_kernel_reactive = max(this_frame_reactive, select(0.0, 1.0, is_new));
    let kernel_bias_max = min(1.99, 1.0 + (1.0 / df.x - 1.0)) * (1.0 - f_kernel_reactive);
    let kernel_bias_min = max(1.0, (1.0 + kernel_bias_max) * 0.3);
    let kernel_bias = max(0.0, mix(kernel_bias_max, kernel_bias_min, f_kernel_reactive));
    let rect_curve_bias = mix(-2.0, -3.0, clamp(f_hr_velocity / 50.0, 0.0, 1.0));

    var box: RectBox;
    box_reset(&box);
    var cw = vec4<f32>(0.0);
    let base_off = f_src_unjit - f_src_pos;
    for (var row: i32 = 0; row < 3; row++) {
        for (var col: i32 = 0; col < 3; col++) {
            let scr = vec2<i32>(select(col, 3 - col, flip_col), select(row, 3 - row, flip_row));
            let s_pos = vec2<i32>(i_src_pos) + offset_tl + scr;
            let s_cl = clamp(s_pos, vec2<i32>(0), mxr);
            let c = RGBToYCoCg(textureLoad(post_tex, s_cl, 0).rgb);
            let off = base_off + f_offset_tl + vec2<f32>(scr);
            var w = 0.0;
            if (s_pos.x >= 0 && s_pos.x < i32(rend.x) && s_pos.y >= 0 && s_pos.y < i32(rend.y)) {
                let biased = off * vec2<f32>(kernel_bias);
                w = Lanczos2ApproxSq(dot(biased, biased));
            }
            cw += vec4<f32>(c * w, w);
            box_add(&box, c, exp(rect_curve_bias * dot(off, off)), row == 0 && col == 0);
        }
    }
    box_finalize(&box);

    cw.w *= select(0.0, 1.0, cw.w > FSR2_EPSILON);
    if (cw.w > FSR2_EPSILON) {
        cw = vec4<f32>(cw.xyz / cw.w, cw.w * (1.0 / 12.0)); // fUpsampleLanczosWeightScale
        // Deringing : clamp à la boîte (aabb).
        cw = vec4<f32>(clamp(cw.xyz, box.aabb_min, box.aabb_max), cw.w);
    }

    // --- ComputeBaseAccumulationWeight ---
    var base = MAX_ACCUM_LANCZOS * select(0.0, 1.0, existing)
        * (1.0 - this_frame_reactive); // depth clip = 0
    base = min(base, mix(base, cw.w * 10.0,
        max(in_motion_last, clamp(f_hr_velocity * 10.0, 0.0, 1.0))));
    base = min(base, mix(base, cw.w, clamp(f_hr_velocity / 20.0, 0.0, 1.0)));
    var f_accum = vec3<f32>(base);

    // --- RectifyHistory (sans lock / luma-history : contribution = 0) ---
    if (!is_new) {
        let scale_influence = min(20.0, pow(1.0 / abs(df.x * df.y), 3.0));
        let vel_factor = clamp(f_hr_velocity / 20.0, 0.0, 1.0);
        let f_box_scale = mix(scale_influence, 1.0, vel_factor);
        let scaled = box.box_vec * f_box_scale;
        let box_min = max(box.aabb_min, box.center - scaled);
        let box_max = min(box.aabb_max, box.center + scaled);
        if (any(box_min > f_history) || any(f_history > box_max)) {
            // Historique hors boîte -> clampé, accumulation repart de 0.1.
            f_history = clamp(f_history, box_min, box_max);
            f_accum = min(f_accum, vec3<f32>(0.1));
        }
    }

    // --- Accumulate ---
    f_accum = max(vec3<f32>(FSR2_EPSILON), f_accum + vec3<f32>(cw.w));
    let f_alpha = vec3<f32>(cw.w) / f_accum;
    f_history = mix(f_history, cw.xyz, f_alpha);
    f_history = YCoCgToRGB(f_history);

    // --- ComputeTemporalReactiveFactor (stocké pour la frame suivante) ---
    var new_reactive = min(0.99, this_frame_reactive);
    new_reactive = max(new_reactive, mix(new_reactive, 0.4, clamp(f_hr_velocity, 0.0, 1.0)));
    new_reactive = max(new_reactive * new_reactive, 0.0);
    new_reactive = select(new_reactive, 1.0, is_new);
    if (clamp(f_hr_velocity * 10.0, 0.0, 1.0) >= 1.0) {
        new_reactive = max(FSR2_EPSILON, new_reactive) * -1.0;
    }

    return vec4<f32>(f_history, new_reactive);
}

// ---------- passe 4 : RCAS (FSR 1/2, avec débruitage) ----------

@fragment
fn fs_rcas(in: VSOut) -> @location(0) vec4<f32> {
    let hs = vec2<i32>(u.display_size.xy);
    let mx = hs - vec2<i32>(1);
    let sp = vec2<i32>(floor(in.clip.xy));
    // RCAS opère en espace perceptuel : l'historique est linéaire.
    let b = srgb_enc(textureLoad(hist_tex, clamp(sp + vec2<i32>(0, -1), vec2<i32>(0), mx), 0).rgb);
    let d = srgb_enc(textureLoad(hist_tex, clamp(sp + vec2<i32>(-1, 0), vec2<i32>(0), mx), 0).rgb);
    let e = srgb_enc(textureLoad(hist_tex, clamp(sp, vec2<i32>(0), mx), 0).rgb);
    let f = srgb_enc(textureLoad(hist_tex, clamp(sp + vec2<i32>(1, 0), vec2<i32>(0), mx), 0).rgb);
    let h = srgb_enc(textureLoad(hist_tex, clamp(sp + vec2<i32>(0, 1), vec2<i32>(0), mx), 0).rgb);

    // Luma x2.
    let bl = b.b * 0.5 + (b.r * 0.5 + b.g);
    let dl = d.b * 0.5 + (d.r * 0.5 + d.g);
    let el = e.b * 0.5 + (e.r * 0.5 + e.g);
    let fl = f.b * 0.5 + (f.r * 0.5 + f.g);
    let hl = h.b * 0.5 + (h.r * 0.5 + h.g);

    // Détection de bruit (FSR_RCAS_DENOISE).
    var nz = 0.25 * bl + 0.25 * dl + 0.25 * fl + 0.25 * hl - el;
    nz = clamp(abs(nz) * APrxMedRcp(max(max(max(bl, dl), el), fl)
        - min(min(min(bl, dl), el), fl)), 0.0, 1.0);
    nz = -0.5 * nz + 1.0;

    // Min/max de l'anneau.
    let mn4 = min(min(min(b, d), f), h);
    let mx4 = max(max(max(b, d), f), h);
    let peak_c = vec2<f32>(1.0, -4.0);
    let hit_min = min(mn4, e) * (1.0 / (4.0 * mx4));
    let hit_max = (peak_c.x - max(mx4, e)) / (4.0 * mn4 + peak_c.y);
    let lobe_r = max(-hit_min.r, hit_max.r);
    let lobe_g = max(-hit_min.g, hit_max.g);
    let lobe_b = max(-hit_min.b, hit_max.b);
    let lobe = max(-FSR_RCAS_LIMIT, min(max(lobe_r, max(lobe_g, lobe_b)), 0.0)) * exp2(-u.jitter.w);
    let lobe_n = lobe * nz;
    let rcp_l = APrxMedRcp(4.0 * lobe_n + 1.0);
    let pix = (lobe_n * (b + d + h + f) + e) * vec3<f32>(rcp_l);

    // Retour en linéaire : la cible sRGB ré-encode à l'écriture.
    return vec4<f32>(srgb_dec(pix), 1.0);
}
