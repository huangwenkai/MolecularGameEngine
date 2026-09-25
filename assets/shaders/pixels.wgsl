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
    // 材质字节最高位 = 背景标记（树等背景像素）：前景通道跳过，由 fs_bg 绘制
    let b = m.r * 255.0;
    if (b >= 127.5) {
        discard;
    }
    let idx = u32(b + 0.5);
    let base = textureSampleLevel(palette, psamp, vec2<f32>((f32(idx) + 0.5) / 256.0, 0.5), 0.0);
    let shade = 0.75 + 0.5 * m.g;
    return vec4<f32>(base.rgb * shade, base.a) * in.color;
}

/// 背景像素通道：只绘制带背景标记的像素（树干/树叶/浆果丛/绳索）
/// 角色/实体在其之前绘制 → 树永远在角色身后，不再遮挡角色
@fragment
fn fs_bg(in: VOut) -> @location(0) vec4<f32> {
    let m = textureSample(tex, samp, in.uv);
    let b = m.r * 255.0;
    if (b < 127.5) {
        discard;
    }
    let idx = u32(b - 127.5);
    let base = textureSampleLevel(palette, psamp, vec2<f32>((f32(idx) + 0.5) / 256.0, 0.5), 0.0);
    let shade = 0.75 + 0.5 * m.g;
    return vec4<f32>(base.rgb * shade, base.a) * in.color;
}

/// 背景墙通道：RG 纹理（材质 + 明度），4px/格，压暗渲染形成洞穴/地下背景
@fragment
fn fs_walls(in: VOut) -> @location(0) vec4<f32> {
    let m = textureSample(tex, samp, in.uv);
    let b = m.r * 255.0;
    if (b < 0.5) {
        discard;
    }
    let idx = u32(b + 0.5);
    let base = textureSampleLevel(palette, psamp, vec2<f32>((f32(idx) + 0.5) / 256.0, 0.5), 0.0);
    let shade = 0.75 + 0.5 * m.g;
    // 墙体压暗（泰拉瑞亚式背景层次），光照仍由 composite 统一叠加
    return vec4<f32>(base.rgb * shade * 0.65, base.a) * in.color;
}
