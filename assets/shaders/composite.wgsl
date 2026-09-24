// 最终合成：场景 × 光照（天空光×环境系数 / 方块光取最大）+ 夜色偏蓝
struct Params {
    topleft: vec2<f32>,
    viewport: vec2<f32>,
    inv_tiles: vec2<f32>,
    ambient: f32,
    _pad: vec4<f32>,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var scene_tex: texture_2d<f32>;
@group(0) @binding(2) var scene_samp: sampler;
@group(0) @binding(3) var light_tex: texture_2d<f32>;
@group(0) @binding(4) var light_samp: sampler;

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

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let s = textureSample(scene_tex, scene_samp, in.uv);
    let world = p.topleft + in.uv * p.viewport;
    let l = textureSampleLevel(light_tex, light_samp, world * p.inv_tiles, 0.0);
    let sky = l.r * p.ambient;
    var lum = max(sky, l.g);
    lum = max(lum, 0.045);
    var col = s.rgb * lum;
    col = mix(col * vec3<f32>(0.72, 0.82, 1.28), col, clamp(p.ambient * 1.5, 0.0, 1.0));
    return vec4<f32>(col, s.a);
}
