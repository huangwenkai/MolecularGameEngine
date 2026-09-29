//! 技能系统：主动技能（数据驱动 RON + 学习/升级/冷却/施放）—— M20
//!
//! 技能定义在 game/assets/data/skills.ron（工程目录可覆盖）：
//! 名称/键位/冷却/伤害成长/效果类型（kind）/状态词条（status）。
//! 玩家每升 1 级获得 1 技能点，技能面板 [K] 学习/升级；Z/X/C/V/R/G 施放。
use crate::audio;
use crate::entities;
use crate::GameApp;
use glam::Vec2;
use mge_core::math::Aabb;
use mge_platform::action::Action;
use mge_runtime::EngineCtx;
use std::sync::OnceLock;

/// 状态效果词条（命中/施加）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StatusSpec {
    pub kind: String, // burn | poison | frozen
    pub dur: f32,
}

/// 技能定义（数据驱动，skills.ron）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    pub key_hint: String,
    pub desc: String,
    pub cd: f32,
    /// 效果类型：melee_aoe | nova | heal | strike | aoe_self
    pub kind: String,
    #[serde(default)]
    pub dmg_base: f32,
    #[serde(default)]
    pub dmg_per_lv: f32,
    /// AoE 半径 / 打击范围
    #[serde(default)]
    pub radius: f32,
    /// nova 弹数
    #[serde(default)]
    pub count: u32,
    /// nova 弹速
    #[serde(default)]
    pub speed: f32,
    /// nova 投射物：fireball | frost | poison
    #[serde(default)]
    pub proj: String,
    #[serde(default)]
    pub heal_base: f32,
    #[serde(default)]
    pub heal_per_lv: f32,
    /// 状态词条
    #[serde(default)]
    pub status: Option<StatusSpec>,
}

#[derive(Debug, serde::Deserialize)]
struct SkillsRoot {
    skills: Vec<SkillDef>,
}

fn embedded_defs() -> Vec<SkillDef> {
    ron::from_str::<SkillsRoot>(include_str!("../assets/data/skills.ron"))
        .expect("skills.ron 解析失败")
        .skills
}

static DEFS: OnceLock<Vec<SkillDef>> = OnceLock::new();

/// 技能定义表（首访加载；工程目录 data/skills.ron 可覆盖内置表）
pub fn defs() -> &'static [SkillDef] {
    DEFS.get_or_init(|| {
        let p = crate::project::dir_of("data").join("skills.ron");
        if let Ok(s) = std::fs::read_to_string(&p) {
            match ron::from_str::<SkillsRoot>(&s) {
                Ok(r) if !r.skills.is_empty() => {
                    tracing::info!("技能表加载（工程覆盖）：{} 个技能", r.skills.len());
                    return r.skills;
                }
                Ok(_) => tracing::warn!("工程 skills.ron 为空，使用内置技能表"),
                Err(e) => tracing::warn!("工程 skills.ron 解析失败: {e}，使用内置技能表"),
            }
        }
        embedded_defs()
    })
}

pub const SKILL_MAX_LV: u8 = 5;

/// 存档用快照（冷却不存）。
/// learned 用 Vec：技能数量演进时旧档自动补零对齐。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SkillSave {
    pub pts: u8,
    pub learned: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Skills {
    /// 未分配技能点
    pub pts: u8,
    /// 各技能等级（0 = 未学习）
    pub learned: Vec<u8>,
    /// 技能面板是否打开（独立面板，默认 K）
    pub ui_open: bool,
    /// 剩余冷却（秒）
    cds: Vec<f32>,
}

impl Skills {
    /// 对齐技能定义数量（启动/读档后调用）
    pub fn resize_to_defs(&mut self) {
        let n = defs().len();
        self.learned.resize(n, 0);
        self.cds.resize(n, 0.0);
    }

    /// 某等级下的实际冷却（秒）
    pub fn cd_of(&self, i: usize) -> f32 {
        let lv = self.learned.get(i).copied().unwrap_or(0).max(1) as f32;
        defs()[i].cd * (1.0 - 0.08 * (lv - 1.0))
    }

    pub fn cd_remaining(&self, i: usize) -> f32 {
        self.cds.get(i).copied().unwrap_or(0.0)
    }

    /// 学习/升级（消耗 1 技能点）；成功返回 true
    pub fn learn(&mut self, i: usize) -> bool {
        if self.pts == 0 || self.learned.get(i).copied().unwrap_or(0) >= SKILL_MAX_LV {
            return false;
        }
        if let Some(l) = self.learned.get_mut(i) {
            *l += 1;
        }
        self.pts -= 1;
        true
    }

    /// 免费习得 1 级（实验按钮用）
    pub fn grant(&mut self, i: usize) {
        if let Some(l) = self.learned.get_mut(i) {
            *l = (*l).max(1);
        }
        self.cds.resize(defs().len(), 0.0);
    }
}

/// 每 tick 调用：冷却推进 + Z/X/C 施放
pub fn tick(app: &mut GameApp, ctx: &mut EngineCtx) {
    for cd in app.skills.cds.iter_mut() {
        *cd = (*cd - 1.0 / 60.0).max(0.0);
    }
    let keys = [
        Action::Skill1,
        Action::Skill2,
        Action::Skill3,
        Action::Skill4,
        Action::Skill5,
        Action::Skill6,
    ];
    let want = keys.iter().position(|a| ctx.input.just_pressed(*a));
    let Some(i) = want else { return };
    let Some(def) = defs().get(i) else { return };
    if app.skills.learned.get(i).copied().unwrap_or(0) == 0 {
        app.hint = (format!(
            "尚未学习「{}」（按 K 打开技能面板，用技能点学习）",
            def.name
        ), 2.0);
        return;
    }
    if app.skills.cds.get(i).copied().unwrap_or(0.0) > 0.0 {
        app.hint = (
            format!("「{}」冷却中 {:.1}s", def.name, app.skills.cd_remaining(i)),
            1.0,
        );
        return;
    }
    let lv = app.skills.learned[i] as f32;
    let ok = match def.kind.as_str() {
        "melee_aoe" => cast_melee_aoe(app, ctx, def, lv),
        "nova" => cast_nova(app, def),
        "heal" => cast_heal(app, def, lv),
        "strike" => cast_strike(app, ctx, def, lv),
        "aoe_self" => cast_aoe_self(app, ctx, def, lv),
        other => {
            app.hint = (format!("未知技能效果类型「{other}」"), 1.5);
            false
        }
    };
    if ok {
        app.skills.cds[i] = app.skills.cd_of(i);
        app.player.casting = 0.35; // 施法姿态（手臂上举 + 聚能光效）
    }
}

/// 近身 AoE（旋风斩类）：周身范围伤害（怪物 + 假人）
fn cast_melee_aoe(app: &mut GameApp, ctx: &mut EngineCtx, def: &SkillDef, lv: f32) -> bool {
    let st = app.inv.aggregate(&app.db);
    let dmg = st.damage(def.dmg_base + def.dmg_per_lv * (lv - 1.0));
    let hb = Aabb::new(
        app.player.pos - Vec2::new(0.0, 10.0),
        Vec2::new(def.radius, def.radius * 0.6),
    );
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
    // 旋风特效：环形白光粒子（半径随定义）+ 挥砍音效
    let rr = def.radius * 0.5;
    for k in 0..20 {
        let a = k as f32 / 20.0 * std::f32::consts::TAU;
        app.vfx.dot(
            app.player.pos - Vec2::new(0.0, 10.0)
                + Vec2::new(a.cos() * rr, a.sin() * rr * 0.55),
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

/// 环形投射物新星（火焰/冰霜等，弹种由数据决定）
fn cast_nova(app: &mut GameApp, def: &SkillDef) -> bool {
    let hand = app.player.pos + Vec2::new(0.0, -10.0);
    let kind = match def.proj.as_str() {
        "frost" => crate::projectiles::ProjKind::FrostBolt,
        "poison" => crate::projectiles::ProjKind::PoisonBolt,
        _ => crate::projectiles::ProjKind::Fireball,
    };
    for k in 0..def.count {
        let a = k as f32 / def.count as f32 * std::f32::consts::TAU;
        let dir = Vec2::new(a.cos(), a.sin()).normalize_or_zero();
        app.projectiles
            .spawn(kind, hand + dir * 8.0, dir * def.speed);
    }
    app.audio.play(audio::Sfx::Shoot);
    true
}

/// 治疗：满血时不施放（不进冷却）
fn cast_heal(app: &mut GameApp, def: &SkillDef, lv: f32) -> bool {
    if app.player.hp >= app.player.max_hp {
        app.hint = ("生命值已满，治疗术未施放".to_string(), 1.0);
        return false;
    }
    let amount = def.heal_base + def.heal_per_lv * (lv - 1.0);
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

/// 落雷（闪电术类）：雷击离鼠标最近的敌人；无敌人时轰击鼠标落点（小范围 AoE）
fn cast_strike(app: &mut GameApp, ctx: &mut EngineCtx, def: &SkillDef, lv: f32) -> bool {
    const RANGE: f32 = 320.0; // 施法距离（玩家到落点）
    let aoe = def.radius.max(8.0);
    let st = app.inv.aggregate(&app.db);
    let dmg = st.damage(def.dmg_base + def.dmg_per_lv * (lv - 1.0));

    // ---- 选落点：距鼠标最近且在施法距离内的敌人；否则鼠标处（向玩家方向夹回 RANGE）----
    let mouse = app.mouse_world;
    let mut strike = mouse;
    {
        let best = app
            .monsters
            .list
            .iter()
            .filter(|m| m.hp > 0.0 && (m.pos - app.player.pos).length() <= RANGE)
            .min_by(|a, b| {
                (a.pos - mouse)
                    .length()
                    .total_cmp(&(b.pos - mouse).length())
            });
        if let Some(m) = best {
            strike = m.pos - Vec2::new(0.0, m.half.y);
        } else {
            let d = strike - app.player.pos;
            let dist = d.length();
            if dist > RANGE {
                strike = app.player.pos + d / dist * RANGE;
            }
        }
    }

    // ---- 天降雷击：真锯齿闪电链（自上而下，主线+分支）----
    let top = strike + Vec2::new(0.0, -240.0);
    app.vfx.bolt(top, strike, &mut app.rng, 0.22);
    let flick = strike
        + Vec2::new(app.rng.range_f32(-16.0, 16.0), 0.0)
        + Vec2::new(0.0, -60.0);
    app.vfx.bolt(top + Vec2::new(3.0, 0.0), flick, &mut app.rng, 0.13);
    // 落点闪光 + 四散火花
    app.vfx.dot(strike, Vec2::ZERO, 0.3, 4.0, [1.0, 1.0, 1.0], 0.0, true);
    for _ in 0..10 {
        let a = app.rng.range_f32(0.0, 6.28);
        app.vfx.dot(
            strike,
            Vec2::new(a.cos() * 90.0, a.sin() * 50.0 - 40.0),
            0.4,
            1.4,
            [0.7, 0.85, 1.0],
            120.0,
            true,
        );
    }

    // ---- 伤害：落点 AoE 内所有怪物（直击感：受击闪白 + 击退）----
    let hb = Aabb::new(strike - Vec2::splat(aoe), Vec2::splat(aoe * 2.0));
    let hits = app.monsters.melee_hit(&hb, dmg, app.player.facing, true);
    for hp_pos in &hits {
        app.audio.play(audio::Sfx::Hit);
        app.vfx.text(*hp_pos + Vec2::new(0.0, -18.0), dmg as u32, false);
    }
    // 假人同样受击
    let fx = app.weapons.clone();
    let mut query = app
        .ecs
        .query::<(&entities::Transform, &mut entities::Dummy)>();
    for (_e, (tr, dm)) in query.iter() {
        if dm.respawn > 0 {
            continue;
        }
        let da = Aabb::new(tr.pos - Vec2::new(0.0, 10.0), Vec2::new(6.0, 10.0));
        if hb.intersects(&da) {
            dm.hp -= dmg;
            dm.flash = 0.18;
            let _ = app.vfx.spawn(&fx.hit, tr.pos + Vec2::new(0.0, -10.0), 1.0, &mut app.rng);
            app.vfx.text(tr.pos + Vec2::new(0.0, -18.0), dmg as u32, false);
        }
    }

    app.audio.play(audio::Sfx::Explode);
    ctx.camera.add_shake(4.0);
    true
}

/// 自身毒域（毒爆类）：以玩家为中心的范围中毒 + 伤害
fn cast_aoe_self(app: &mut GameApp, ctx: &mut EngineCtx, def: &SkillDef, lv: f32) -> bool {
    let st = app.inv.aggregate(&app.db);
    let dmg = st.damage(def.dmg_base + def.dmg_per_lv * (lv - 1.0));
    let r = def.radius.max(20.0);
    // 毒环视觉
    let n = 24;
    for k in 0..n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        app.vfx.dot(
            app.player.pos - Vec2::new(0.0, 10.0)
                + Vec2::new(a.cos() * r, a.sin() * r * 0.5),
            Vec2::new(a.cos() * 90.0, a.sin() * 45.0 - 30.0),
            0.5,
            2.2,
            [0.35, 0.9, 0.3],
            60.0,
            true,
        );
    }
    // 范围内怪物受击 + 中毒
    let hb = Aabb::new(
        app.player.pos - Vec2::new(0.0, 10.0) - Vec2::splat(r),
        Vec2::splat(r * 2.0),
    );
    let hits = app.monsters.melee_hit(&hb, dmg, app.player.facing, false);
    for hp_pos in &hits {
        app.audio.play(audio::Sfx::Hit);
        app.vfx.text(*hp_pos + Vec2::new(0.0, -18.0), dmg as u32, false);
        if let Some(st_spec) = &def.status {
            if let Some(kind) = status_kind(&st_spec.kind) {
                app.monsters.apply_status_at(*hp_pos, kind, st_spec.dur);
            }
        }
    }
    // 假人
    let fx = app.weapons.clone();
    let mut query = app
        .ecs
        .query::<(&entities::Transform, &mut entities::Dummy)>();
    for (_e, (tr, dm)) in query.iter() {
        if dm.respawn > 0 {
            continue;
        }
        let da = Aabb::new(tr.pos - Vec2::new(0.0, 10.0), Vec2::new(6.0, 10.0));
        if hb.intersects(&da) {
            dm.hp -= dmg;
            dm.flash = 0.18;
            let _ = app.vfx.spawn(&fx.hit, tr.pos + Vec2::new(0.0, -10.0), 1.0, &mut app.rng);
            app.vfx.text(tr.pos + Vec2::new(0.0, -18.0), dmg as u32, false);
        }
    }
    app.audio.play(audio::Sfx::Explode);
    ctx.camera.add_shake(3.0);
    true
}

/// 状态词条字符串 → StatusKind
fn status_kind(s: &str) -> Option<crate::monsters::StatusKind> {
    match s {
        "burn" => Some(crate::monsters::StatusKind::Burn),
        "poison" => Some(crate::monsters::StatusKind::Poison),
        "frozen" => Some(crate::monsters::StatusKind::Frozen),
        _ => None,
    }
}

/// 技能 HUD（左下角）：键位 + 冷却状态
pub fn draw_hud(app: &GameApp, ctx: &egui::Context) {
    // 设置面板/引擎 IDE 打开时不绘制（IDE 独占界面，HUD 为 Area 前景层会压在其上）
    if app.settings_ui.open || app.ide.open {
        return;
    }
    egui::Area::new(egui::Id::new("skills_hud"))
        .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -16.0])
        .show(ctx, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(120.0);
                ui.label(format!("技能点 {}", app.skills.pts));
                for (i, def) in defs().iter().enumerate() {
                    let lv = app.skills.learned.get(i).copied().unwrap_or(0);
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
                // 闪电魔法书自带法术（独立冷却，与技能系统无关）
                if app.has_tome() {
                    let cd = app.tome_cd;
                    let (txt, color) = if cd > 0.0 {
                        (
                            format!("📖 引雷 {:.1}s [左键]", cd),
                            egui::Color32::from_rgb(230, 140, 90),
                        )
                    } else {
                        ("📖 引雷 就绪 [左键]".to_string(), egui::Color32::from_rgb(120, 180, 255))
                    };
                    ui.colored_label(color, txt);
                }
                // 武器淬毒状态
                if app.player.poison_buff > 0.0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 220, 110),
                        format!("🧪 淬毒 {:.1}s", app.player.poison_buff),
                    );
                }
            });
        });
}

/// 技能面板（独立窗口，默认 K 打开）：技能点 + 学习/升级
pub fn draw_window(app: &mut GameApp, ctx: &egui::Context) {
    if !app.skills.ui_open {
        return;
    }
    egui::Window::new("技能 [K]")
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.heading("技能树");
            if app.skills.pts > 0 {
                ui.colored_label(
                    egui::Color32::GOLD,
                    format!("可用技能点 {}（每升 1 级获得 1 点）", app.skills.pts),
                );
            } else {
                ui.weak("升级可获得技能点（每级 1 点）");
            }
            ui.separator();
            for (i, def) in defs().iter().enumerate() {
                let lv = app.skills.learned.get(i).copied().unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.monospace(format!(
                        "[{}] {}{}",
                        def.key_hint,
                        def.name,
                        if lv > 0 { format!(" Lv.{lv}") } else { String::new() }
                    ));
                    if lv == 0 {
                        ui.add_enabled_ui(app.skills.pts > 0, |ui| {
                            if ui.button("学习").clicked() {
                                app.skills.learn(i);
                            }
                        });
                    } else if lv < SKILL_MAX_LV {
                        ui.add_enabled_ui(app.skills.pts > 0, |ui| {
                            if ui.button("升级").clicked() {
                                app.skills.learn(i);
                            }
                        });
                    } else {
                        ui.weak("MAX");
                    }
                });
                ui.horizontal(|ui| {
                    ui.small(def.desc.as_str());
                    if lv > 0 {
                        ui.small(format!("冷却 {:.1}s", app.skills.cd_of(i)));
                    }
                });
            }
            ui.separator();
            ui.small("技能定义：data/skills.ron（数据驱动，工程目录可覆盖）；Z/X/C/V/R/G 施放。");
        });
}
