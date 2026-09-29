//! 存档系统：世界像素（RLE）+ 玩家/背包/时间/火把（RON）
use crate::GameApp;
use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

pub const SAVE_DIR: &str = "saves";

#[derive(Serialize, Deserialize)]
struct Meta {
    player: PlayerData,
    inv: InvData,
    time: f32,
    torches: Vec<(i32, i32)>,
    /// 建筑件（M21 新增；serde(default) 兼容旧存档）
    #[serde(default)]
    pieces: Vec<mge_world::pieces::Piece>,
    /// 农作物（M21/S1-6 新增；serde(default) 兼容旧存档）
    #[serde(default)]
    crops: Vec<crate::farm::Crop>,
    /// NPC（M22/S1-11 新增；serde(default) 兼容旧存档）
    #[serde(default)]
    npcs: Vec<NpcSave>,
    /// 村庄声望 / 天数 / 世界历史
    #[serde(default)]
    village_rep: i32,
    #[serde(default)]
    day: u32,
    #[serde(default)]
    events: Vec<(u32, String)>,
    /// 技能（M17 新增；serde(default) 兼容旧存档）
    #[serde(default)]
    skills: crate::skills::SkillSave,
}

#[derive(Serialize, Deserialize)]
struct NpcSave {
    x: f32,
    y: f32,
    home_x: f32,
    home_y: f32,
    prof: u8,
    name: String,
    rel: i8,
    memories: Vec<String>,
}


#[derive(Serialize, Deserialize)]
struct PlayerData {
    x: f32,
    y: f32,
    hp: f32,
    facing: f32,
}

#[derive(Serialize, Deserialize)]
struct InvData {
    bag: Vec<Option<crate::items::Item>>,
    equip: Vec<Option<crate::items::Item>>,
    gold: u32,
    level: u32,
    xp: u32,
    points: u8,
    attr: crate::inventory::Attr,
}

fn save_paths(slot: u8) -> (PathBuf, PathBuf) {
    let slot = match slot {
        s @ 1..=3 => s,
        _ => 1,
    };
    let dir = PathBuf::from(SAVE_DIR);
    (dir.join(format!("save{slot}.ron")), dir.join(format!("save{slot}.px")))
}

/// F5：保存（世界 RLE + 元数据）
pub fn save_game(app: &mut GameApp) -> std::io::Result<()> {
    std::fs::create_dir_all(SAVE_DIR)?;
    let (meta_path, px_path) = save_paths(app.settings.slot);

    // ---- 世界像素 RLE：[mat][shade][count u16 LE] ----
    let raw = app.world.pixels.export_mat_shade();
    let mut px = Vec::with_capacity(raw.len() / 8 + 64);
    let mut i = 0;
    while i < raw.len() {
        let (m, s) = (raw[i], raw[i + 1]);
        let mut run = 1u16;
        while i + run as usize * 2 < raw.len()
            && raw[i + run as usize * 2] == m
            && raw[i + run as usize * 2 + 1] == s
            && run < u16::MAX
        {
            run += 1;
        }
        px.extend_from_slice(&[m, s, (run & 0xFF) as u8, (run >> 8) as u8]);
        i += run as usize * 2;
    }

    let meta = Meta {
        player: PlayerData {
            x: app.player.pos.x,
            y: app.player.pos.y,
            hp: app.player.hp,
            facing: app.player.facing,
        },
        inv: InvData {
            bag: app.inv.bag.clone(),
            equip: app.inv.equip.to_vec(),
            gold: app.inv.gold,
            level: app.inv.level,
            xp: app.inv.xp,
            points: app.inv.points,
            attr: app.inv.attr,
        },
        time: app.world.time,
        torches: app.world.torches.clone(),
        pieces: app.world.pieces.clone(),
        crops: app.crops.clone(),
        npcs: app
            .npcs
            .list
            .iter()
            .map(|n| NpcSave {
                x: n.pos.x,
                y: n.pos.y,
                home_x: n.home.x,
                home_y: n.home.y,
                prof: n.prof,
                name: n.name.clone(),
                rel: n.rel,
                memories: n.memories.clone(),
            })
            .collect(),
        village_rep: app.village_rep,
        day: app.day,
        events: app.events.clone(),
        skills: crate::skills::SkillSave {
            pts: app.skills.pts,
            learned: app.skills.learned.clone(),
        },
    };

    // 写入（先临时文件再改名，防半写）
    {
        let mut f = std::fs::File::create(px_path.with_extension("tmp"))?;
        f.write_all(&px)?;
        std::fs::rename(px_path.with_extension("tmp"), &px_path)?;
    }
    let text = ron::ser::to_string_pretty(&meta, Default::default())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::write(meta_path.with_extension("tmp"), text)?;
    std::fs::rename(meta_path.with_extension("tmp"), &meta_path)?;

    tracing::info!(
        "存档完成 | 像素 {} KB → {} KB | 火把 {}",
        raw.len() / 1024,
        px.len() / 1024,
        app.world.torches.len()
    );
    Ok(())
}

/// F9：读档
pub fn load_game(app: &mut GameApp) -> std::io::Result<()> {
    let (meta_path, px_path) = save_paths(app.settings.slot);
    let text = std::fs::read_to_string(&meta_path)?;
    let meta: Meta = ron::from_str(&text)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let px = std::fs::read(&px_path)?;

    // ---- 解 RLE → 原始流 ----
    let need = (app.world.pixels.w * app.world.pixels.h * 2) as usize;
    let mut raw = Vec::with_capacity(need);
    let mut i = 0;
    while i + 3 < px.len() {
        let (m, s) = (px[i], px[i + 1]);
        let run = px[i + 2] as u16 | ((px[i + 3] as u16) << 8);
        for _ in 0..run {
            raw.push(m);
            raw.push(s);
        }
        i += 4;
    }
    if raw.len() != need {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("像素流尺寸不符 {}/{}", raw.len(), need),
        ));
    }
    app.world.pixels.import_mat_shade(&raw);
    app.world.mark_terrain_dirty();

    // ---- 元数据 ----
    app.player.pos = Vec2::new(meta.player.x, meta.player.y);
    app.player.hp = meta.player.hp;
    app.player.facing = meta.player.facing;
    app.inv.bag = meta.inv.bag;
    app.inv.equip = meta.inv.equip.try_into().unwrap_or_default();
    app.inv.gold = meta.inv.gold;
    app.inv.level = meta.inv.level;
    app.inv.xp = meta.inv.xp;
    app.inv.points = meta.inv.points;
    app.inv.attr = meta.inv.attr;
    app.world.time = meta.time;
    app.world.torches = meta.torches;
    app.world.pieces = meta.pieces;
    app.crops = meta.crops;
    // NPC：按存档恢复（职业/名字/关系/记忆/住宅）
    app.npcs.list = meta
        .npcs
        .iter()
        .map(|s| crate::npc::Npc {
            pos: Vec2::new(s.x, s.y),
            vel: Vec2::ZERO,
            hunger: 20.0,
            thirst: 20.0,
            fatigue: 10.0,
            state: crate::npc::NpcState::Idle,
            state_t: 0,
            target: Vec2::new(s.home_x, s.home_y),
            home: Vec2::new(s.home_x, s.home_y),
            face: 1.0,
            anim: 0.0,
            prof: s.prof,
            name: s.name.clone(),
            rel: s.rel,
            memories: s.memories.clone(),
        })
        .collect();
    app.village_rep = meta.village_rep;
    app.day = meta.day;
    app.events = meta.events;
    app.skills.pts = meta.skills.pts;
    // learned 兼容旧存档（技能数演进）：对齐到当前技能定义数量
    app.skills.resize_to_defs();
    for (i, v) in meta.skills.learned.iter().enumerate() {
        if let Some(l) = app.skills.learned.get_mut(i) {
            *l = *v;
        }
    }
    // 怪物/掉落为动态实体，不存档
    app.monsters.list.clear();
    app.monsters.bullets.clear();
    app.drops.list.clear();

    tracing::info!("读档完成 | 等级 {} | 火把 {}", app.inv.level, app.world.torches.len());
    Ok(())
}
