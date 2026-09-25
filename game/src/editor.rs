//! 特效编辑器 v1：egui 调参面板 + 实时预览 + vfx.ron 保存/热重载 + 武器挂点映射
//! 验收标准：做一个新特效并挂到武器上，全程不重启
use crate::vfx::{Blueprint, Emitter};
use crate::GameApp;
use egui::ComboBox;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 运行时数据文件路径（编辑器读写；编译期嵌入仅作初始兜底）
pub const VFX_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/data/vfx.ron");
pub const WEAPONS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/data/weapons.ron");
pub const MATERIALS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/data/materials.ron");
pub const SHADERS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/shaders/");

const SHADERS: &[(&str, mge_render::renderer::ShaderKind)] = &[
    ("sprite.wgsl", mge_render::renderer::ShaderKind::Sprite),
    ("pixels.wgsl", mge_render::renderer::ShaderKind::Pixels),
    ("composite.wgsl", mge_render::renderer::ShaderKind::Composite),
    ("bloom.wgsl", mge_render::renderer::ShaderKind::Bloom),
];

/// 武器 → 特效蓝图名映射（数据驱动，热重载）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponFx {
    #[serde(default)]
    pub slash: String,
    #[serde(default)]
    pub hit: String,
    #[serde(default)]
    pub crit: String,
    #[serde(default)]
    pub arrow_hit: String,
    #[serde(default)]
    pub hit_spark: String,
    #[serde(default)]
    pub explosion: String,
    #[serde(default)]
    pub fizz: String,
    #[serde(default)]
    pub death: String,
}

impl Default for WeaponFx {
    fn default() -> Self {
        Self {
            slash: "sword_slash".into(),
            hit: "hit_spark".into(),
            crit: "crit_spark".into(),
            arrow_hit: "arrow_hit".into(),
            hit_spark: "hit_spark".into(),
            explosion: "explosion".into(),
            fizz: "fire_fizz".into(),
            death: "dummy_death".into(),
        }
    }
}

/// 编辑器状态
#[derive(Default)]
pub struct VfxEditor {
    pub open: bool,
    /// 当前页：0 特效 / 1 动画
    pub tab: usize,
    /// 选中蓝图名
    pub sel: String,
    /// 预览触发标记（tick 中消费，因为需要相机等）
    pub trigger: bool,
    vfx_mtime: Option<std::time::SystemTime>,
    wpn_mtime: Option<std::time::SystemTime>,
    mat_mtime: Option<std::time::SystemTime>,
    anims_mtime: Option<std::time::SystemTime>,
    shader_mtimes: [Option<std::time::SystemTime>; 4],
    /// 待重载的着色器（tick 中消费：需要 renderer）
    pub shader_req: Vec<(mge_render::renderer::ShaderKind, String)>,
    // ---- 动画页 ----
    /// 选中动画名
    pub anim_sel: String,
    /// "加载精灵表"请求（tick 中消费：需要 renderer 上传图集）
    pub anim_load_req: bool,
    pub anim_err: Option<String>,
    /// 统一帧时长（编辑用）
    pub anim_time: f32,
}

/// 读取武器映射（文件缺失/损坏时用默认）
pub fn load_weapons() -> WeaponFx {
    match std::fs::read_to_string(WEAPONS_PATH) {
        Ok(s) => ron::from_str(&s).unwrap_or_else(|e| {
            tracing::warn!("weapons.ron 解析失败，使用默认: {e}");
            WeaponFx::default()
        }),
        Err(_) => WeaponFx::default(),
    }
}

/// 保存武器映射
pub fn save_weapons(app: &mut GameApp) {
    match ron::ser::to_string_pretty(&app.weapons, Default::default()) {
        Ok(s) => {
            if let Err(e) = std::fs::write(WEAPONS_PATH, &s) {
                tracing::error!("weapons.ron 保存失败: {e}");
            } else {
                tracing::info!("weapons.ron 已保存");
                if let Ok(m) = mtime(WEAPONS_PATH) {
                    app.editor.wpn_mtime = Some(m);
                }
            }
        }
        Err(e) => tracing::error!("weapons.ron 序列化失败: {e}"),
    }
}

/// 保存全部蓝图到 vfx.ron（立即生效，武器无需重启）
pub fn save_vfx(app: &mut GameApp) {
    match ron::ser::to_string_pretty(&app.vfx.bps, Default::default()) {
        Ok(s) => {
            if let Err(e) = std::fs::write(VFX_PATH, &s) {
                tracing::error!("vfx.ron 保存失败: {e}");
            } else {
                tracing::info!("vfx.ron 已保存（{} 个蓝图）", app.vfx.bps.len());
                if let Ok(m) = mtime(VFX_PATH) {
                    app.editor.vfx_mtime = Some(m);
                }
            }
        }
        Err(e) => tracing::error!("vfx.ron 序列化失败: {e}"),
    }
}

fn mtime(path: &str) -> std::io::Result<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified())
}

/// mtime 轮询热重载（每 N tick 调一次；编辑器打开时跳过 vfx 覆盖防丢编辑）。
/// 返回 true 表示材质表被重载（调用方需重建调色板）。
pub fn reload_if_changed(app: &mut GameApp) -> bool {
    let mut mat_reloaded = false;
    // ---- materials.ron：追加式新材质（改 id 顺序会破坏存档与既有像素）----
    if let Ok(m) = mtime(MATERIALS_PATH) {
        let changed = app.editor.mat_mtime.map(|b| b != m).unwrap_or(true);
        if changed && !app.editor.open {
            match std::fs::read_to_string(MATERIALS_PATH) {
                Ok(s) => match mge_sim::Materials::from_ron(&s) {
                    Ok(new_mats) => {
                        app.world.mats = new_mats;
                        app.world.pixels.ids = mge_sim::SimIds::new(&app.world.mats);
                        mat_reloaded = true;
                        tracing::info!("materials.ron 热重载完成");
                    }
                    Err(e) => tracing::warn!("materials.ron 热重载解析失败: {e}"),
                },
                Err(e) => tracing::warn!("materials.ron 读取失败: {e}"),
            }
            app.editor.mat_mtime = Some(m);
        } else if app.editor.mat_mtime.is_none() {
            app.editor.mat_mtime = Some(m);
        }
    }
    // ---- weapons.ron：随时可重载 ----
    if let Ok(m) = mtime(WEAPONS_PATH) {
        let changed = app.editor.wpn_mtime.map(|b| b != m).unwrap_or(true);
        if changed {
            app.weapons = load_weapons();
            app.editor.wpn_mtime = Some(m);
        }
    }
    // ---- vfx.ron：面板打开时不自动覆盖 ----
    if let Ok(m) = mtime(VFX_PATH) {
        let changed = app.editor.vfx_mtime.map(|b| b != m).unwrap_or(true);
        if changed && !app.editor.open {
            match std::fs::read_to_string(VFX_PATH) {
                Ok(s) => match ron::from_str::<HashMap<String, Blueprint>>(&s) {
                    Ok(bps) => {
                        app.vfx.bps = bps;
                        tracing::info!("vfx.ron 热重载完成（{} 个蓝图）", app.vfx.bps.len());
                    }
                    Err(e) => tracing::warn!("vfx.ron 热重载解析失败: {e}"),
                },
                Err(e) => tracing::warn!("vfx.ron 读取失败: {e}"),
            }
        }
        if changed && app.editor.vfx_mtime.is_none() {
            app.editor.vfx_mtime = Some(m);
        } else if changed && !app.editor.open {
            app.editor.vfx_mtime = Some(m);
        }
    }
    // ---- animations.ron：面板打开时跳过（防丢编辑）----
    if let Ok(m) = mtime(crate::anim::ANIMS_PATH) {
        let changed = app.editor.anims_mtime.map(|b| b != m).unwrap_or(true);
        if changed && !app.editor.open {
            let bank = crate::anim::AnimBank::load();
            app.anims.defs = bank.defs;
            tracing::info!("animations.ron 热重载完成（{} 个动画）", app.anims.defs.len());
            app.editor.anims_mtime = Some(m);
        } else if app.editor.anims_mtime.is_none() {
            app.editor.anims_mtime = Some(m);
        }
    }
    // ---- WGSL 着色器热重载（改文件不重启）----
    for (i, (name, kind)) in SHADERS.iter().enumerate() {
        let path = format!("{SHADERS_DIR}{name}");
        if let Ok(m) = mtime(&path) {
            let changed = app.editor.shader_mtimes[i].map(|b| b != m).unwrap_or(false);
            if changed {
                match std::fs::read_to_string(&path) {
                    Ok(s) => app.editor.shader_req.push((*kind, s)),
                    Err(e) => tracing::warn!("{name} 读取失败: {e}"),
                }
            }
            app.editor.shader_mtimes[i] = Some(m);
        }
    }
    mat_reloaded
}

/// egui 面板（App::render 中调用）：特效 / 动画 双页
pub fn draw(app: &mut GameApp, ctx: &egui::Context) {
    if !app.editor.open {
        return;
    }
    egui::Window::new("编辑器 (F1)")
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.editor.tab, 0, "特效");
                ui.selectable_value(&mut app.editor.tab, 1, "动画");
            });
            ui.separator();
            match app.editor.tab {
                0 => tab_vfx(ui, app),
                _ => tab_anim(ui, app),
            }
        });
}

fn tab_vfx(ui: &mut egui::Ui, app: &mut GameApp) {
            let mut do_save = false;
            let mut do_save_weapons = false;

            // ---- 蓝图选择 / 新建 / 删除 ----
            ui.horizontal(|ui| {
                ui.label("蓝图");
                let names: Vec<String> = app.vfx.bps.keys().cloned().collect();
                ComboBox::from_id_salt("vfx_sel")
                    .selected_text(app.editor.sel.clone())
                    .show_ui(ui, |ui| {
                        for n in names {
                            ui.selectable_value(&mut app.editor.sel, n.clone(), n);
                        }
                    });
                if ui.button("新建").clicked() {
                    let mut i = 1;
                    let name = loop {
                        let n = format!("new_fx_{i}");
                        if !app.vfx.bps.contains_key(&n) {
                            break n;
                        }
                        i += 1;
                    };
                    app.vfx
                        .bps
                        .insert(name.clone(), Blueprint { shake: 0.0, hitstop: 0, emitters: vec![Emitter::default()] });
                    app.editor.sel = name;
                }
                if ui.button("删除").clicked() && app.vfx.bps.contains_key(&app.editor.sel) {
                    app.vfx.bps.remove(&app.editor.sel);
                    app.editor.sel = app.vfx.bps.keys().next().cloned().unwrap_or_default();
                }
            });

            // ---- 蓝图参数 ----
            if let Some(bp) = app.vfx.bps.get_mut(&app.editor.sel.clone()) {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("震屏");
                    ui.add(egui::DragValue::new(&mut bp.shake).speed(0.1).range(0.0..=20.0));
                    ui.label("顿帧");
                    ui.add(egui::DragValue::new(&mut bp.hitstop).range(0..=30));
                    ui.label(format!("发射器 ×{}", bp.emitters.len()));
                });
                // ---- 发射器列表 ----
                let mut del: Option<usize> = None;
                for (i, em) in bp.emitters.iter_mut().enumerate() {
                    let hdr = format!("#{i}  {}  ×{}", em.shape.kind, em.count);
                    egui::CollapsingHeader::new(hdr)
                        .default_open(i == 0)
                        .show(ui, |ui| emitter_ui(ui, em, i, &mut del));
                }
                if let Some(i) = del {
                    bp.emitters.remove(i);
                }
                if ui.button("+ 添加发射器").clicked() {
                    bp.emitters.push(Emitter {
                        offset: [0.0, 0.0],
                        shape: Default::default(),
                        count: 16,
                        dir: -1.5708,
                        spread: 1.5,
                        speed: [60.0, 160.0],
                        gravity: 260.0,
                        drag: 1.0,
                        life: [0.2, 0.5],
                        size: [1.0, 2.5],
                        color: [1.0, 0.8, 0.3],
                        color2: [0.0, 0.0, 0.0],
                        glow: true,
                    });
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("▶ 预览触发").clicked() {
                    app.editor.trigger = true;
                }
                if ui.button("💾 保存 vfx.ron").clicked() {
                    do_save = true;
                }
                if ui.button("⟳ 从文件重载").clicked() {
                    if let Ok(s) = std::fs::read_to_string(VFX_PATH) {
                        match ron::from_str::<HashMap<String, Blueprint>>(&s) {
                            Ok(bps) => app.vfx.bps = bps,
                            Err(e) => tracing::warn!("重载失败: {e}"),
                        }
                    }
                }
            });

            // ---- 武器挂点 ----
            ui.separator();
            ui.heading("武器挂点（改完保存即生效）");
            let names: Vec<String> = app.vfx.bps.keys().cloned().collect();
            for (label, val) in [
                ("挥剑", &mut app.weapons.slash),
                ("近战命中", &mut app.weapons.hit),
                ("暴击", &mut app.weapons.crit),
                ("箭命中", &mut app.weapons.arrow_hit),
                ("火球命中", &mut app.weapons.hit_spark),
                ("爆炸", &mut app.weapons.explosion),
                ("入水化汽", &mut app.weapons.fizz),
                ("死亡", &mut app.weapons.death),
            ] {
                ui.horizontal(|ui| {
                    ui.monospace(label);
                    ComboBox::from_id_salt(format!("wpn_{label}"))
                        .selected_text(val.clone())
                        .show_ui(ui, |ui| {
                            for n in &names {
                                ui.selectable_value(val, n.clone(), n);
                            }
                        });
                });
            }
            if ui.button("💾 保存 weapons.ron").clicked() {
                do_save_weapons = true;
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("主音量");
                ui.add(egui::Slider::new(&mut app.audio.volume, 0.0..=1.0));
            });
            if do_save {
                save_vfx(app);
            }
            if do_save_weapons {
                save_weapons(app);
            }
}

fn emitter_ui(ui: &mut egui::Ui, em: &mut Emitter, idx: usize, del: &mut Option<usize>) {
    ui.horizontal(|ui| {
        ui.label("形状");
        if ui.selectable_label(em.shape.kind == "point", "point").clicked() {
            em.shape.kind = "point".into();
        }
        if ui.selectable_label(em.shape.kind == "arc", "arc").clicked() {
            em.shape.kind = "arc".into();
        }
        if em.shape.kind == "arc" {
            ui.label("半径");
            ui.add(egui::DragValue::new(&mut em.shape.radius).speed(0.2));
            ui.label("角0");
            ui.add(egui::DragValue::new(&mut em.shape.a0).speed(0.02));
            ui.label("角1");
            ui.add(egui::DragValue::new(&mut em.shape.a1).speed(0.02));
        } else {
            ui.label("方向");
            ui.add(egui::DragValue::new(&mut em.dir).speed(0.02).range(-6.28..=6.28));
            ui.label("锥角");
            ui.add(egui::DragValue::new(&mut em.spread).speed(0.02).range(0.0..=3.15));
        }
        if ui.small_button("✕").clicked() {
            *del = Some(idx);
        }
    });
    ui.horizontal(|ui| {
        ui.label("偏移");
        ui.add(egui::DragValue::new(&mut em.offset[0]).speed(0.1));
        ui.add(egui::DragValue::new(&mut em.offset[1]).speed(0.1));
        ui.label("数量");
        ui.add(egui::DragValue::new(&mut em.count).range(1..=512));
        ui.label("重力");
        ui.add(egui::DragValue::new(&mut em.gravity).speed(1.0));
        ui.label("阻力");
        ui.add(egui::DragValue::new(&mut em.drag).speed(0.005).range(0.0..=1.0));
    });
    ui.horizontal(|ui| {
        ui.label("速度");
        ui.add(egui::DragValue::new(&mut em.speed[0]).speed(1.0));
        ui.add(egui::DragValue::new(&mut em.speed[1]).speed(1.0));
        ui.label("寿命");
        ui.add(egui::DragValue::new(&mut em.life[0]).speed(0.01).range(0.0..=5.0));
        ui.add(egui::DragValue::new(&mut em.life[1]).speed(0.01).range(0.0..=5.0));
        ui.label("大小");
        ui.add(egui::DragValue::new(&mut em.size[0]).speed(0.05).range(0.2..=16.0));
        ui.add(egui::DragValue::new(&mut em.size[1]).speed(0.05).range(0.2..=16.0));
    });
    ui.horizontal(|ui| {
        ui.label("色A");
        ui.color_edit_button_rgb(&mut em.color);
        ui.label("色B");
        ui.color_edit_button_rgb(&mut em.color2);
        ui.checkbox(&mut em.glow, "辉光");
    });
}

/// 动画编辑页：精灵表加载 / 帧时长 / 帧事件 / 预览 / 保存（全程不重启）
fn tab_anim(ui: &mut egui::Ui, app: &mut GameApp) {
    let names: Vec<String> = app.anims.defs.keys().cloned().collect();
    if app.editor.anim_sel.is_empty() {
        app.editor.anim_sel = names.first().cloned().unwrap_or_default();
    }
    ui.horizontal(|ui| {
        ui.label("动画");
        ComboBox::from_id_salt("anim_sel")
            .selected_text(app.editor.anim_sel.clone())
            .show_ui(ui, |ui| {
                for n in &names {
                    ui.selectable_value(&mut app.editor.anim_sel, n.clone(), n);
                }
            });
        if ui.button("新建").clicked() {
            let mut i = 1;
            let name = loop {
                let n = format!("new_anim_{i}");
                if !app.anims.defs.contains_key(&n) {
                    break n;
                }
                i += 1;
            };
            app.anims.defs.insert(
                name.clone(),
                crate::anim::AnimDef {
                    name: name.clone(),
                    sheet: String::new(),
                    frame_w: 16,
                    frame_h: 16,
                    frame_times: vec![0.12],
                    events: vec![],
                    r#loop: true,
                },
            );
            app.editor.anim_sel = name;
        }
        if ui.button("删除").clicked() && app.anims.defs.contains_key(&app.editor.anim_sel) {
            app.anims.remove(&app.editor.anim_sel);
            app.anim_preview = None;
            app.editor.anim_sel.clear();
        }
    });

    let Some(def) = app.anims.defs.get_mut(&app.editor.anim_sel.clone()) else {
        return;
    };
    ui.separator();
    egui::Grid::new("anim_meta")
        .num_columns(2)
        .spacing([10.0, 3.0])
        .show(ui, |ui| {
            ui.label("精灵表文件");
            ui.text_edit_singleline(&mut def.sheet);
            ui.end_row();
            ui.label("帧尺寸");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut def.frame_w).range(1..=256));
                ui.label("x");
                ui.add(egui::DragValue::new(&mut def.frame_h).range(1..=256));
            });
            ui.end_row();
            ui.label("循环");
            ui.checkbox(&mut def.r#loop, "");
            ui.end_row();
        });
    if ui.button("📂 加载/重切精灵表（assets/anims/ 下）").clicked() {
        app.editor.anim_load_req = true;
    }
    if let Some(e) = &app.editor.anim_err {
        ui.colored_label(egui::Color32::RED, e);
    }

    // 帧时长
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("统一帧时长");
        ui.add(
            egui::DragValue::new(&mut app.editor.anim_time)
                .speed(0.01)
                .range(0.016..=2.0)
                .suffix("s"),
        );
        if ui.button("应用到全部帧").clicked() && app.editor.anim_time >= 0.016 {
            if def.frame_times.is_empty() {
                def.frame_times.push(app.editor.anim_time);
            } else {
                for t in &mut def.frame_times {
                    *t = app.editor.anim_time;
                }
            }
        }
    });

    // 帧事件
    ui.heading("帧事件");
    let mut del_ev: Option<usize> = None;
    for (i, (f, e)) in def.events.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label("帧");
            ui.add(egui::DragValue::new(f).range(0..=255));
            ui.label("事件");
            ui.text_edit_singleline(e);
            if ui.small_button("✕").clicked() {
                del_ev = Some(i);
            }
        });
    }
    if let Some(i) = del_ev {
        def.events.remove(i);
    }
    if ui.button("+ 添加事件").clicked() {
        def.events.push((0, "hit".into()));
    }

    // 预览 / 保存
    ui.separator();
    ui.horizontal(|ui| {
        let playing = app.anim_preview.is_some();
        if ui.button(if playing { "⏹ 停止预览" } else { "▶ 预览（玩家头顶）" }).clicked() {
            app.anim_preview = if playing {
                None
            } else {
                Some(crate::anim::AnimPlayer::new(app.editor.anim_sel.clone()))
            };
        }
        if ui.button("💾 保存 animations.ron").clicked() {
            if let Err(e) = app.anims.save() {
                tracing::error!("animations.ron 保存失败: {e}");
            }
        }
    });
    ui.small("Aseprite：File → Export sprite sheet 导出横向 PNG 放入 assets/anims/；帧事件在预览播放到该帧时输出日志。");
}
