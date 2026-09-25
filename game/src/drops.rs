//! 掉落物：受像素重力、稀有度光柱、磁吸拾取
use crate::inventory::Inventory;
use crate::items::{Item, ItemDb};
use glam::Vec2;
use mge_core::rng::Rng;
use mge_render::{Region, SpriteBatch};
use mge_world::World;

pub struct Drop {
    pub item: Item,
    pub pos: Vec2,
    pub vel: Vec2,
    pub age: f32,
    /// 出手保护（刚掉落不被立即吸走）
    pub delay: f32,
}

#[derive(Default)]
pub struct Drops {
    pub list: Vec<Drop>,
}

impl Drops {
    /// 掉落一束物品（随机散开）
    pub fn spawn_loot(&mut self, items: Vec<Item>, pos: Vec2, rng: &mut Rng) {
        for it in items {
            let ang = rng.range_f32(-2.6, -0.5);
            let spd = rng.range_f32(50.0, 130.0);
            self.list.push(Drop {
                item: it,
                pos,
                vel: Vec2::new(ang.cos() * spd, ang.sin() * spd),
                age: 0.0,
                delay: 0.35,
            });
        }
        if self.list.len() > 200 {
            self.list.drain(0..self.list.len() - 200);
        }
    }

    /// 更新：像素重力 / 水面浮力 / 磁吸 / 拾取。
    /// 返回 (拾取物品名列表, 是否发生背包满)（UI 提示用）
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        player_pos: Vec2,
        inv: &mut Inventory,
        db: &ItemDb,
    ) -> (Vec<String>, bool) {
        let water = world.pixels.ids.water;
        let mut picked = Vec::new();
        let mut bag_full = false;
        self.list.retain_mut(|d| {
            d.age += 1.0 / 60.0;
            if d.delay > 0.0 {
                d.delay -= 1.0 / 60.0;
            }
            let (px, py) = (d.pos.x as i32, d.pos.y as i32);
            let in_water = world.pixels.get(px, py).mat == water;

            // ---- 磁吸 ----
            if d.delay <= 0.0 {
                let to_p = player_pos + Vec2::new(0.0, -8.0) - d.pos;
                let dist = to_p.length();
                if dist < 56.0 {
                    let pull = to_p.normalize_or_zero() * 980.0 * (1.0 - dist / 56.0);
                    d.vel += pull / 60.0;
                    if dist < 8.0 {
                        if inv.add(d.item.clone(), db) {
                            picked.push(db.def(&d.item.def).name.clone());
                            return false;
                        }
                        // 背包满：不再吸（并提示玩家）
                        d.vel = -d.vel * 0.5;
                        bag_full = true;
                    }
                }
            }

            // ---- 重力 / 浮力 ----
            if in_water {
                d.vel.y -= 60.0 / 60.0; // 缓浮
                d.vel *= 0.92;
            } else {
                d.vel.y += 700.0 / 60.0;
            }
            if d.vel.y > 400.0 {
                d.vel.y = 400.0;
            }

            // ---- 移动 + 像素碰撞 ----
            d.pos += d.vel / 60.0;
            let (px, py) = (d.pos.x as i32, d.pos.y as i32);
            if world.solid_px(px, py) {
                // 回退：先试分离 X，再 Y
                d.pos -= d.vel / 60.0;
                let nx = d.pos.x + d.vel.x / 60.0;
                if world.solid_px(nx as i32, d.pos.y as i32) {
                    d.vel.x *= -0.3;
                } else {
                    d.pos.x = nx;
                }
                let ny = d.pos.y + d.vel.y / 60.0;
                if world.solid_px(d.pos.x as i32, ny as i32) {
                    if d.vel.y > 0.0 {
                        d.vel.y = 0.0;
                        d.vel.x *= 0.8;
                    } else {
                        d.vel.y = 0.0;
                    }
                } else {
                    d.pos.y = ny;
                }
            }
            d.pos.y < (world.pixels.h + 64) as f32
        });
        (picked, bag_full)
    }

    /// 渲染：稀有度光柱 + 物品色块
    pub fn render(
        &self,
        batch: &mut SpriteBatch,
        db: &ItemDb,
        white: &Region,
        tl: Vec2,
        br: Vec2,
    ) {
        for d in &self.list {
            if d.pos.x < tl.x - 8.0 || d.pos.x > br.x + 8.0 || d.pos.y < tl.y - 16.0
                || d.pos.y > br.y + 16.0
            {
                continue;
            }
            let r = db.rarity(&d.item);
            let col = r.color();
            let bob = (d.age * 3.0).sin() * 1.2;
            let p = d.pos + Vec2::new(0.0, bob - 2.0);
            // 光柱（高 14px，宽 1.5px，半透明，稀有度越高越亮）
            let glow = 0.25 + 0.12 * (d.item.affixes.len() as f32);
            batch.push_at(p - Vec2::new(0.0, 6.0), Vec2::new(1.5, 14.0), white, [
                col[0], col[1], col[2], glow,
            ]);
            // 物品本体
            let size = if db.def(&d.item.def).stack > 1 { 2.5 } else { 3.0 };
            batch.push_at(p, Vec2::splat(size), white, col);
            batch.push_at(p - Vec2::new(0.5, 0.5), Vec2::splat(size * 0.4), white, [1.0; 4]);
        }
    }
}
