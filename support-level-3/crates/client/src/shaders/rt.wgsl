// Passe de ray tracing (optionnelle) : pour chaque pixel du tampon RT,
// reconstruit la position monde depuis la profondeur, calcule la normale
// par différences de profondeur, puis lance des rayons contre les AABB
// de la scène :
//   - ombre douces des néons (rayon d'ombre par lumière proche, jitter stable)
//   - ombre de la lampe torche (rayon déterministe)
//   - occlusion ambiante (1 rayon cosinus hémisphère)
//   - un rebond de lumière approximatif depuis le point touché (mode Ultra)
// Sortie MRT : out0 = (ao, ombre_statique, ombre_torche, 1)
//              out1 = (gi.rgb, 1)
// Résolution indépendante (0.4x / 0.5x du tampon monde) — pensé RTX 2060.

struct WorldU {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    light_pos: array<vec4<f32>, 24>,
    light_col: array<vec4<f32>, 24>,
    flash_pos: vec4<f32>,
    flash_dir: vec4<f32>,
    misc: vec4<f32>,
    flash_col: vec4<f32>,
};

struct RtParams {
    inv_vp: mat4x4<f32>,
    cam_pos: vec4<f32>,
    dims: vec4<f32>,   // x,y : taille tampon RT · z,w : taille texture profondeur
    counts: vec4<f32>, // x : boîtes statiques · y : dynamiques · z : lumières · w : rayon AO
    misc: vec4<f32>,   // x : force GI · y : AO appliquée à la lumière directe · z,w : réserve
};

struct GpuAabb {
    lo: vec4<f32>,
    hi: vec4<f32>,
};

@group(0) @binding(0) var<uniform> rp: RtParams;
@group(0) @binding(1) var<uniform> u: WorldU;
@group(0) @binding(2) var<storage, read> boxes_static: array<GpuAabb>;
@group(0) @binding(3) var<storage, read> boxes_dyn: array<GpuAabb>;
@group(0) @binding(4) var depth_tex: texture_depth_2d;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VSOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VSOut;
    out.pos = vec4<f32>(pos[vi], 0.0, 1.0);
    out.uv = vec2<f32>((pos[vi].x + 1.0) * 0.5, 1.0 - (pos[vi].y + 1.0) * 0.5);
    return out;
}

// ---------- utilitaires ----------

fn hash3(p: vec3<f32>) -> vec3<f32> {
    let q = vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    );
    return fract(sin(q) * vec3<f32>(43758.5453, 22578.145, 19642.349));
}

fn hash1(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(12.9898, 78.233, 45.164))) * 43758.5453);
}

/// Évite les divisions par zéro dans le slab test.
fn safe_dir(d: vec3<f32>) -> vec3<f32> {
    return select(d, vec3<f32>(1e-6), abs(d) < vec3<f32>(1e-8));
}

/// Slab test : t d'entrée (> 0) si la boîte est touchée avant tmax, sinon -1.
fn ray_aabb(ro: vec3<f32>, rd: vec3<f32>, b: GpuAabb, tmax: f32) -> f32 {
    if (b.hi.x <= b.lo.x || b.hi.y <= b.lo.y || b.hi.z <= b.lo.z) {
        return -1.0; // boîte dégénérée (padding) -> jamais touchée
    }
    let inv = vec3<f32>(1.0) / rd;
    let t0 = (b.lo.xyz - ro) * inv;
    let t1 = (b.hi.xyz - ro) * inv;
    let tmin3 = min(t0, t1);
    let tmax3 = max(t0, t1);
    let tmin = max(max(tmin3.x, tmin3.y), max(tmin3.z, 0.0));
    let tm = min(min(tmax3.x, tmax3.y), tmax3.z);
    if (tm < tmin || tmin > tmax) {
        return -1.0;
    }
    return tmin;
}

/// Rayon d'ombre : 0 si un obstacle est rencontré, 1 sinon (sortie anticipée).
fn shadow_ray(ro: vec3<f32>, rd_in: vec3<f32>, dist: f32) -> f32 {
    if (dist <= 0.03) {
        return 1.0;
    }
    let rd = safe_dir(rd_in);
    let sn = u32(rp.counts.x);
    for (var i = 0u; i < sn; i = i + 1u) {
        let t = ray_aabb(ro, rd, boxes_static[i], dist);
        if (t >= 0.0) {
            return 0.0;
        }
    }
    let dn = u32(rp.counts.y);
    for (var i = 0u; i < dn; i = i + 1u) {
        let t = ray_aabb(ro, rd, boxes_dyn[i], dist);
        if (t >= 0.0) {
            return 0.0;
        }
    }
    return 1.0;
}

/// Trace un rayon contre toute la scène, renvoie le t du point le plus proche
/// (-1 si rien) et optionnellement la normale de la face touchée.
fn trace(ro: vec3<f32>, rd_in: vec3<f32>, tmax: f32, nrm: ptr<function, vec3<f32>>) -> f32 {
    let rd = safe_dir(rd_in);
    var best = tmax;
    var hit = false;
    let sn = u32(rp.counts.x);
    for (var i = 0u; i < sn; i = i + 1u) {
        let t = ray_aabb(ro, rd, boxes_static[i], best);
        if (t >= 0.0 && t < best) {
            best = t;
            hit = true;
            *nrm = aabb_normal(boxes_static[i], ro + rd * t);
        }
    }
    let dn = u32(rp.counts.y);
    for (var i = 0u; i < dn; i = i + 1u) {
        let t = ray_aabb(ro, rd, boxes_dyn[i], best);
        if (t >= 0.0 && t < best) {
            best = t;
            hit = true;
            *nrm = aabb_normal(boxes_dyn[i], ro + rd * t);
        }
    }
    return select(-1.0, best, hit);
}

/// Normale de la face d'AABB la plus proche d'un point de la surface.
fn aabb_normal(b: GpuAabb, p: vec3<f32>) -> vec3<f32> {
    var n = vec3<f32>(0.0, 1.0, 0.0);
    var best = abs(p.y - b.lo.y);
    var d = abs(p.y - b.hi.y);
    if (d < best) { best = d; n = vec3<f32>(0.0, -1.0, 0.0); }
    d = abs(p.x - b.lo.x);
    if (d < best) { best = d; n = vec3<f32>(-1.0, 0.0, 0.0); }
    d = abs(p.x - b.hi.x);
    if (d < best) { best = d; n = vec3<f32>(1.0, 0.0, 0.0); }
    d = abs(p.z - b.lo.z);
    if (d < best) { best = d; n = vec3<f32>(0.0, 0.0, -1.0); }
    d = abs(p.z - b.hi.z);
    if (d < best) { best = d; n = vec3<f32>(0.0, 0.0, 1.0); }
    return n;
}

fn wpos_from_depth(uv: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let h = rp.inv_vp * ndc;
    return h.xyz / max(h.w, 1e-5);
}

// ---------- fragment ----------

struct FSOut {
    @location(0) out0: vec4<f32>,
    @location(1) out1: vec4<f32>,
};

@fragment
fn fs(in: VSOut) -> FSOut {
    var o: FSOut;
    o.out0 = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    o.out1 = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    let rt_dims = vec2<f32>(rp.dims.x, rp.dims.y);
    let px = vec2<i32>(in.pos.xy);
    let uv = (vec2<f32>(px) + vec2<f32>(0.5, 0.5)) / rt_dims;
    let dsize = vec2<i32>(i32(rp.dims.z), i32(rp.dims.w));
    let dsize1 = dsize - vec2<i32>(1, 1);
    let dpx = clamp(vec2<i32>(uv * vec2<f32>(rp.dims.z, rp.dims.w)), vec2<i32>(0, 0), dsize1);

    let d0 = textureLoad(depth_tex, dpx, 0);
    if (d0 >= 0.99995) {
        return o; // pas de géométrie -> identité
    }
    let p = wpos_from_depth(uv, d0);

    // Normale par différences de profondeur (voisins droite / bas).
    let dpx_r = clamp(dpx + vec2<i32>(1, 0), vec2<i32>(0, 0), dsize1);
    let dpx_d = clamp(dpx + vec2<i32>(0, 1), vec2<i32>(0, 0), dsize1);
    let uv_r = (vec2<f32>(dpx_r) + vec2<f32>(0.5, 0.5)) / vec2<f32>(rp.dims.z, rp.dims.w);
    let uv_d = (vec2<f32>(dpx_d) + vec2<f32>(0.5, 0.5)) / vec2<f32>(rp.dims.z, rp.dims.w);
    let d1 = textureLoad(depth_tex, dpx_r, 0);
    let d2 = textureLoad(depth_tex, dpx_d, 0);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    if (d1 < 0.99995 && d2 < 0.99995) {
        let pr = wpos_from_depth(uv_r, d1);
        let pd = wpos_from_depth(uv_d, d2);
        let cn = normalize(cross(pr - p, pd - p));
        if (dot(cn, rp.cam_pos.xyz - p) > 0.0) {
            n = cn;
        }
    }

    // ---- Ombres des néons (moyenne pondérée par leur contribution) ----
    let nl_count = u32(u.misc.x);
    var sum_w = 0.0;
    var sum_wv = 0.0;
    for (var i = 0u; i < 24u; i = i + 1u) {
        if (i >= nl_count) { break; }
        let lp = u.light_pos[i];
        let lc = u.light_col[i];
        if (lc.w <= 0.01) { continue; }
        let to = lp.xyz - p;
        let dist = length(to);
        if (dist > lp.w || dist < 0.05) { continue; }
        let att = smoothstep(lp.w, lp.w * 0.2, dist);
        let w = lc.w * att * att * max(dot(n, to / max(dist, 0.001)), 0.0);
        if (w <= 0.004) { continue; }
        // Jitter stable par pixel/lumière : pénombre sans scintillement.
        let j = (hash3(vec3<f32>(f32(px.x), f32(px.y), f32(i) * 7.31)) - vec3<f32>(0.5)) * 0.36;
        let lt = lp.xyz + j;
        let vis = shadow_ray(p + n * 0.012, normalize(lt - p), distance(p, lt) - 0.2);
        sum_w = sum_w + w;
        sum_wv = sum_wv + w * vis;
    }
    let sh_static = select(1.0, sum_wv / max(sum_w, 1e-4), sum_w > 1e-4);

    // ---- Ombre de la torche (déterministe) ----
    var sh_flash = 1.0;
    if (u.flash_col.w > 0.001) {
        let to = p - u.flash_pos.xyz;
        let dist = max(length(to), 0.001);
        let dir = to / dist;
        let cos_a = dot(dir, normalize(u.flash_dir.xyz));
        if (cos_a > u.misc.w - 0.02 && dist < 15.0) {
            sh_flash = shadow_ray(p + n * 0.012, -dir, dist - 0.1);
        }
    }

    // ---- AO + rebond (1 rayon cosinus hémisphère, hash stable) ----
    let ao_radius = rp.counts.w;
    var ao = 1.0;
    var gi = vec3<f32>(0.0, 0.0, 0.0);
    let up = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    let t_dir = normalize(cross(n, up));
    let b_dir = cross(n, t_dir);
    let u1 = hash1(vec3<f32>(f32(px.x) * 1.13, f32(px.y) * 1.27, 3.1));
    let u2 = hash1(vec3<f32>(f32(px.x) * 1.71, f32(px.y) * 1.93, 9.4));
    let ang = u1 * 6.2831853;
    let rr = sqrt(u2);
    let local = vec3<f32>(cos(ang) * rr, sin(ang) * rr, sqrt(max(1.0 - u2, 0.0)));
    let dir_w = normalize(t_dir * local.x + b_dir * local.y + n * local.z);
    var hn = vec3<f32>(0.0, 1.0, 0.0);
    let t = trace(p + n * 0.012, dir_w, ao_radius, &hn);
    if (t >= 0.0) {
        ao = clamp(t / ao_radius, 0.0, 1.0);
        ao = ao * ao; // rapproche le profil d'un AO multi-échantillons
        // Rebond approximatif : depuis le point touché, contribution des néons.
        if (rp.misc.x > 0.001) {
            let hp2 = p + dir_w * t + hn * 0.01;
            for (var i = 0u; i < 24u; i = i + 1u) {
                if (i >= nl_count) { break; }
                let lp = u.light_pos[i];
                let lc = u.light_col[i];
                if (lc.w <= 0.01) { continue; }
                let tol = lp.xyz - hp2;
                let dl = length(tol);
                if (dl > lp.w) { continue; }
                let attl = smoothstep(lp.w, lp.w * 0.2, dl);
                gi = gi + lc.rgb * lc.w * attl * attl * max(dot(hn, tol / max(dl, 0.001)), 0.0);
            }
            gi = gi * rp.misc.x * 0.5;
        }
    }

    o.out0 = vec4<f32>(ao, sh_static, sh_flash, 1.0);
    o.out1 = vec4<f32>(gi, 1.0);
    return o;
}
