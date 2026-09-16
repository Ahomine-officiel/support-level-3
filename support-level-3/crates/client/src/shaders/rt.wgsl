// Passe de ray tracing (optionnelle) : pour chaque pixel du tampon RT,
// reconstruit la position monde depuis la profondeur, calcule la normale
// par différences de profondeur, puis lance de VRAIS rayons contre la scène :
//   - ombre douces des néons (rayon d'ombre par lumière proche, jitter stable)
//   - ombre de la lampe torche (rayon déterministe)
//   - occlusion ambiante (1 rayon en Qualité, 3 en Ultra/Overdrive)
//   - PATH TRACING Monte Carlo : des chemins complets (1 à 3 rebonds cosinus
//     sur l'albedo des surfaces) qui échantillonnent à CHAQUE point touché
//     l'éclairage direct des néons et de la torche (next event estimation)
//     AVEC ombres portées — la lumière des néons coule, rebondit du sol au
//     plafond et colore le couloir (le même algorithme que le mode Overdrive
//     de Cyberpunk 2077, en compute plutôt que sur RT cores)
//   - RÉFLEXIONS PLEINE SCÈNE : le rayon miroir trace toute la scène et le
//     point touché est ré-éclairé (NEE) — le lino renvoie le couloir, les
//     portes, les néons et la traînée de la torche
// Sortie MRT : out0 = (ao, ombre_statique, ombre_torche, 1)
//              out1 = (gi path traced + réflexions, 1)
// Résolution indépendante (0.4x / 0.5x / 0.6x du tampon monde) — pensé RTX 2060.

struct WorldU {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    light_pos: array<vec4<f32>, 24>,
    light_col: array<vec4<f32>, 24>,
    flash_pos: vec4<f32>,
    flash_dir: vec4<f32>,
    misc: vec4<f32>,
    flash_col: vec4<f32>,
    /// x = calme (1 début → 0 horreur), y = peur.
    mood: vec4<f32>,
};

struct RtParams {
    inv_vp: mat4x4<f32>,
    cam_pos: vec4<f32>,
    dims: vec4<f32>,   // x,y : taille tampon RT · z,w : taille texture profondeur
    counts: vec4<f32>, // x : boîtes statiques · y : dynamiques · z : lumières · w : rayon AO
    misc: vec4<f32>,   // x : force GI · y : réserve · z : échantillons AO (1 ou 3) · w : réserve
    pt: vec4<f32>,     // x : rayons GI/px · y : rebonds · z : budget ombres (0/1/2) · w : réserve
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

/// La boîte contient-elle le point (avec marge) ?
fn box_contains(b: GpuAabb, pt: vec3<f32>, m: f32) -> bool {
    return pt.x >= b.lo.x - m && pt.x <= b.hi.x + m && pt.y >= b.lo.y - m && pt.y <= b.hi.y + m && pt.z >= b.lo.z - m && pt.z <= b.hi.z + m;
}

/// Rayon d'ombre vers un NÉON : ignore les boîtes qui contiennent le point
/// lumière (support du luminaire, dalle du plafond). Sans ça, le néon
/// s'auto-ombre : tous les halos disparaissent en RT (« tout est noir »).
fn shadow_ray_light(ro: vec3<f32>, rd_in: vec3<f32>, dist: f32, lt: vec3<f32>) -> f32 {
    if (dist <= 0.03) {
        return 1.0;
    }
    let rd = safe_dir(rd_in);
    let sn = u32(rp.counts.x);
    for (var i = 0u; i < sn; i = i + 1u) {
        if (box_contains(boxes_static[i], lt, 0.06)) {
            continue;
        }
        let t = ray_aabb(ro, rd, boxes_static[i], dist);
        if (t >= 0.0) {
            return 0.0;
        }
    }
    let dn = u32(rp.counts.y);
    for (var i = 0u; i < dn; i = i + 1u) {
        if (box_contains(boxes_dyn[i], lt, 0.06)) {
            continue;
        }
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

// ---------- éclairage direct par next event estimation ----------

/// Direction cosinus-autour de n (échantillonnage Monte Carlo diffus).
fn cosine_hemi(n: vec3<f32>, u1: f32, u2: f32) -> vec3<f32> {
    // up = l'axe le PLUS ORTHOGONAL à n : l'ancienne sélection (n.y > 0.9 ->
    // up = x sinon z) donnait cross(n, up) = 0 pour n = ±Z exact (murs latéraux,
    // normales d'AABB) -> normalize(0) = NaN -> GI entière NaN (inf en f16).
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(n.z) < 0.9);
    let t = normalize(cross(n, up));
    let b = cross(n, t);
    let ang = u1 * 6.2831853;
    let rr = sqrt(u2);
    let z = sqrt(max(1.0 - u2, 0.0));
    return normalize(t * (cos(ang) * rr) + b * (sin(ang) * rr) + n * z);
}

/// Albedo approximé des surfaces touchées par les rebonds (pas de G-buffer
/// de textures : lino chaud, murs froids, plafond clair, variation spatiale
/// pour casser l'uniformité).
fn surface_albedo(n: vec3<f32>, p: vec3<f32>) -> vec3<f32> {
    var a = vec3<f32>(0.36, 0.38, 0.41); // murs (gris froid)
    if (n.y > 0.5) {
        a = vec3<f32>(0.44, 0.41, 0.36); // lino (chaud)
    } else if (n.y < -0.5) {
        a = vec3<f32>(0.5, 0.51, 0.53); // plafond
    }
    let v = 0.85 + 0.3 * hash1(floor(p * 3.1));
    return a * v;
}

/// Éclairage direct des néons au point p (NEE). Budget d'ombres :
///   0 = somme non ombrée (le plus rapide — Qualité)
///   1 = la lumière dominante reçoit son rayon d'ombre (Ultra)
///   2 = chaque lumière contributrice reçoit son rayon d'ombre (Overdrive)
fn neon_direct(p: vec3<f32>, n: vec3<f32>, budget: u32, px: vec2<i32>, salt: u32) -> vec3<f32> {
    let nl_count = u32(u.misc.x);
    var unshadowed = vec3<f32>(0.0, 0.0, 0.0);
    var shadowed = vec3<f32>(0.0, 0.0, 0.0);
    var best_w = 0.0;
    var best_i = 0u;
    var any_w = 0.0;
    for (var i = 0u; i < 24u; i = i + 1u) {
        if (i >= nl_count) { break; }
        let lp = u.light_pos[i];
        let lc = u.light_col[i];
        if (lc.w <= 0.01) { continue; }
        let to = lp.xyz - p;
        let dist = length(to);
        if (dist > lp.w || dist < 0.05) { continue; }
        let att = 1.0 - clamp(dist / max(lp.w, 0.001), 0.0, 1.0); // linéaire bornée (l'ancien smoothstep à bords inversés explosait au loin -> inf en f16)
        let nl = max(dot(n, to / max(dist, 0.001)), 0.0);
        let w = lc.w * att * att * nl;
        if (w <= 0.004) { continue; }
        unshadowed = unshadowed + lc.rgb * w;
        any_w = any_w + w;
        if (budget == 1u) {
            if (w > best_w) {
                best_w = w;
                best_i = i;
            }
        } else if (budget >= 2u) {
            // Jitter de pénombre stable (pas de scintillement) ; la boîte qui
            // contient la lumière ne peut pas l'occulter (support luminaire).
            let j = (hash3(vec3<f32>(f32(px.x) * 1.37 + f32(salt) * 91.7, f32(px.y) * 1.11, f32(i) * 7.31 + f32(salt) * 3.3)) - vec3<f32>(0.5)) * 0.24;
            let lt = lp.xyz + j;
            let tol = lt - p;
            let dl = max(length(tol), 0.001);
            let vis = shadow_ray_light(p + n * 0.012, tol / dl, dl - 0.2, lt);
            shadowed = shadowed + lc.rgb * w * vis;
        }
    }
    if (budget == 0u || any_w <= 1e-4) {
        return unshadowed;
    }
    if (budget == 1u) {
        if (best_w <= 0.0) {
            return unshadowed;
        }
        let lp = u.light_pos[best_i];
        let j = (hash3(vec3<f32>(f32(px.x) * 1.37 + f32(salt) * 91.7, f32(px.y) * 1.11, f32(best_i) * 7.31 + f32(salt) * 3.3)) - vec3<f32>(0.5)) * 0.24;
        let lt = lp.xyz + j;
        let tol = lt - p;
        let dl = max(length(tol), 0.001);
        let vis = shadow_ray_light(p + n * 0.012, tol / dl, dl - 0.2, lt);
        return unshadowed - u.light_col[best_i].rgb * best_w * (1.0 - vis);
    }
    return shadowed;
}

/// Éclairage direct de la lampe torche au point p (1 rayon d'ombre déterministe).
fn torch_direct(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    if (u.flash_col.w <= 0.001) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    let to = p - u.flash_pos.xyz;
    let d = max(length(to), 0.001);
    let dir = to / d;
    let cos_a = dot(dir, normalize(u.flash_dir.xyz));
    if (cos_a <= u.misc.w - 0.02 || d >= 15.0) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    let spot = smoothstep(u.misc.w, u.flash_dir.w, cos_a);
    let nl = max(dot(n, -dir), 0.0);
    let att = clamp(1.0 - d / 15.0, 0.0, 1.0);
    let vis = shadow_ray(p + n * 0.012, -dir, d - 0.1);
    return u.flash_col.rgb * u.flash_col.w * (spot * nl * att * att * 1.7) * vis;
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
        let att = 1.0 - clamp(dist / max(lp.w, 0.001), 0.0, 1.0); // linéaire bornée (l'ancien smoothstep à bords inversés explosait au loin -> inf en f16)
        let w = lc.w * att * att * max(dot(n, to / max(dist, 0.001)), 0.0);
        if (w <= 0.004) { continue; }
        // Jitter stable par pixel/lumière : pénombre sans scintillement.
        // ±0.12 max : sous un plafond à 3.0 m (lumière à 2.85), un jitter
        // plus large ferait piquer les rayons dans la dalle → néon à moitié
        // auto-ombré = halos disparus.
        let j = (hash3(vec3<f32>(f32(px.x), f32(px.y), f32(i) * 7.31)) - vec3<f32>(0.5)) * 0.24;
        let lt = lp.xyz + j;
        let vis = shadow_ray_light(p + n * 0.012, normalize(lt - p), distance(p, lt) - 0.2, lt);
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

    // ---- AO (1 rayon en Qualité, 3 en Ultra/Overdrive) ----
    // Plancher de lisibilité : l'AO ne descend jamais sous 0.45 — avant, une
    // scène sans lumière partait au noir absolu (« on voit rien »).
    let ao_radius = rp.counts.w;
    let ao_samples = u32(max(rp.misc.z, 1.0));
    var ao_acc = 0.0;
    for (var s = 0u; s < ao_samples; s = s + 1u) {
        let u1 = hash1(vec3<f32>(f32(px.x) * 1.13, f32(px.y) * 1.27, 3.1 + f32(s) * 17.73));
        let u2 = hash1(vec3<f32>(f32(px.x) * 1.71, f32(px.y) * 1.93, 9.4 + f32(s) * 23.31));
        let dir_w = cosine_hemi(n, u1, u2);
        var hn = vec3<f32>(0.0, 1.0, 0.0);
        let t = trace(p + n * 0.012, dir_w, ao_radius, &hn);
        if (t >= 0.0) {
            let occ = clamp(t / ao_radius, 0.0, 1.0);
            ao_acc = ao_acc + occ * occ; // rapproche le profil d'un AO multi-échantillons
        } else {
            ao_acc = ao_acc + 1.0; // rayon perdu : pas d'occlusion
        }
    }
    var ao = ao_acc / f32(ao_samples);
    ao = 0.45 + 0.55 * ao;

    // ---- GI : PATH TRACING Monte Carlo + next event estimation ----
    // De VRAIS chemins de lumière : rebonds cosinus sur l'albedo des surfaces,
    // et à CHAQUE point touché l'éclairage direct des néons + de la torche est
    // échantillonné AVEC ombres portées — plus aucune fuite de lumière à travers
    // les murs (l'ancien rebond éclairait à travers). La lumière des néons
    // coule, rebondit du sol au plafond et colore le couloir : c'est le même
    // algorithme que le mode path tracing de Cyberpunk 2077 (peu d'échantillons
    // par pixel, lissés par la résolution réduite + RCAS + brouillard).
    var gi = vec3<f32>(0.0, 0.0, 0.0);
    if (rp.misc.x > 0.001) {
        let gi_samples = u32(max(rp.pt.x, 1.0));
        let bounces = u32(max(rp.pt.y, 1.0));
        let budget = u32(max(rp.pt.z, 0.0));
        for (var s = 0u; s < gi_samples; s = s + 1u) {
            var sp = p + n * 0.012;
            var sn = n;
            var beta = vec3<f32>(1.0, 1.0, 1.0);
            for (var b = 0u; b < bounces; b = b + 1u) {
                let salt = s * 37u + b * 101u;
                let u1 = hash1(vec3<f32>(f32(px.x) * 1.13 + f32(salt) * 5.71, f32(px.y) * 1.27 + f32(salt) * 9.13, 3.1 + f32(salt)));
                let u2 = hash1(vec3<f32>(f32(px.x) * 1.71 + f32(salt) * 3.37, f32(px.y) * 1.93 + f32(salt) * 7.77, 9.4 + f32(salt)));
                let dir_w = cosine_hemi(sn, u1, u2);
                var hn = vec3<f32>(0.0, 1.0, 0.0);
                let t = trace(sp, dir_w, 34.0, &hn);
                if (t < 0.0 || t != t) {
                    break; // échappé OU NaN (t != t) : on arrête le chemin
                }
                let hp = sp + dir_w * t;
                let alb = surface_albedo(hn, hp);
                gi = gi + beta * alb * (neon_direct(hp + hn * 0.012, hn, budget, px, salt) + torch_direct(hp + hn * 0.012, hn));
                beta = beta * alb;
                if (max(beta.x, max(beta.y, beta.z)) < 0.02) {
                    break; // roulette russe manuelle : le chemin ne contribue plus
                }
                sp = hp + hn * 0.012;
                sn = hn;
            }
        }
        gi = gi * rp.misc.x * 1.2 / f32(gi_samples);
    }

    // ---- Réflexions : PLEINE SCÈNE + néons sur sols polis + Fresnel ----
    // Le rayon miroir trace VRAIMENT la scène : le couloir (murs, portes,
    // baies serveurs) se reflète dans le lino ciré, la lumière dominante et
    // la torche sont ré-éclairées au point réfléchi — le « wow » du RT.
    var refl = vec3<f32>(0.0, 0.0, 0.0);
    let vdir = normalize(p - rp.cam_pos.xyz);
    let ndv = max(dot(n, -vdir), 0.0);
    let fres = 0.04 + 0.96 * pow(1.0 - ndv, 5.0); // Schlick
    let polished = select(0.0, 1.0, n.y > 0.5);   // sols (lino ciré) : reflets forts
    let rk = clamp(polished * 0.85 + fres, 0.0, 1.0);
    if (rk > 0.03) {
        let rdir = reflect(vdir, n);
        let ro2 = p + n * 0.012;

        // 1) Réflexion pleine scène : on trace le rayon miroir contre tous les
        //    AABB et on ré-éclaire le point touché (ambiance + néons + torche).
        var hn2 = vec3<f32>(0.0, 1.0, 0.0);
        let tr = trace(ro2, rdir, 34.0, &hn2);
        if (tr >= 0.0) {
            let hp = ro2 + rdir * tr;
            // Le lieu réfléchi est ré-éclairé : ambiance + néons (NEE avec
            // l'ombre de la lumière dominante) + torche avec son rayon d'ombre.
            let lit = mix(vec3<f32>(0.035, 0.04, 0.055), vec3<f32>(0.16, 0.165, 0.18), u.mood.x)
                + neon_direct(hp + hn2 * 0.012, hn2, 1u, px, 7u)
                + torch_direct(hp + hn2 * 0.012, hn2);
            // Fondu avec la distance : le reflet s'évanouit dans le brouillard.
            let fade = exp(-0.09 * tr);
            refl = refl + lit * 0.28 * fade * rk;
        }

        // 2) Reflets spéculaires des néons (sphères émissives pendants) —
        //    cône gloss resserré sur les sols : des reflets lisibles et
        //    orientés vers les luminaires, pas un quadrillage de traînées
        //    blanches qui ressemble à des « textures cassées ».
        let cone = select(0.965, 0.96, n.y > 0.5);
        var spec = vec3<f32>(0.0, 0.0, 0.0);
        for (var i = 0u; i < 24u; i = i + 1u) {
            if (i >= nl_count) { break; }
            let lp = u.light_pos[i];
            let lc = u.light_col[i];
            if (lc.w <= 0.01) { continue; }
            // Le néon pend juste sous le plafond : sphère émissive ~0.45 m.
            let sc = lp.xyz - vec3<f32>(0.0, 0.18, 0.0);
            let to = sc - ro2;
            let ds = length(to);
            if (ds > 26.0 || ds < 0.3) { continue; }
            let ldir = to / ds;
            let align = dot(ldir, rdir);
            if (align < cone) { continue; } // cône gloss autour du rayon miroir
            // Occlusion : un mur entre le sol et le néon casse le reflet
            // (on ignore la boîte du luminaire : auto-ombre sinon).
            if (shadow_ray_light(ro2, ldir, ds - 0.5, sc) < 0.5) { continue; }
            let glow = lc.rgb * lc.w * (1.0 - clamp(ds / 26.0, 0.0, 1.0) * 0.65);
            spec = spec + glow * smoothstep(cone, 1.0, align);
        }
        refl = refl + spec * 0.9;
    }

    o.out0 = vec4<f32>(ao, sh_static, sh_flash, 1.0);
    o.out1 = vec4<f32>(gi + refl, 1.0);
    return o;
}
