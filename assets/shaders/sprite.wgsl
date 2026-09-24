struct Camera {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;

struct VIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(in: VIn) -> VOut {
    var o: VOut;
    o.pos = camera.view_proj * vec4<f32>(in.pos, 0.0, 1.0);
    o.uv = in.uv;
    o.color = in.color;
    return o;
}

@group(1) @binding(0) var samp: sampler;
@group(1) @binding(1) var tex: texture_2d<f32>;

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    return c * in.color;
}
