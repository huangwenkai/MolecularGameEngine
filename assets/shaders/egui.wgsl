// egui mesh 管线：屏幕空间 UI 渲染（预乘 RGBA 纹理 × 顶点色）
struct Screen {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var smp: sampler;
@group(1) @binding(1) var tex: texture_2d<f32>;

struct VIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) col: vec4<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) col: vec4<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    let ndc = v.pos / screen.size * 2.0 - 1.0;
    o.pos = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    o.uv = v.uv;
    o.col = v.col;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, smp, i.uv);
    return i.col * t;
}
