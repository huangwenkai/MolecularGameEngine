// 像素世界渲染：RG8（材质id + 明度）→ 调色板展开
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
@group(1) @binding(2) var psamp: sampler;
@group(1) @binding(3) var palette: texture_2d<f32>;

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let m = textureSample(tex, samp, in.uv);
    let idx = u32(m.r * 255.0 + 0.5);
    let base = textureSampleLevel(palette, psamp, vec2<f32>((f32(idx) + 0.5) / 256.0, 0.5), 0.0);
    let shade = 0.75 + 0.5 * m.g;
    return vec4<f32>(base.rgb * shade, base.a) * in.color;
}
