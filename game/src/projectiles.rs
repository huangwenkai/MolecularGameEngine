//! 投射物：箭矢（重力/入水熄灭/钉入地形）与火球（尾焰/命中爆炸）
use crate::vfx::Vfx;
use glam::Vec2;
use mge_core::rng::Rng;
use mge_render::{Region, SpriteBatch};
use mge_world::World;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProjKind {
    Arrow,
    Fireball,
}

#[derive(Debug, Clone, Copy)]
pub struct Projectile {
    pub kind: ProjKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub stuck: bool,
    pub age: f32,
}

pub struct Projectiles {
    pub list: Vec<Projectile>,
    /// 特效蓝图名（数据驱动，编辑器可改）
    pub fx_fizz: String,
    pub fx_explosion: String,
    pub fx_arrow_hit: String,
    pub fx_hit_spark: String,
}

impl Default for Projectiles {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            fx_fizz: "fire_fizz".into(),
            fx_explosion: "explosion".into(),
            fx_arrow_hit: "arrow_hit".into(),
            fx_hit_spark: "hit_spark".into(),
        }
    }
}

pub const ARROW_DMG: f32 = 14.0;
pub const FIREBALL_DMG: f32 = 26.0;

impl Projectiles {
    pub fn spawn(&mut self, kind: ProjKind, pos: Vec2, vel: Vec2) {
        self.list.push(Projectile {
            kind,
            pos,
            vel,
            life: match kind {
                ProjKind::Arrow => 6.0,
                ProjKind::Fireball => 4.0,
            },
            stuck: false,
            age: 0.0,
        });
    }

    /// 更新（地形/水面交互），返回顿帧；爆炸特效与毁伤在内部完成
    pub fn update(&mut self, world: &mut World, vfx: &mut Vfx, rng: &mut Rng, dt: f32) -> u8 {
        let water = world.pixels.ids.water;
        let mut hitstop = 0u8;
        self.list.retain_mut(|p| {
            p.life -= dt;
            p.age += dt;
            if p.life <= 0.0 {
                return false;
            }
            if p.stuck {
                return true; // 钉在地形里，倒计时后消失
            }
            let steps = ((p.vel * dt).length() / 3.0).ceil().max(1.0) as i32;
            let sdt = dt / steps as f32;
            for _ in 0..steps {
                if p.kind == ProjKind::Arrow {
                    p.vel.y += 620.0 * sdt;
                }
                p.pos += p.vel * sdt;
                let (px, py) = (p.pos.x as i32, p.pos.y as i32);
                let mat = world.pixels.get(px, py).mat;
                // 入水：箭矢减速 / 火球化汽
                if mat == water {
                    if p.kind == ProjKind::Fireball {
                        vfx.spawn(&self.fx_fizz, p.pos, 1.0, rng);
                        return false;
                    }
                    p.vel *= 0.90;
                }
                // 命中地形
                if world.solid_px(px, py) {
                    if p.kind == ProjKind::Fireball {
                        world.explode(px, py, 7);
                        vfx.spawn(&self.fx_explosion, p.pos, 1.0, rng);
                        hitstop = hitstop.max(6);
                        return false;
                    }
                    p.stuck = true;
                    p.life = p.life.min(3.0);
                    vfx.spawn(&self.fx_arrow_hit, p.pos, 1.0, rng);
                    break;
                }
            }
            // 尾焰/尾迹
            if p.kind == ProjKind::Fireball && !p.stuck {
                vfx.dot(
                    p.pos + Vec2::new(rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0)),
                    -p.vel * 0.08,
                    rng.range_f32(0.15, 0.35),
                    rng.range_f32(1.5, 2.8),
                    [1.0, 0.55, 0.15],
                    -40.0,
                    true,
                );
            }
            true
        });
        hitstop
    }

    /// 假人命中检测：调用方提供目标集合，返回命中 (entity, 伤害, 击退, 暴击, 类型, 命中点)
    /// 火球命中后由调用方在世界里引爆（避免借用冲突）；st 为玩家聚合属性
    pub fn check_dummies(
        &mut self,
        targets: &[(hecs::Entity, Vec2, Vec2)], // (entity, pos, half)
        vfx: &mut Vfx,
        rng: &mut Rng,
        st: &crate::items::Stats,
    ) -> Vec<(hecs::Entity, f32, Vec2, bool, ProjKind, Vec2)> {
        let mut out = Vec::new();
        self.list.retain_mut(|p| {
            if p.stuck {
                return true;
            }
            for (e, tpos, thalf) in targets {
                let da = mge_core::math::Aabb::new(*tpos - Vec2::new(0.0, thalf.y), *thalf);
                if da.min.x <= p.pos.x
                    && p.pos.x <= da.max.x
                    && da.min.y <= p.pos.y
                    && p.pos.y <= da.max.y
                {
                    let crit = rng.chance(st.crit / 100.0);
                    let crit_mult = 1.8 + st.crit_dmg / 100.0;
                    let base = match p.kind {
                        ProjKind::Arrow => ARROW_DMG,
                        ProjKind::Fireball => FIREBALL_DMG,
                    };
                    let dmg = st.damage(base) * if crit { crit_mult } else { 1.0 };
                    let knock = p.vel.normalize_or_zero() * 90.0;
                    let name = match p.kind {
                        ProjKind::Arrow => self.fx_arrow_hit.as_str(),
                        ProjKind::Fireball => self.fx_hit_spark.as_str(),
                    };
                    vfx.spawn(name, p.pos, 1.0, rng);
                    vfx.text(p.pos + Vec2::new(0.0, -6.0), dmg as u32, crit);
                    out.push((*e, dmg, knock, crit, p.kind, p.pos));
                    return false;
                }
            }
            true
        });
        out
    }
}

pub fn render(list: &[Projectile], batch: &mut SpriteBatch, white: &Region, arrow: &Region) {
    for p in list {
        match p.kind {
            ProjKind::Arrow => {
                let ang = if p.stuck {
                    p.vel.y.atan2(0.0)
                } else {
                    p.vel.y.atan2(p.vel.x)
                };
                batch.push(p.pos, Vec2::new(9.0, 3.0), ang, arrow, [1.0; 4]);
            }
            ProjKind::Fireball => {
                let k = (p.age * 18.0).sin().abs();
                batch.push_at(p.pos, Vec2::new(3.0, 3.0), white, [
                    1.0,
                    0.6 + k * 0.25,
                    0.2,
                    1.0,
                ]);
                batch.push_at(p.pos, Vec2::new(1.6, 1.6), white, [1.0, 1.0, 0.85, 1.0]);
            }
        }
    }
}
