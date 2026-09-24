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
    /// 选中蓝图名
    pub sel: String,
    /// 预览触发标记（tick 中消费，因为需要相机等）
    pub trigger: bool,
    vfx_mtime: Option<std::time::SystemTime>,
    wpn_mtime: Option<std::time::SystemTime>,
    mat_mtime: Option<std::time::SystemTime>,
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
    mat_reloaded
}

/// egui 面板（App::render 中调用）
pub fn draw(app: &mut GameApp, ctx: &egui::Context) {
    if !app.editor.open {
        return;
    }
    egui::Window::new("特效编辑器 (VFX)")
        .default_width(420.0)
        .show(ctx, |ui| {
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
        });
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
