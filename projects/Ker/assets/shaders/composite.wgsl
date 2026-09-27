// 最终合成：场景 × 光照（天空光×环境系数 / 方块光取最大）+ 夜色偏蓝 + Bloom 叠加 + ACES 色调映射
struct Params {
    topleft: vec2<f32>,
    viewport: vec2<f32>,
    inv_tiles: vec2<f32>,
    ambient: f32,
    // uniform 对齐：vec4 从 float 8 起（_pad.x=曝光, _pad.y=保留, _pad.z=保留, _pad.w=bloom 强度）
    _pad: vec4<f32>,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var scene_tex: texture_2d<f32>;
@group(0) @binding(2) var scene_samp: sampler;
@group(0) @binding(3) var light_tex: texture_2d<f32>;
@group(0) @binding(4) var light_samp: sampler;
@group(0) @binding(5) var bloom_tex: texture_2d<f32>;
@group(0) @binding(6) var bloom_samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// ACES 色调映射（Narkowicz 近似）：把线性 HDR 值压缩到显示范围，高光柔化不发白
fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VOut {
    var pts = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let xy = pts[i];
    var o: VOut;
    o.pos = vec4<f32>(xy, 0.0, 1.0);
    o.uv = vec2<f32>((xy.x + 1.0) * 0.5, (1.0 - xy.y) * 0.5);
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let s = textureSample(scene_tex, scene_samp, in.uv);
    let world = p.topleft + in.uv * p.viewport;
    let l = textureSampleLevel(light_tex, light_samp, world * p.inv_tiles, 0.0);
    let sky = l.r * p.ambient;
    var lum = max(sky, l.g);
    lum = max(lum, 0.075);
    var col = s.rgb * lum;
    col = mix(col * vec3<f32>(0.72, 0.82, 1.28), col, clamp(p.ambient * 1.5, 0.0, 1.0));
    // Bloom 辉光叠加（强度可调，0 关闭）
    col += textureSample(bloom_tex, bloom_samp, in.uv).rgb * p._pad.w;
    // 色调映射：曝光系数（_pad.x，默认 1.0）→ ACES 压缩高光
    col = aces(col * p._pad.x);
    return vec4<f32>(col, s.a);
}
