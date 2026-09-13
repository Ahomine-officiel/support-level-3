// Pipeline monde : éclairage (néons + lampe torche), émissifs, brouillard.

struct Uniforms {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    light_pos: array<vec4<f32>, 24>,
    light_col: array<vec4<f32>, 24>,
    flash_pos: vec4<f32>,
    flash_dir: vec4<f32>,
    misc: vec4<f32>,
    flash_col: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
// Résultat de la passe de ray tracing (optionnelle) :
//   rt0 = (ao, ombre néons, ombre torche, 1) · rt1 = (gi.rgb, 1)
// Hors RT : textures 1x1 neutres (ao=1, ombres=1, gi=0) -> rendu identique.
@group(0) @binding(1) var rt_samp: sampler;
@group(0) @binding(2) var rt0_tex: texture_2d<f32>;
@group(0) @binding(3) var rt1_tex: texture_2d<f32>;

struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct InstanceInput {
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) m3: vec4<f32>,
    @location(7) emissive: vec4<f32>,
    @location(8) tint: vec4<f32>,
};

struct VSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) wpos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) emis: vec4<f32>,
    @location(4) tint: vec4<f32>,
};

@vertex
fn vs(in: VertexInput, inst: InstanceInput) -> VSOut {
    let m = mat4x4<f32>(inst.m0, inst.m1, inst.m2, inst.m3);
    let wp = m * vec4<f32>(in.pos, 1.0);
    var out: VSOut;
    out.clip = u.view_proj * wp;
    out.wpos = wp.xyz;
    out.nrm = normalize((m * vec4<f32>(in.nrm, 0.0)).xyz);
    out.uv = in.uv;
    out.emis = inst.emissive;
    out.tint = inst.tint;
    return out;
}

@group(1) @binding(0) var samp: sampler;
@group(1) @binding(1) var tex: texture_2d<f32>;
@group(1) @binding(2) var<uniform> mat_u: vec4<f32>;

const FOG_COLOR = vec3<f32>(0.012, 0.016, 0.026);

@fragment
fn fs(in: VSOut) -> @location(0) vec4<f32> {
    let tex4 = textureSample(tex, samp, in.uv);
    let albedo = tex4.rgb * clamp(in.tint.rgb, vec3<f32>(0.0), vec3<f32>(2.0));
    let n = normalize(in.nrm);

    // Résultat RT échantillonné à l'écran (projection de la position monde).
    let sp = u.view_proj * vec4<f32>(in.wpos, 1.0);
    let suv = sp.xy / max(sp.w, 1e-4) * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    let rt0 = textureSampleLevel(rt0_tex, rt_samp, suv, 0.0);
    let rt1 = textureSampleLevel(rt1_tex, rt_samp, suv, 0.0);
    let ao = rt0.r;
    let sh = rt0.g;
    let shf = rt0.b;
    let gi = rt1.rgb;

    var light = vec3<f32>(0.035, 0.04, 0.055) * ao;
    let nl_count = u32(u.misc.x);
    for (var i: u32 = 0u; i < 24u; i = i + 1u) {
        if (i >= nl_count) { break; }
        let lp = u.light_pos[i];
        let lc = u.light_col[i];
        let to = lp.xyz - in.wpos;
        let d = max(length(to), 0.001);
        let att = smoothstep(lp.w, lp.w * 0.2, d);
        let nl = max(dot(n, to / d), 0.0);
        light = light + lc.rgb * lc.w * nl * att * att * sh * mix(1.0, ao, 0.35);
    }

    // Lampe torche (spot attaché à la caméra).
    if (u.flash_col.w > 0.001) {
        let to = in.wpos - u.flash_pos.xyz;
        let d = max(length(to), 0.001);
        let dir = to / d;
        let cos_a = dot(dir, normalize(u.flash_dir.xyz));
        let spot = smoothstep(u.misc.w, u.flash_dir.w, cos_a);
        let nl = max(dot(n, -dir), 0.0);
        let att = clamp(1.0 - d / 15.0, 0.0, 1.0);
        light = light + u.flash_col.rgb * u.flash_col.w * (spot * nl * att * att * 1.7 + 0.008) * shf;
    }

    let emis = tex4.rgb * mat_u.w * in.emis.a * in.emis.rgb;
    var color = albedo * light + albedo * gi + max(emis, vec3<f32>(0.0));

    let dist = length(in.wpos - u.cam_pos.xyz);
    let f = u.misc.z * dist;
    let fog = 1.0 - exp(-f * f);
    color = mix(color, FOG_COLOR, fog);

    return vec4<f32>(color, 1.0);
}
