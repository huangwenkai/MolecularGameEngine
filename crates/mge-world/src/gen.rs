//! 程序化世界生成：像素级地形（Noita 式）
//! 洞穴噪声按 4px 网格采样后成块雕刻（噪声本身低频，结果与逐像素采样一致）
use mge_core::rng::Rng;
use mge_sim::{Materials, Pixel, PixelWorld};
use noise::{Fbm, NoiseFn, Perlin};

pub struct GenResult {
    pub spawn_x: i32,
    pub spawn_y: i32,
}

/// 写入一个带颜色抖动的地形像素（无条件覆盖）
fn put(pixels: &mut PixelWorld, mats: &Materials, rng: &mut Rng, x: i32, y: i32, mat: u8) {
    if mat == 0 || x < 0 || y < 0 || x >= pixels.w || y >= pixels.h {
        return;
    }
    let j = mats.def(mat).jitter as i32;
    let shade = (127 + rng.range_i32(-j, j)).clamp(0, 255) as u8;
    pixels.set(x, y, Pixel { mat, shade, life: 0, aux: 0 });
}

/// 清空一个像素
fn clear(pixels: &mut PixelWorld, x: i32, y: i32) {
    pixels.set(x, y, Pixel::default());
}

/// 仅当为空时写入（松散层/液体用）
fn put_if_empty(pixels: &mut PixelWorld, mats: &Materials, rng: &mut Rng, x: i32, y: i32, mat: u8) {
    if mat != 0 && pixels.get(x, y).mat == 0 {
        put(pixels, mats, rng, x, y, mat);
    }
}

/// 仅空位写入指定明度的像素（树/装饰用，明度 0..=255，128 为标准亮度）
fn put_shade_if_empty(pixels: &mut PixelWorld, x: i32, y: i32, mat: u8, shade: i32) {
    if mat == 0 || x < 0 || y < 0 || x >= pixels.w || y >= pixels.h {
        return;
    }
    if pixels.get(x, y).mat != 0 {
        return;
    }
    pixels.set(x, y, Pixel { mat, shade: shade.clamp(0, 255) as u8, life: 0, aux: 0 });
}

/// 椭圆树冠：边缘噪声锯齿，下暗上亮（背景层叶子，有体积感）
fn canopy(pixels: &mut PixelWorld, rng: &mut Rng, cx: i32, cy: i32, rx: i32, ry: i32, leaf: u8) {
    if leaf == 0 || rx <= 0 || ry <= 0 {
        return;
    }
    for dy in (-ry - 1)..=(ry + 1) {
        for dx in (-rx - 1)..=(rx + 1) {
            let d2 = (dx * dx) as f32 / (rx * rx) as f32 + (dy * dy) as f32 / (ry * ry) as f32;
            if d2 > 1.0 {
                continue;
            }
            // 边缘随机锯齿，避免"完美椭圆"的假感
            if d2 > 0.5 && rng.chance((d2 - 0.5) * 1.7) {
                continue;
            }
            let shade = 118 - dy * 9 + rng.range_i32(-14, 14);
            put_shade_if_empty(pixels, cx + dx, cy + dy, leaf, shade);
        }
    }
}

/// 阔叶树（森林）：弯曲渐细树干 + 根部外扩 + 侧枝 + 层叠树冠
fn gen_oak(pixels: &mut PixelWorld, rng: &mut Rng, x: i32, s: i32, wood: u8, leaf: u8) {
    let hgt = rng.range_i32(46, 74);
    let lean = rng.range_i32(-5, 5);
    // 树干：底部 4px 渐细至 2px，微弯，边缘暗中心亮（树皮立体感）
    for dy in 1..=hgt {
        let t = dy as f32 / hgt as f32;
        let cx = x + (lean as f32 * t * t).round() as i32;
        let wdt = if dy <= 4 { 4 } else if t > 0.75 { 2 } else { 3 };
        let x0 = cx - wdt / 2;
        for dx in 0..wdt {
            let base = if dx == 0 || dx == wdt - 1 { 88 } else { 138 };
            put_shade_if_empty(pixels, x0 + dx, s - dy, wood, base + rng.range_i32(-10, 10));
        }
    }
    // 根部外扩
    for dx in [-2i32, -1, 1, 2] {
        put_shade_if_empty(pixels, x + dx, s - 1, wood, 96 + rng.range_i32(-8, 8));
    }
    // 侧枝（左右交替，向上伸展），记录枝端供叶团附着
    let mut tips: Vec<(i32, i32)> = Vec::new();
    let n_br = 2 + rng.range_i32(0, 2);
    for i in 0..n_br {
        let bh = (hgt * (50 + 20 * i) / 100).max(10);
        let dir = if i % 2 == 0 { -1 } else { 1 };
        let mut bx = x + dir * 2;
        let mut by = s - bh;
        let len = rng.range_i32(5, 11);
        for _ in 0..len {
            bx += dir;
            if rng.chance(0.65) {
                by -= 1;
            }
            put_shade_if_empty(pixels, bx, by, wood, 100 + rng.range_i32(-8, 8));
            if rng.chance(0.4) {
                put_shade_if_empty(pixels, bx, by - 1, wood, 112 + rng.range_i32(-8, 8));
            }
        }
        tips.push((bx, by));
    }
    // 主树冠 + 枝端叶团
    let top_y = s - hgt;
    let (rx, ry) = (rng.range_i32(9, 13), rng.range_i32(6, 9));
    canopy(pixels, rng, x + lean, top_y - 3, rx, ry, leaf);
    for (tx, ty) in tips {
        let (rx, ry) = (rng.range_i32(3, 5), rng.range_i32(3, 4));
        canopy(pixels, rng, tx, ty - 2, rx, ry, leaf);
    }
}

/// 松树（雪原）：细直树干 + 层叠三角冠
fn gen_pine(pixels: &mut PixelWorld, rng: &mut Rng, x: i32, s: i32, wood: u8, leaf: u8) {
    let hgt = rng.range_i32(58, 88);
    for dy in 1..=hgt {
        for (dx, base) in [(0i32, 92i32), (1, 126)] {
            put_shade_if_empty(pixels, x + dx, s - dy, wood, base + rng.range_i32(-8, 8));
        }
    }
    let mut ly = s - hgt;
    let mut lw = 2.4f32 + rng.f32() * 1.5; // 顶层半宽（逐层加宽）
    let layers = 4 + rng.range_i32(0, 2);
    for _ in 0..layers {
        let lh = rng.range_i32(7, 12);
        for dy in 0..lh {
            let hw = (lw * dy as f32 / lh as f32) as i32;
            for dx in -hw..=hw {
                let shade = 112 - dy * 5 + rng.range_i32(-12, 12);
                put_shade_if_empty(pixels, x + dx, ly + dy, leaf, shade);
            }
        }
        ly += lh - 3;
        lw += 2.0 + rng.f32() * 1.6;
    }
}

pub fn generate(
    seed: u64,
    pixels: &mut PixelWorld,
    mats: &Materials,
    walls: &mut [u8],
    wall_shade: &mut [u8],
) -> GenResult {
    let (w, h) = (pixels.w, pixels.h);
    let id = |name: &str| mats.id(name).unwrap_or(0);
    let (dirt, grass, stone, wood, leaf) =
        (id("dirt"), id("grass"), id("stone"), id("wood"), id("leaf"));
    let (sandstone, snowpack, bedrock) = (id("sandstone"), id("snowpack"), id("bedrock"));
    let (coal, iron, gold) = (id("coal_ore"), id("iron_ore"), id("gold_ore"));
    let (sand_px, snow_px, water, lava, rope) =
        (id("sand"), id("snow"), id("water"), id("lava"), id("rope"));

    let mut rng = pixels.rng().fork();
    let fbm_terr = Fbm::<Perlin>::new(seed as u32);
    let fbm_terr2 = Fbm::<Perlin>::new((seed ^ 0xBEEF) as u32);
    let fbm_cave = Fbm::<Perlin>::new((seed ^ 0xCAFE) as u32);
    let fbm_cave2 = Fbm::<Perlin>::new((seed ^ 0xF00D) as u32);

    // ---- 地表高度（像素）----
    let mut surf = vec![0i32; w as usize];
    for x in 0..w {
        let n1 = fbm_terr.get([x as f64 * 0.006, 0.0]);
        let n2 = fbm_terr2.get([x as f64 * 0.028, 50.0]);
        surf[x as usize] = (h as f64 * 0.34 + n1 * 44.0 + n2 * 10.0).round() as i32;
    }
    let biome = |x: i32| -> u8 {
        if (x as f32) < w as f32 * 0.22 {
            0 // 雪原
        } else if (x as f32) > w as f32 * 0.78 {
            2 // 沙漠
        } else {
            1 // 森林
        }
    };

    // ---- 地层填充 ----
    for x in 0..w {
        let s = surf[x as usize];
        let dirt_depth = 48 + (fbm_terr2.get([x as f64 * 0.05, 999.0]) * 28.0) as i32;
        for y in s..h {
            let t = if y < s + 3 {
                match biome(x) {
                    0 => snowpack,
                    2 => sandstone,
                    _ => grass,
                }
            } else if y < s + dirt_depth {
                match biome(x) {
                    0 => snowpack,
                    2 => sandstone,
                    _ => dirt,
                }
            } else {
                stone
            };
            put(pixels, mats, &mut rng, x, y, t);
        }
    }

    // ---- 边界基岩 ----
    for y in 0..h {
        for x in 0..8 {
            put(pixels, mats, &mut rng, x, y, bedrock);
            put(pixels, mats, &mut rng, w - 1 - x, y, bedrock);
        }
    }
    for x in 0..w {
        for y in (h - 4)..h {
            put(pixels, mats, &mut rng, x, y, bedrock);
        }
    }

    // ---- 洞穴（4px 网格采样 → 成块雕刻）----
    for cy in 0..(h / 4) {
        let y = cy * 4;
        for cx in 0..(w / 4) {
            let x = cx * 4;
            if x < 10 || x >= w - 10 || y < 24 || y >= h - 24 {
                continue;
            }
            let s = surf[((x).min(w - 1)) as usize];
            if y < s + 24 {
                continue;
            }
            let n1 = fbm_cave.get([x as f64 * 0.02, y as f64 * 0.03]);
            let n2 = fbm_cave2.get([x as f64 * 0.011 + 40.0, y as f64 * 0.015 + 40.0]);
            if n1 > 0.32 || n2.abs() < 0.042 {
                for dy in 0..4 {
                    for dx in 0..4 {
                        clear(pixels, x + dx, y + dy);
                    }
                }
            }
        }
    }

    // ---- 矿脉（像素随机游走，替换石头）----
    let vein_count = (w * h) / 128000;
    for _ in 0..vein_count {
        let x0 = rng.range_i32(16, w - 17);
        let y0 = rng.range_i32((h as f32 * 0.40) as i32, h - 32);
        let ore = if y0 > (h as f32 * 0.72) as i32 {
            if rng.chance(0.35) { gold } else if rng.chance(0.6) { iron } else { coal }
        } else if y0 > (h as f32 * 0.45) as i32 {
            if rng.chance(0.55) { iron } else { coal }
        } else {
            coal
        };
        let mut cx = x0;
        let mut cy = y0;
        for _ in 0..rng.range_i32(16, 44) {
            if pixels.get(cx, cy).mat == stone {
                put(pixels, mats, &mut rng, cx, cy, ore);
            }
            cx = (cx + rng.range_i32(-3, 3)).clamp(10, w - 11);
            cy = (cy + rng.range_i32(-3, 3)).clamp((h as f32 * 0.35) as i32, h - 28);
        }
    }

    // ---- 树（背景装饰：树干不碰撞、不遮挡角色，角色可从树前走过）----
    let mut last_tree = -999i32;
    for x in 16..w - 16 {
        let b = biome(x);
        let (density, ground) = match b {
            1 => (0.012, grass),
            0 => (0.006, snowpack),
            _ => (0.0, grass),
        };
        if density == 0.0 || !rng.chance(density) || x - last_tree < 26 {
            continue;
        }
        let s = surf[x as usize];
        if pixels.get(x, s).mat != ground || pixels.get(x, s - 1).mat != 0 {
            continue;
        }
        last_tree = x;
        if b == 0 {
            gen_pine(pixels, &mut rng, x, s, wood, leaf);
        } else {
            gen_oak(pixels, &mut rng, x, s, wood, leaf);
        }
    }

    // ---- 背景墙（地下洞穴背景，泰拉瑞亚式：4px/格，洞穴中保留形成封闭背景）----
    let (wall_dirt, wall_stone) = (id("wall_dirt"), id("wall_stone"));
    let (wall_snow, wall_sand) = (id("wall_snow"), id("wall_sand"));
    if wall_dirt > 0 {
        let gw = (w / 4) as usize;
        for cy in 0..(h / 4) {
            for cx in 0..(w / 4) {
                let px = cx * 4 + 2;
                let py = cy * 4 + 2;
                let s = surf[((px).min(w - 1)) as usize];
                if py <= s + 6 {
                    continue; // 地表浅层无墙（露出天空）
                }
                let depth = py - s;
                let dirt_depth = 48 + (fbm_terr2.get([px as f64 * 0.05, 999.0]) * 28.0) as i32;
                let m = if depth > dirt_depth + 6 {
                    wall_stone
                } else {
                    match biome(px) {
                        0 => wall_snow,
                        2 => wall_sand,
                        _ => wall_dirt,
                    }
                };
                let i = cy as usize * gw + cx as usize;
                walls[i] = m;
                wall_shade[i] = (122 + rng.range_i32(-8, 8)) as u8;
            }
        }
    }

    // ---- 地表水塘（平坦处挖坑注水）----
    let mut ponds = 0;
    let mut bx = w / 6;
    while bx < w - 48 && ponds < 4 {
        let s0 = surf[bx as usize];
        let flat = (bx..bx + 30).all(|i| (surf[i as usize] - s0).abs() <= 3);
        if flat && biome(bx) == 1 && rng.chance(0.5) {
            for dx in 4..26 {
                for dy in 4..14 {
                    clear(pixels, bx + dx, s0 + dy);
                }
            }
            for dx in 4..26 {
                for dy in 4..14 {
                    put_if_empty(pixels, mats, &mut rng, bx + dx, s0 + dy, water);
                }
            }
            ponds += 1;
            bx += 140;
        } else {
            bx += 24;
        }
    }

    // ---- 深层岩浆 ----
    if lava > 0 {
        for _ in 0..80 {
            let x = rng.range_i32(20, w - 21);
            let y = rng.range_i32((h as f32 * 0.76) as i32, h - 24);
            if pixels.get(x, y).mat == 0 && pixels.get(x, y + 2).mat != 0 {
                for dy in -3..=1 {
                    for dx in -6..=6 {
                        put_if_empty(pixels, mats, &mut rng, x + dx, y + dy, lava);
                    }
                }
            }
        }
    }

    // ---- 洞穴绳索 ----
    for _ in 0..160 {
        let x = rng.range_i32(16, w - 17);
        let y = rng.range_i32(100, h - 60);
        if pixels.get(x, y).mat == 0 && pixels.get(x, y - 1).mat != 0 {
            let len = rng.range_i32(16, 40);
            for dy in 0..len {
                if pixels.get(x, y + dy).mat == 0 {
                    put(pixels, mats, &mut rng, x, y + dy, rope);
                } else {
                    break;
                }
            }
        }
    }

    // ---- 雪原/沙漠表层松散像素 ----
    for x in 8..w - 8 {
        let s = surf[x as usize];
        match biome(x) {
            0 => {
                for py in (s - 14).max(0)..s {
                    put_if_empty(pixels, mats, &mut rng, x, py, snow_px);
                }
            }
            2 => {
                for py in (s - 10).max(0)..s {
                    put_if_empty(pixels, mats, &mut rng, x, py, sand_px);
                }
            }
            _ => {}
        }
    }

    let cx = w / 2;
    GenResult { spawn_x: cx, spawn_y: surf[cx as usize] - 24 }
}
