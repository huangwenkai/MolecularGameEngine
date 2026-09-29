//! 农业系统：耕地 → 播种 → 生长 → 收获（S1-6）
//! 农具（Tool::Hoe）左键按目标自动判断：草/泥土表面耕地、耕地上播种（消耗麦种）、成熟收获。
use crate::GameApp;
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// 每阶段生长秒数（成熟 = MAX_STAGE × STAGE_TIME）
pub const STAGE_TIME: f32 = 20.0;
pub const MAX_STAGE: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crop {
    /// 茎基像素坐标（耕地顶面）
    pub x: i32,
    pub y: i32,
    /// 累计生长时间
    pub t: f32,
}

impl Crop {
    pub fn stage(&self) -> u8 {
        ((self.t / STAGE_TIME) as u8).min(MAX_STAGE)
    }

    pub fn mature(&self) -> bool {
        self.stage() >= MAX_STAGE
    }
}

/// 生长推进 + 耕地被破坏时移除作物
pub fn update(app: &mut GameApp) {
    let soil = app.world.mats.id("soil").unwrap_or(u8::MAX);
    app.crops.retain_mut(|c| {
        c.t += 1.0 / 60.0;
        app.world.pixels.get(c.x, c.y).mat == soil
    });
}

/// 消耗背包中 1 个麦种
fn consume_seed(app: &mut GameApp) -> bool {
    for slot in app.inv.bag.iter_mut() {
        if let Some(it) = slot {
            if it.def == "seeds_wheat" && it.count > 0 {
                it.count -= 1;
                if it.count == 0 {
                    *slot = None;
                }
                return true;
            }
        }
    }
    false
}

/// 农具左键：按目标自动判断耕地/播种/收获；返回提示文案
pub fn use_hoe(app: &mut GameApp) -> Option<&'static str> {
    let tx = app.mouse_world.x as i32;
    let ty = app.mouse_world.y as i32;
    let soil = app.world.mats.id("soil").unwrap_or(u8::MAX);

    // 1) 收获：同格有成熟作物
    if let Some(i) = app.crops.iter().position(|c| c.x == tx && c.y == ty) {
        if app.crops[i].mature() {
            app.crops.remove(i);
            let mut loot = vec![crate::items::Item {
                def: "wheat".into(),
                count: 2,
                affixes: vec![],
            }];
            if app.rng.chance(0.5) {
                loot.push(crate::items::Item {
                    def: "seeds_wheat".into(),
                    count: 1,
                    affixes: vec![],
                });
            }
            let pos = Vec2::new(tx as f32 + 0.5, ty as f32 - 3.0);
            app.drops.spawn_loot(loot, pos, &mut app.rng);
            app.audio.play(crate::audio::Sfx::Pickup);
            return Some("收获小麦 ×2");
        }
        return Some("还在生长……");
    }

    // 2) 播种：目标格是耕地且相邻无作物
    if app.world.pixels.get(tx, ty).mat == soil {
        if app.crops.iter().any(|c| c.y == ty && (c.x - tx).abs() <= 1) {
            return Some("这里已经种了");
        }
        if consume_seed(app) {
            app.crops.push(Crop { x: tx, y: ty, t: 0.0 });
            app.audio.play(crate::audio::Sfx::Place);
            return Some("播种小麦（约 40s 成熟）");
        }
        return Some("没有麦种（ESC 实验区可领取）");
    }

    // 3) 耕地：草/泥土表面 → 3 宽耕地
    let p = app.world.pixels.get(tx, ty);
    let md = app.world.mats.def(p.mat);
    if md.name == "grass" || md.name == "dirt" {
        let mut tilled = 0;
        for dx in -2..=2 {
            let q = app.world.pixels.get(tx + dx, ty);
            let qd = app.world.mats.def(q.mat);
            if qd.name == "grass" || qd.name == "dirt" {
                app.world.pixels.set(
                    tx + dx,
                    ty,
                    mge_sim::Pixel { mat: soil, shade: q.shade, life: 0, aux: 0 },
                );
                tilled += 1;
            }
        }
        if tilled > 0 {
            app.world.mark_terrain_dirty();
            app.audio.play(crate::audio::Sfx::Mine);
            return Some("耕地完成（再点播种）");
        }
    }
    None
}
