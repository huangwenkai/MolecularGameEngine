//! 植被：数据驱动的地表装饰生成（背景层像素，与树同层）
//! 定义表 assets/data/vegetation.ron（植被编辑器读写 + 热重载），材质引用 materials.ron 中的名称
use crate::gen::put_shade_if_empty;
use mge_core::rng::Rng;
use mge_sim::{Materials, Pixel, PixelWorld};
use serde::{Deserialize, Serialize};

/// 植被形态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VegKind {
    /// 草丛：数根细叶
    Grass,
    /// 花：茎 + 花头
    Flower,
    /// 灌木：小型树冠
    Bush,
    /// 蘑菇：菌柄 + 伞盖
    Mushroom,
    /// 仙人掌：柱身 + 侧臂
    Cactus,
}

/// 单种植被定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlantDef {
    pub name: String,
    /// 生物群系名（forest / snow / desert；空 = 全部）
    #[serde(default)]
    pub biomes: Vec<String>,
    /// 可生成的地表材质名（空 = 任意实心表面）
    #[serde(default)]
    pub ground: Vec<String>,
    /// 每列生成概率 0..1
    #[serde(default = "dflt_density")]
    pub density: f32,
    /// 同种植被最小间距 px
    #[serde(default)]
    pub gap: i32,
    pub kind: VegKind,
    /// 主体材质（草叶/花瓣/树冠/伞盖/柱身）
    pub body: String,
    /// 茎干材质（花/蘑菇用；None = 主体材质）
    #[serde(default)]
    pub stem: Option<String>,
    /// 高度范围 px（含）
    #[serde(default = "dflt_h")]
    pub h: (i32, i32),
    /// 宽度范围 px（含）
    #[serde(default = "dflt_w")]
    pub w: (i32, i32),
}

fn dflt_density() -> f32 {
    0.05
}
fn dflt_h() -> (i32, i32) {
    (4, 8)
}
fn dflt_w() -> (i32, i32) {
    (2, 4)
}

/// 植被定义文件（vegetation.ron）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VegFile {
    pub plants: Vec<PlantDef>,
}

impl VegFile {
    /// 编译期嵌入的默认表（文件缺失/损坏时的兜底）
    pub fn embedded() -> Self {
        ron::from_str(include_str!("../../../assets/data/vegetation.ron"))
            .expect("embedded vegetation.ron invalid")
    }

    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| format!("vegetation.ron parse error: {e}"))
    }
}

/// 生物群系 id → 名称（与 gen::biome_at 一致：0 雪原 / 1 森林 / 2 沙漠）
fn biome_name(b: u8) -> Option<&'static str> {
    match b {
        0 => Some("snow"),
        1 => Some("forest"),
        2 => Some("desert"),
        _ => None,
    }
}

/// 解析定义引用的全部主体/茎干材质 id（背景标记、重新生长前清除用）
pub fn resolve_mats(defs: &[PlantDef], mats: &Materials) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for def in defs {
        let mut push = |name: &str| {
            if let Some(id) = mats.id(name) {
                if id != 0 && !out.contains(&id) {
                    out.push(id);
                }
            }
        };
        push(&def.body);
        if let Some(s) = &def.stem {
            push(s);
        }
    }
    out
}

/// 扫描当前地表高度（每列第一行实心像素；树/植被等非实心背景像素被跳过）
pub fn current_surf(pixels: &PixelWorld, mats: &Materials) -> Vec<i32> {
    (0..pixels.w)
        .map(|x| {
            (0..pixels.h)
                .find(|&y| mats.def(pixels.get(x, y).mat).solid)
                .unwrap_or(pixels.h / 2)
        })
        .collect()
}

/// 清除全部植被像素（重新生长前调用；只扫地表上方，避免全图扫描）
pub fn clear(pixels: &mut PixelWorld, ids: &[u8], surf: &[i32]) {
    if ids.is_empty() {
        return;
    }
    let ymax = (surf.iter().copied().max().unwrap_or(pixels.h / 2) + 64).min(pixels.h);
    for y in 0..ymax {
        for x in 0..pixels.w {
            if ids.contains(&pixels.get(x, y).mat) {
                pixels.set(x, y, Pixel::default());
            }
        }
    }
}

/// 在地表生成植被（背景层像素）。返回用到的材质 id。
/// biome(x) 返回 gen::biome_at 的 id（0 雪原 / 1 森林 / 2 沙漠）
pub fn grow(
    pixels: &mut PixelWorld,
    mats: &Materials,
    defs: &[PlantDef],
    surf: &[i32],
    biome: impl Fn(i32) -> u8,
    rng: &mut Rng,
) -> Vec<u8> {
    let (w, h) = (pixels.w, pixels.h);
    // 解析材质 id（名称无法解析的定义跳过）
    let mut resolved: Vec<(usize, u8, u8)> = Vec::new(); // (def 序号, 主体, 茎干)
    let mut used: Vec<u8> = Vec::new();
    for (i, def) in defs.iter().enumerate() {
        let Some(body) = mats.id(&def.body).filter(|&id| id != 0) else {
            continue;
        };
        let stem = def.stem.as_deref().and_then(|s| mats.id(s)).unwrap_or(0);
        resolved.push((i, body, stem));
        for id in [body, stem] {
            if id != 0 && !used.contains(&id) {
                used.push(id);
            }
        }
    }
    if resolved.is_empty() {
        return used;
    }
    let mut last = vec![-9999i32; defs.len()];
    for x in 2..w - 2 {
        let s = surf[(x as usize).min(surf.len() - 1)];
        if s <= 1 || s + 1 >= h {
            continue;
        }
        let b = biome(x);
        let ground_mat = pixels.get(x, s).mat;
        for &(di, body, stem) in &resolved {
            let def = &defs[di];
            if x - last[di] < def.gap.max(0) {
                continue;
            }
            // 生物群系过滤
            if !def.biomes.is_empty() && !def.biomes.iter().any(|n| biome_name(b) == Some(n.as_str())) {
                continue;
            }
            // 地表材质过滤
            if !def.ground.is_empty() {
                if !mats.def(ground_mat).solid {
                    continue;
                }
                if !def.ground.iter().any(|n| mats.id(n) == Some(ground_mat)) {
                    continue;
                }
            }
            if !rng.chance(def.density.max(0.0)) {
                continue;
            }
            last[di] = x;
            let (h0, h1) = (def.h.0.min(def.h.1), def.h.0.max(def.h.1));
            let (w0, w1) = (def.w.0.min(def.w.1), def.w.0.max(def.w.1));
            let hh = rng.range_i32(h0.max(1), h1.max(1));
            let ww = rng.range_i32(w0.max(1), w1.max(1));
            draw_plant(pixels, rng, x, s, def.kind, body, stem, hh, ww);
        }
    }
    used
}

/// 按形态绘制一株植被（仅写入空像素，不覆盖地形）
fn draw_plant(
    pixels: &mut PixelWorld,
    rng: &mut Rng,
    x: i32,
    s: i32,
    kind: VegKind,
    body: u8,
    stem: u8,
    h: i32,
    w: i32,
) {
    match kind {
        VegKind::Grass => {
            // 数根细叶，顶部随机偏折
            for i in 0..w {
                let bx = x + i - w / 2;
                let bh = (h + rng.range_i32(-1, 1)).max(2);
                let lean = rng.range_i32(-1, 1);
                for dy in 0..bh {
                    let dx = if dy >= bh / 2 { lean } else { 0 };
                    put_shade_if_empty(pixels, bx + dx, s - 1 - dy, body, 118 + rng.range_i32(-18, 18));
                }
            }
        }
        VegKind::Flower => {
            // 茎 + 十字形花头
            let stem_mat = if stem != 0 { stem } else { body };
            for dy in 0..h {
                put_shade_if_empty(pixels, x, s - 1 - dy, stem_mat, 108 + rng.range_i32(-10, 10));
            }
            let top = s - h;
            put_shade_if_empty(pixels, x, top - 1, body, 130 + rng.range_i32(-14, 14));
            put_shade_if_empty(pixels, x - 1, top - 1, body, 118 + rng.range_i32(-14, 14));
            put_shade_if_empty(pixels, x + 1, top - 1, body, 118 + rng.range_i32(-14, 14));
            put_shade_if_empty(pixels, x, top - 2, body, 142 + rng.range_i32(-14, 14));
        }
        VegKind::Bush => {
            // 小型树冠（锯齿椭圆，下暗上亮）
            let rx = (w / 2).max(2);
            let ry = (h / 2).max(1);
            let cy = s - 1 - ry;
            for dy in -ry..=ry {
                for dx in -rx..=rx {
                    let d2 = (dx * dx) as f32 / (rx * rx) as f32 + (dy * dy) as f32 / (ry * ry) as f32;
                    if d2 > 1.0 {
                        continue;
                    }
                    if d2 > 0.4 && rng.chance((d2 - 0.4) * 1.6) {
                        continue;
                    }
                    let shade = 118 - dy * 10 + rng.range_i32(-14, 14);
                    put_shade_if_empty(pixels, x + dx, cy + dy, body, shade);
                }
            }
        }
        VegKind::Mushroom => {
            // 菌柄 + 扁椭圆伞盖
            let stem_mat = if stem != 0 { stem } else { body };
            let sh = (h * 2 / 3).max(2);
            for dy in 0..sh {
                put_shade_if_empty(pixels, x, s - 1 - dy, stem_mat, 122 + rng.range_i32(-8, 8));
            }
            let cap_cy = s - sh;
            let rx = w.max(1);
            let ry = (h / 3).max(1);
            for dy in -ry..=0 {
                for dx in -rx..=rx {
                    let d2 = (dx * dx) as f32 / (rx * rx) as f32 + (dy * dy) as f32 / (ry * ry) as f32;
                    if d2 <= 1.0 {
                        let shade = 112 - dy * 8 + rng.range_i32(-10, 10);
                        put_shade_if_empty(pixels, x + dx, cap_cy + dy, body, shade);
                    }
                }
            }
        }
        VegKind::Cactus => {
            // 柱身 + 1~2 条侧臂
            for dy in 0..h {
                for dx in 0..w {
                    put_shade_if_empty(pixels, x - w / 2 + dx, s - 1 - dy, body, 112 + rng.range_i32(-10, 10));
                }
            }
            for i in 0..rng.range_i32(1, 2) {
                let dir = if i == 0 { 1 } else { -1 };
                let ay = s - 1 - rng.range_i32(h * 2 / 5, h * 3 / 5).max(2);
                let arm_h = rng.range_i32(3, 6);
                for k in 1..=2 {
                    put_shade_if_empty(pixels, x + dir * (w / 2 + k), ay, body, 108 + rng.range_i32(-8, 8));
                }
                for k in 0..arm_h {
                    put_shade_if_empty(pixels, x + dir * (w / 2 + 2), ay - 1 - k, body, 108 + rng.range_i32(-8, 8));
                }
            }
        }
    }
}
