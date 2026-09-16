// Post-process « vision de panique » : distorsion, grain, vignette, rouge downed.

struct PostU {
    fear: f32,
    time: f32,
    downed: f32,
    chase: f32,
};
@group(0) @binding(0) var<uniform> p: PostU;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex: texture_2d<f32>;

struct VSOut {
    @builtin(position) clip: vec4<f32>,
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
    out.clip = vec4<f32>(pos[vi], 0.0, 1.0);
    out.uv = vec2<f32>((pos[vi].x + 1.0) * 0.5, 1.0 - (pos[vi].y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs(in: VSOut) -> @location(0) vec4<f32> {
    var uv = in.uv;
    let c = uv - vec2<f32>(0.5, 0.5);

    // Distorsion panique (barrel + ondulation).
    let r2 = dot(c, c);
    uv = uv + c * r2 * p.fear * 0.4;
    uv = uv + vec2<f32>(
        sin(uv.y * 38.0 + p.time * 9.0),
        cos(uv.x * 34.0 + p.time * 7.5),
    ) * 0.0035 * p.fear;

    // Aberration chromatique.
    let ab = 0.005 * p.fear;
    var col = vec3<f32>(
        textureSample(tex, samp, uv + vec2<f32>(ab, 0.0)).r,
        textureSample(tex, samp, uv).g,
        textureSample(tex, samp, uv - vec2<f32>(ab, 0.0)).b,
    );

    // Grain filmique — quasi invisible au repos (début de partie lisible,
    // pas du « bruit blanc ») et ne monte réellement qu'avec la peur.
    let g = fract(sin(dot(uv * (p.time + 3.0), vec2<f32>(12.9898, 78.233))) * 43758.5453);
    col = col + (g - 0.5) * (0.008 + 0.035 * p.fear);

    // Vignette.
    let vig = smoothstep(0.92, 0.3, length(c));
    col = col * mix(0.32, 1.0, vig);

    // Peur : désaturation + assombrissement.
    let grey = vec3<f32>(dot(col, vec3<f32>(0.333)));
    col = mix(col, grey, p.fear * 0.35);
    col = col * (1.0 - 0.22 * p.fear);

    // Poursuite : teinte rouge.
    col = mix(col, col * vec3<f32>(1.35, 0.55, 0.55), p.chase * 0.55);

    // Au sol : écran rouge sombre.
    col = mix(col, vec3<f32>(0.28, 0.015, 0.02), p.downed * 0.65);

    return vec4<f32>(col, 1.0);
}
