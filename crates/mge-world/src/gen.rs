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

pub fn generate(seed: u64, pixels: &mut PixelWorld, mats: &Materials) -> GenResult {
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

    // ---- 树 ----
    for x in 16..w - 16 {
        let b = biome(x);
        let (density, ground) = match b {
            1 => (0.006, grass),
            0 => (0.003, snowpack),
            _ => (0.0, grass),
        };
        if !rng.chance(density) {
            continue;
        }
        let s = surf[x as usize];
        if pixels.get(x, s).mat != ground || pixels.get(x, s - 1).mat != 0 {
            continue;
        }
        let hh = rng.range_i32(40, 72);
        for dy in 1..=hh {
            put(pixels, mats, &mut rng, x, s - dy, wood);
            put(pixels, mats, &mut rng, x + 1, s - dy, wood);
        }
        let top = s - hh;
        let r = rng.range_i32(8, 14);
        for dy in -(r + 2)..=(r / 2) {
            for dx in -(r + 2)..=(r + 2) {
                if dx * dx + dy * dy * 2 <= r * r {
                    let (lx, ly) = (x + dx, top + dy);
                    if pixels.get(lx, ly).mat == 0 {
                        put(pixels, mats, &mut rng, lx, ly, leaf);
                    }
                }
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
