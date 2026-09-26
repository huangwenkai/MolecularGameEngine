//! 技能系统：主动技能（学习/升级/冷却/施放）—— 暗黑成长层补全（M17）
//!
//! 玩家每升 1 级获得 1 技能点（升级主循环里累加），
//! 在背包面板 [I] 的技能区学习/升级；Z/X/C 施放，技能 HUD 显示冷却。
use crate::audio;
use crate::entities;
use crate::GameApp;
use glam::Vec2;
use mge_core::math::Aabb;
use mge_platform::action::Action;
use mge_runtime::EngineCtx;

/// 技能静态定义
pub struct SkillDef {
    pub name: &'static str,
    pub key_hint: &'static str,
    pub desc: &'static str,
    /// 1 级基础冷却（秒）；每升 1 级 -8%
    pub cd: f32,
}

pub const SKILLS: [SkillDef; 3] = [
    SkillDef {
        name: "旋风斩",
        key_hint: "Z",
        desc: "对周身敌人造成范围伤害（14 + 8/级）",
        cd: 8.0,
    },
    SkillDef {
        name: "火焰新星",
        key_hint: "X",
        desc: "向四周发射 8 枚火球",
        cd: 12.0,
    },
    SkillDef {
        name: "治疗术",
        key_hint: "C",
        desc: "恢复生命（30 + 20/级）",
        cd: 18.0,
    },
];

pub const SKILL_MAX_LV: u8 = 5;

/// 存档用快照（冷却不存）
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct SkillSave {
    pub pts: u8,
    pub learned: [u8; 3],
}

#[derive(Debug, Clone, Default)]
pub struct Skills {
    /// 未分配技能点
    pub pts: u8,
    /// 各技能等级（0 = 未学习）
    pub learned: [u8; 3],
    /// 剩余冷却（秒）
    cds: [f32; 3],
}

impl Skills {
    /// 某等级下的实际冷却（秒）
    pub fn cd_of(&self, i: usize) -> f32 {
        let lv = self.learned[i].max(1) as f32;
        SKILLS[i].cd * (1.0 - 0.08 * (lv - 1.0))
    }

    pub fn cd_remaining(&self, i: usize) -> f32 {
        self.cds[i]
    }

    /// 学习/升级（消耗 1 技能点）；成功返回 true
    pub fn learn(&mut self, i: usize) -> bool {
        if self.pts == 0 || self.learned[i] >= SKILL_MAX_LV {
            return false;
        }
        self.learned[i] += 1;
        self.pts -= 1;
        true
    }
}

/// 每 tick 调用：冷却推进 + Z/X/C 施放
pub fn tick(app: &mut GameApp, ctx: &mut EngineCtx) {
    for cd in app.skills.cds.iter_mut() {
        *cd = (*cd - 1.0 / 60.0).max(0.0);
    }
    let want = if ctx.input.just_pressed(Action::Skill1) {
        Some(0)
    } else if ctx.input.just_pressed(Action::Skill2) {
        Some(1)
    } else if ctx.input.just_pressed(Action::Skill3) {
        Some(2)
    } else {
        None
    };
    let Some(i) = want else { return };
    if app.skills.learned[i] == 0 {
        app.hint =
            (format!("尚未学习「{}」（打开背包 [I]，用技能点学习）", SKILLS[i].name), 2.0);
        return;
    }
    if app.skills.cds[i] > 0.0 {
        app.hint = (format!("「{}」冷却中 {:.1}s", SKILLS[i].name, app.skills.cds[i]), 1.0);
        return;
    }
    let ok = match i {
        0 => cast_whirlwind(app, ctx),
        1 => cast_fire_nova(app),
        _ => cast_heal(app),
    };
    if ok {
        app.skills.cds[i] = app.skills.cd_of(i);
    }
}

/// 旋风斩：周身 AoE（怪物 + 假人），返回是否施放成功
fn cast_whirlwind(app: &mut GameApp, ctx: &mut EngineCtx) -> bool {
    let st = app.inv.aggregate(&app.db);
    let lv = app.skills.learned[0] as f32;
    let dmg = st.damage(14.0 + 8.0 * (lv - 1.0));
    let hb = Aabb::new(app.player.pos - Vec2::new(0.0, 10.0), Vec2::new(56.0, 32.0));
    let fx = app.weapons.clone();

    // 怪物
    let hits = app.monsters.melee_hit(&hb, dmg, app.player.facing, false);
    for hp_pos in &hits {
        app.audio.play(audio::Sfx::Hit);
        let _ = app.vfx.spawn(&fx.hit, *hp_pos, app.player.facing, &mut app.rng);
        app.vfx.text(*hp_pos + Vec2::new(0.0, -18.0), dmg as u32, false);
    }
    // 假人
    let mut query = app
        .ecs
        .query::<(&entities::Transform, &mut entities::Dummy, &mut entities::Vel)>();
    for (_e, (tr, dm, vel)) in query.iter() {
        if dm.respawn > 0 {
            continue;
        }
        let da = Aabb::new(tr.pos - Vec2::new(0.0, 10.0), Vec2::new(6.0, 10.0));
        if hb.intersects(&da) {
            dm.hp -= dmg;
            dm.flash = 0.18;
            vel.v += Vec2::new(120.0 * app.player.facing, -30.0) / 60.0;
            let _ = app.vfx.spawn(
                &fx.hit,
                tr.pos + Vec2::new(0.0, -10.0),
                app.player.facing,
                &mut app.rng,
            );
            app.vfx.text(tr.pos + Vec2::new(0.0, -18.0), dmg as u32, false);
        }
    }
    // 旋风特效：环形白光粒子 + 挥砍音效
    for k in 0..20 {
        let a = k as f32 / 20.0 * std::f32::consts::TAU;
        app.vfx.dot(
            app.player.pos - Vec2::new(0.0, 10.0)
                + Vec2::new(a.cos() * 26.0, a.sin() * 14.0),
            Vec2::new(a.cos() * 120.0, a.sin() * 60.0 - 20.0),
            0.35,
            1.6,
            [0.8, 0.9, 1.0],
            -20.0,
            true,
        );
    }
    app.audio.play(audio::Sfx::Swing);
    ctx.camera.add_shake(3.0);
    true
}

/// 火焰新星：8 方向火球
fn cast_fire_nova(app: &mut GameApp) -> bool {
    let hand = app.player.pos + Vec2::new(0.0, -10.0);
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        let dir = Vec2::new(a.cos(), a.sin()).normalize_or_zero();
        app.projectiles.spawn(
            crate::projectiles::ProjKind::Fireball,
            hand + dir * 8.0,
            dir * 200.0,
        );
    }
    app.audio.play(audio::Sfx::Shoot);
    true
}

/// 治疗术：满血时不施放（不进冷却）
fn cast_heal(app: &mut GameApp) -> bool {
    if app.player.hp >= app.player.max_hp {
        app.hint = ("生命值已满，治疗术未施放".to_string(), 1.0);
        return false;
    }
    let lv = app.skills.learned[2] as f32;
    let amount = 30.0 + 20.0 * (lv - 1.0);
    app.player.hp = (app.player.hp + amount).min(app.player.max_hp);
    for _ in 0..14 {
        let a = app.rng.range_f32(0.0, 6.28);
        app.vfx.dot(
            app.player.pos
                + Vec2::new(app.rng.range_f32(-6.0, 6.0), app.rng.range_f32(0.0, 14.0)),
            Vec2::new(a.cos() * -30.0, a.sin() * -30.0 - 30.0),
            0.45,
            1.6,
            [0.4, 1.0, 0.5],
            -40.0,
            true,
        );
    }
    app.audio.play(audio::Sfx::Pickup);
    true
}

/// 技能 HUD（左下角）：键位 + 冷却状态
pub fn draw_hud(app: &GameApp, ctx: &egui::Context) {
    if app.settings_ui.open {
        return;
    }
    egui::Area::new(egui::Id::new("skills_hud"))
        .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -16.0])
        .show(ctx, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(120.0);
                ui.label(format!("技能点 {}", app.skills.pts));
                for (i, def) in SKILLS.iter().enumerate() {
                    let lv = app.skills.learned[i];
                    let (txt, color) = if lv == 0 {
                        (format!("[{}] {} 未学习", def.key_hint, def.name), egui::Color32::GRAY)
                    } else {
                        let cd = app.skills.cd_remaining(i);
                        if cd > 0.0 {
                            (
                                format!("[{}] {} {:.1}s", def.key_hint, def.name, cd),
                                egui::Color32::from_rgb(230, 140, 90),
                            )
                        } else {
                            (
                                format!("[{}] {} Lv.{} 就绪", def.key_hint, def.name, lv),
                                egui::Color32::from_rgb(140, 230, 150),
                            )
                        }
                    };
                    ui.colored_label(color, txt);
                }
            });
        });
}
