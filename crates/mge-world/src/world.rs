//! World：像素世界 + 光照 + 昼夜（地形即像素，Noita 式）
use crate::gen;
use crate::light::{LightMap, LIGHT_CELL};
use mge_core::events::Events;
use mge_core::rng::Rng;
use mge_sim::materials::Kind;
use mge_sim::{Materials, Pixel, PixelWorld, SimHooks};

#[derive(Debug, Clone, Copy)]
pub enum WorldEvent {
    Explosion { x: f32, y: f32 },
}

pub struct World {
    pub pixels: PixelWorld,
    pub mats: Materials,
    pub light: LightMap,
    /// 一天的时间 0..1（0 = 黎明）
    pub time: f32,
    /// 一天的现实秒数
    pub day_len: f32,
    pub spawn_x: i32,
    pub spawn_y: i32,
    /// 火把位置（像素坐标）
    pub torches: Vec<(i32, i32)>,
    torch_mat: u8,
    terrain_dirty: bool,
    light_dirty: bool,
    light_frame: u32,
    rng: Rng,
    pub events: Events<WorldEvent>,
}

/// 模拟钩子：仅聚合发光贡献（供方块光照）
struct WorldHooks<'a> {
    light: &'a mut LightMap,
}

impl SimHooks for WorldHooks<'_> {
    fn add_emissive(&mut self, x: i32, y: i32, amount: u8) {
        *self
            .light
            .emissive
            .entry((x / LIGHT_CELL, y / LIGHT_CELL))
            .or_insert(0) += amount as u32;
    }
}

impl World {
    pub fn new(seed: u64, w_px: i32, h_px: i32) -> Self {
        let mats = Materials::embedded();
        let mut pixels = PixelWorld::new(w_px, h_px, &mats);
        let gr = gen::generate(seed, &mut pixels, &mats);
        let light = LightMap::new(
            (w_px + LIGHT_CELL - 1) / LIGHT_CELL,
            (h_px + LIGHT_CELL - 1) / LIGHT_CELL,
        );
        let torch_mat = mats.id("torch").unwrap_or(0);
        Self {
            pixels,
            mats,
            light,
            time: 0.20,
            day_len: 480.0,
            spawn_x: gr.spawn_x,
            spawn_y: gr.spawn_y,
            torches: Vec::new(),
            torch_mat,
            terrain_dirty: true,
            light_dirty: false,
            light_frame: 0,
            rng: Rng::new(seed ^ 0x5EED_5EED),
            events: Events::new(),
        }
    }

    #[inline]
    pub fn solid_px(&self, x: i32, y: i32) -> bool {
        self.mats.def(self.pixels.get(x, y).mat).solid
    }

    #[inline]
    pub fn platform_px(&self, x: i32, y: i32) -> bool {
        self.mats.def(self.pixels.get(x, y).mat).platform
    }

    #[inline]
    pub fn climbable_px(&self, x: i32, y: i32) -> bool {
        self.mats.def(self.pixels.get(x, y).mat).climbable
    }

    /// 地形变化标记（触发光照重算）
    pub fn mark_terrain_dirty(&mut self) {
        self.terrain_dirty = true;
    }

    /// 是否需要重新上传光照纹理（并清除标记）
    pub fn take_light_dirty(&mut self) -> bool {
        std::mem::take(&mut self.light_dirty)
    }

    /// 推进一逻辑帧（1/60s）
    pub fn update(&mut self) {
        self.time = (self.time + 1.0 / 60.0 / self.day_len) % 1.0;

        // 像素模拟（字段拆分借用）
        let World { pixels, mats, light, .. } = self;
        pixels.step(mats, &mut WorldHooks { light });

        self.light.decay_emissive();
        self.light_frame += 1;
        if self.terrain_dirty || self.light_frame % 10 == 0 {
            self.light.relight(&self.pixels, &self.mats, &self.torches);
            self.terrain_dirty = false;
            self.light_dirty = true;
        }
    }

    /// 挖掘一个像素（累积伤害），破坏时掉落碎屑
    pub fn mine_px(&mut self, x: i32, y: i32, power: u16) -> bool {
        let Some(broken) = self.pixels.mine_px(x, y, power, &self.mats) else {
            return false;
        };
        let drop = self.mats.def(broken).drop.clone();
        if let Some(drop) = drop {
            if let Some(dm) = self.mats.id(&drop) {
                if self.rng.chance(0.2) {
                    self.pixels.spawn(x, y, dm, &self.mats);
                }
            }
        }
        if broken == self.torch_mat {
            self.torches.retain(|(tx, ty)| *tx != x || *ty != y);
        }
        self.terrain_dirty = true;
        true
    }

    /// 放置火把（1 像素 + 光源）
    /// 放置火把：目标格为空，且必须依附表面（四邻有静态实心/平台像素），不可悬空
    pub fn place_torch(&mut self, x: i32, y: i32) -> bool {
        if self.pixels.get(x, y).mat != 0 || self.torch_mat == 0 {
            return false;
        }
        let supported = [
            (x, y + 1), // 地面
            (x, y - 1), // 悬挂
            (x - 1, y), // 左墙
            (x + 1, y), // 右墙
        ]
        .iter()
        .any(|&(nx, ny)| {
            let d = self.mats.def(self.pixels.get(nx, ny).mat);
            (d.solid || d.platform) && d.kind == Kind::Static
        });
        if !supported {
            return false;
        }
        self.pixels.spawn(x, y, self.torch_mat, &self.mats);
        self.torches.push((x, y));
        self.terrain_dirty = true;
        true
    }

    /// 爆炸：清除半径内地形像素、喷碎屑、点火、烟雾
    pub fn explode(&mut self, cx: i32, cy: i32, r: i32) {
        let fire = self.pixels.ids.fire;
        let smoke = self.pixels.ids.smoke;
        for ty in (cy - r)..=(cy + r) {
            for tx in (cx - r)..=(cx + r) {
                let dx = tx - cx;
                let dy = ty - cy;
                let dist2 = (dx * dx + dy * dy) as f32;
                let rr = r as f32 * (0.75 + self.rng.f32() * 0.4);
                if dist2 <= rr * rr {
                    let p = self.pixels.get(tx, ty);
                    let def = self.mats.def(p.mat);
                    if def.kind == Kind::Static && def.hp > 0 {
                        self.pixels.set(tx, ty, Pixel::default());
                        if let Some(drop) = &def.drop {
                            if let Some(dm) = self.mats.id(drop) {
                                if self.rng.chance(0.12) {
                                    self.pixels.spawn(tx, ty, dm, &self.mats);
                                }
                            }
                        }
                    }
                }
            }
        }
        // 火环与烟
        for _ in 0..(r * 2) {
            let ang = self.rng.range_f32(0.0, std::f32::consts::TAU);
            let rr = r as f32 * self.rng.range_f32(0.8, 1.1);
            let px = cx + (ang.cos() * rr) as i32;
            let py = cy + (ang.sin() * rr) as i32;
            if !self.solid_px(px, py) {
                self.pixels
                    .spawn(px, py, if self.rng.chance(0.6) { fire } else { smoke }, &self.mats);
            }
        }
        self.terrain_dirty = true;
        self.events.send(WorldEvent::Explosion { x: cx as f32, y: cy as f32 });
    }

    /// 昼夜环境光系数（0.16 夜 — 1.0 昼）
    pub fn ambient(&self) -> f32 {
        let t = self.time;
        let day = smooth(0.02, 0.10, t) * (1.0 - smooth(0.48, 0.58, t));
        0.16 + 0.84 * day
    }

    /// 天空颜色（随时间渐变）
    pub fn sky_color(&self) -> [f32; 3] {
        const STOPS: [(f32, [f32; 3]); 8] = [
            (0.00, [0.98, 0.66, 0.45]), // 黎明
            (0.06, [0.55, 0.72, 0.92]),
            (0.25, [0.47, 0.68, 0.93]), // 正午
            (0.44, [0.60, 0.70, 0.90]),
            (0.52, [0.95, 0.52, 0.32]), // 黄昏
            (0.60, [0.16, 0.13, 0.26]),
            (0.75, [0.04, 0.05, 0.11]), // 深夜
            (0.95, [0.60, 0.38, 0.35]), // 破晓前
        ];
        let t = self.time;
        for i in 0..STOPS.len() {
            let (t0, c0) = STOPS[i];
            let (mut t1, c1) = STOPS[(i + 1) % STOPS.len()];
            if i + 1 == STOPS.len() {
                t1 += 1.0;
            }
            if t >= t0 && t <= t1 {
                let k = ((t - t0) / (t1 - t0).max(1e-5)).clamp(0.0, 1.0);
                return [
                    c0[0] + (c1[0] - c0[0]) * k,
                    c0[1] + (c1[1] - c0[1]) * k,
                    c0[2] + (c1[2] - c0[2]) * k,
                ];
            }
        }
        [0.4, 0.6, 0.9]
    }
}

fn smooth(a: f32, b: f32, t: f32) -> f32 {
    let k = ((t - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}
