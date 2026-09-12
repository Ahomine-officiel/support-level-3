// Pipeline UI : quads ortho 2D (texte bitmap + rectangles colorés).

struct UiU {
    screen: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: UiU;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs(in: VertexInput) -> VSOut {
    var out: VSOut;
    let p = vec2<f32>(
        in.pos.x / u.screen.x * 2.0 - 1.0,
        1.0 - in.pos.y / u.screen.y * 2.0,
    );
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@group(1) @binding(0) var samp: sampler;
@group(1) @binding(1) var tex: texture_2d<f32>;

@fragment
fn fs(in: VSOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    let a = t.a * in.color.a;
    return vec4<f32>(in.color.rgb * t.rgb * a, a);
}
