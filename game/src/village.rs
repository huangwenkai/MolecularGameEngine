//! 村庄雏形：安全度聚合 + 世界事件 + 动态任务（哥布林袭击）+ 世界历史（S1-9/10）
use glam::Vec2;

/// 动态任务
#[derive(Debug, Clone)]
pub struct Quest {
    pub name: &'static str,
    pub desc: &'static str,
    pub goal: u32,
    pub progress: u32,
    pub reward: u32,
}

/// 安全度：守卫数量 ×20（上限 100）
pub fn safety(app: &crate::GameApp) -> u32 {
    (app.npcs.list.iter().filter(|n| n.prof == 5).count() as u32 * 20).min(100)
}

/// 人口
pub fn population(app: &crate::GameApp) -> usize {
    app.npcs.list.len()
}

/// 记录世界事件（保留最近 80 条）
pub fn log_event(app: &mut crate::GameApp, desc: impl Into<String>) {
    let day = app.day;
    app.events.push((day, desc.into()));
    if app.events.len() > 80 {
        app.events.remove(0);
    }
}

/// 哥布林袭击触发判定（夜晚随机）——返回是否触发
pub fn try_raid(app: &mut crate::GameApp) -> bool {
    let night = app.world.time > 0.55 && app.world.time < 0.9;
    if app.raid.is_some() || app.quest.is_some() || !night {
        return false;
    }
    if !app.rng.chance(0.004) {
        return false;
    }
    // 玩家周围 140~220px 环形刷 4 只哥布林
    for _ in 0..4 {
        let a = app.rng.range_f32(0.0, 6.28);
        let d = app.rng.range_f32(140.0, 220.0);
        let px = (app.player.pos.x + a.cos() * d) as i32;
        let sy = app.surface_y(px) as f32;
        app.monsters.test_spawn(crate::monsters::Kind::Goblin, Vec2::new(px as f32, sy), &mut app.rng);
    }
    app.raid = Some(4);
    app.quest = Some(Quest {
        name: "保卫村庄",
        desc: "击退哥布林袭击者（4）",
        goal: 4,
        progress: 0,
        reward: 80,
    });
    log_event(app, "哥布林袭击了村庄！");
    app.hint = ("⚠ 哥布林袭击村庄！击退他们！".to_string(), 3.0);
    app.audio.play(crate::audio::Sfx::Explode);
    true
}

/// 击杀进度（怪物死亡时调用）
pub fn on_monster_killed(app: &mut crate::GameApp, count: u32) {
    let Some(q) = app.quest.as_mut() else { return };
    if app.raid.is_none() {
        return;
    }
    q.progress = (q.progress + count).min(q.goal);
    if q.progress >= q.goal {
        let reward = q.reward;
        let name = q.name;
        app.quest = None;
        app.raid = None;
        app.inv.give_coins(reward as u16);
        app.village_rep = (app.village_rep + 10).clamp(-100, 100);
        for n in app.npcs.list.iter_mut() {
            n.rel = (n.rel as i16 + 3).clamp(-100, 100) as i8;
            n.memories.push("与我并肩守卫村庄".into());
            if n.memories.len() > 6 {
                n.memories.remove(0);
            }
        }
        log_event(app, "村民与玩家击退了哥布林袭击！");
        app.hint = (format!("「{name}」完成！奖励 {reward} 金币，声望 +10"), 3.0);
        app.audio.play(crate::audio::Sfx::LevelUp);
    }
}
