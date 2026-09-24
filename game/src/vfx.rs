//! VFX：CPU 粒子池 + 伤害飘字（3x5 位图数字）+ 数据驱动特效蓝图（RON）
//! 蓝图运行时可替换（M12 接编辑器热重载）
use glam::Vec2;
use mge_core::rng::Rng;
use mge_render::{Region, SpriteBatch};
use std::collections::HashMap;

pub const MAX_PARTICLES: usize = 4096;

/// 粒子
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub color: [f32; 3],
    pub gravity: f32,
    pub drag: f32, // 每秒速度保留率（0.9 = 强阻力）
    pub glow: bool,
}

/// 伤害飘字
#[derive(Debug, Clone, Copy)]
pub struct FloatText {
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub value: u32,
    pub crit: bool,
}

// ---- 3x5 位图数字（每行 3 位）----
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

// ---- 特效蓝图（RON 数据驱动）----

/// 发射形状（kind: "point" | "arc"；arc 用于挥砍弧线）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShapeDef {
    pub kind: String,
    #[serde(default)]
    pub radius: f32,
    #[serde(default)]
    pub a0: f32,
    #[serde(default)]
    pub a1: f32,
}

impl Default for ShapeDef {
    fn default() -> Self {
        Self { kind: "point".into(), radius: 12.0, a0: -1.0, a1: 1.0 }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Emitter {
    #[serde(default)]
    pub offset: [f32; 2],
    pub shape: ShapeDef,
    pub count: u32,
    /// 基础方向（弧度，x 正方向为 0），实际 = dir * facing + 随机 spread
    #[serde(default)]
    pub dir: f32,
    #[serde(default)]
    pub spread: f32,
    /// 速度范围 [min, max]
    pub speed: [f32; 2],
    #[serde(default = "def_gravity")]
    pub gravity: f32,
    #[serde(default = "def_drag")]
    pub drag: f32,
    pub life: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 3],
    #[serde(default)]
    pub color2: [f32; 3],
    #[serde(default)]
    pub glow: bool,
}

fn def_gravity() -> f32 {
    260.0
}
fn def_drag() -> f32 {
    1.0
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Blueprint {
    #[serde(default)]
    pub shake: f32,
    #[serde(default)]
    pub hitstop: u8,
    pub emitters: Vec<Emitter>,
}

pub struct Vfx {
    pub particles: Vec<Particle>,
    pub texts: Vec<FloatText>,
    pub bps: HashMap<String, Blueprint>,
}

impl Vfx {
    /// 内置蓝图（RON，后续文件化 + 编辑器热重载）
    pub fn embedded() -> Self {
        let bps = ron::from_str::<HashMap<String, Blueprint>>(include_str!(
            "../assets/data/vfx.ron"
        ))
        .expect("vfx.ron 解析失败");
        Self { particles: Vec::new(), texts: Vec::new(), bps }
    }

    /// 触发命名特效，返回 (震屏, 顿帧)
    pub fn spawn(&mut self, name: &str, pos: Vec2, facing: f32, rng: &mut Rng) -> (f32, u8) {
        let Some(bp) = self.bps.get(name).cloned() else {
            tracing::warn!("未知特效蓝图 {name}");
            return (0.0, 0);
        };
        self.spawn_bp(&bp, pos, facing, rng)
    }

    /// 按蓝图对象发射（编辑器预览用）
    pub fn spawn_bp(&mut self, bp: &Blueprint, pos: Vec2, facing: f32, rng: &mut Rng) -> (f32, u8) {
        for em in &bp.emitters {
            self.emit(em, pos, facing, rng);
        }
        (bp.shake, bp.hitstop)
    }

    /// 直接发射粒子（代码用的低层接口）
    pub fn emit(&mut self, em: &Emitter, pos: Vec2, facing: f32, rng: &mut Rng) {
        for _ in 0..em.count {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let [ox, oy] = em.offset;
            let base = pos + Vec2::new(ox * facing, oy);
            let arc = em.shape.kind == "arc";
            let (ppos, ang) = if arc {
                let a = em.shape.a1 * facing + rng.range_f32(em.shape.a0, em.shape.a1);
                (
                    base + Vec2::new(a.cos(), a.sin()) * em.shape.radius * facing.signum(),
                    a,
                )
            } else {
                (base, em.dir * facing + rng.range_f32(-em.spread, em.spread))
            };
            let spd = rng.range_f32(em.speed[0], em.speed[1]);
            let vel = if arc {
                // 弧线发射：沿切向扫过 + 少许外扩，形成挥砍弧面
                Vec2::new(-ang.sin() * facing, ang.cos()) * spd
                    + Vec2::new(ang.cos(), ang.sin()) * spd * 0.3
            } else {
                // 点发射：沿方向锥喷射
                Vec2::new(ang.cos(), ang.sin()) * spd
            };
            let color = if em.color2 != [0.0, 0.0, 0.0] && rng.chance(0.5) {
                em.color2
            } else {
                em.color
            };
            self.particles.push(Particle {
                pos: ppos,
                vel,
                life: rng.range_f32(em.life[0], em.life[1]),
                max_life: em.life[1],
                size: rng.range_f32(em.size[0], em.size[1]),
                color,
                gravity: em.gravity,
                drag: em.drag,
                glow: em.glow,
            });
        }
    }

    /// 快捷：圆点粒子
    #[allow(clippy::too_many_arguments)]
    pub fn dot(
        &mut self,
        pos: Vec2,
        vel: Vec2,
        life: f32,
        size: f32,
        color: [f32; 3],
        gravity: f32,
        glow: bool,
    ) {
        if self.particles.len() >= MAX_PARTICLES {
            return;
        }
        self.particles.push(Particle {
            pos,
            vel,
            life,
            max_life: life,
            size,
            color,
            gravity,
            drag: 1.0,
            glow,
        });
    }

    pub fn text(&mut self, pos: Vec2, value: u32, crit: bool) {
        self.texts.push(FloatText {
            pos,
            vel: Vec2::new(0.0, -26.0),
            life: 0.9,
            value,
            crit,
        });
    }

    pub fn update(&mut self, dt: f32) {
        self.particles.retain_mut(|p| {
            p.life -= dt;
            if p.life <= 0.0 {
                return false;
            }
            p.vel.y += p.gravity * dt;
            p.vel *= p.drag.powf(dt);
            p.pos += p.vel * dt;
            true
        });
        self.texts.retain_mut(|t| {
            t.life -= dt;
            if t.life <= 0.0 {
                return false;
            }
            t.vel.y += 40.0 * dt; // 缓缓减速上飘
            t.vel *= 0.94_f32.powf(dt * 60.0);
            t.pos += t.vel * dt;
            true
        });
    }

    pub fn render(&self, batch: &mut SpriteBatch, white: &Region, cam_tl: Vec2, cam_br: Vec2) {
        for p in &self.particles {
            if p.pos.x < cam_tl.x - 4.0 || p.pos.x > cam_br.x + 4.0 {
                continue;
            }
            if p.pos.y < cam_tl.y - 4.0 || p.pos.y > cam_br.y + 4.0 {
                continue;
            }
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            let mut c = p.color;
            if p.glow {
                // 高亮粒子：亮度随生命衰减保持在 1 以上
                c = [c[0] + k * 1.2, c[1] + k * 0.9, c[2] + k * 0.6];
            } else {
                c = [c[0] * k, c[1] * k, c[2] * k];
            }
            let s = if p.glow { p.size * (0.5 + k * 0.5) } else { p.size };
            batch.push_at(p.pos, Vec2::new(s, s), white, [c[0], c[1], c[2], 1.0]);
        }
        // 伤害飘字
        for t in &self.texts {
            let k = (t.life / 0.9).clamp(0.0, 1.0);
            let color: [f32; 3] = if t.crit { [1.0, 0.62, 0.10] } else { [0.95, 0.95, 0.95] };
            let s = format!("{}", t.value);
            let w = (s.len() * 4 - 1) as f32;
            let mut ox = -w * 0.5;
            for ch in s.chars() {
                let d = DIGITS[ch as usize - '0' as usize];
                for (row, bits) in d.iter().enumerate() {
                    for col in 0..3 {
                        if bits & (1 << (2 - col)) != 0 {
                            let p = t.pos
                                + Vec2::new(ox + col as f32, row as f32 - 5.0 * (1.0 - k));
                            batch.push_at(p, Vec2::new(1.0, 1.0), white, [
                                color[0] * (0.4 + k * 0.6),
                                color[1] * (0.4 + k * 0.6),
                                color[2] * (0.4 + k * 0.6),
                                1.0,
                            ]);
                        }
                    }
                }
                ox += 4.0;
            }
        }
    }
}
