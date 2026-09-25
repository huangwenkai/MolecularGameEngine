// Bloom 后处理：亮部提取 + 可分离高斯模糊（方向由 uniform 提供）
struct Params {
    texel: vec2<f32>,
    dir: vec2<f32>, // 亮部提取 pass 忽略
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
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

// 亮部提取（软阈值）
@fragment
fn fs_bright(in: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(src_tex, src_samp, in.uv);
    let lum = max(c.r, max(c.g, c.b));
    let k = smoothstep(0.55, 1.0, lum);
    return vec4<f32>(c.rgb * k, 1.0);
}

// 高斯模糊（5 tap 对称 ×2 宽距，手动展开：循环 + 动态数组索引会触发 D3DCompile 崩溃）
@fragment
fn fs_blur(in: VOut) -> @location(0) vec4<f32> {
    let base = textureSample(src_tex, src_samp, in.uv).rgb * 0.227;
    let off1 = p.dir * p.texel * 1.5;
    let off2 = p.dir * p.texel * 3.0;
    let off3 = p.dir * p.texel * 4.5;
    let off4 = p.dir * p.texel * 6.0;
    var acc = base;
    acc += textureSample(src_tex, src_samp, in.uv + off1).rgb * 0.194;
    acc += textureSample(src_tex, src_samp, in.uv - off1).rgb * 0.194;
    acc += textureSample(src_tex, src_samp, in.uv + off2).rgb * 0.121;
    acc += textureSample(src_tex, src_samp, in.uv - off2).rgb * 0.121;
    acc += textureSample(src_tex, src_samp, in.uv + off3).rgb * 0.054;
    acc += textureSample(src_tex, src_samp, in.uv - off3).rgb * 0.054;
    acc += textureSample(src_tex, src_samp, in.uv + off4).rgb * 0.016;
    acc += textureSample(src_tex, src_samp, in.uv - off4).rgb * 0.016;
    return vec4<f32>(acc, 1.0);
}
