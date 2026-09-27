//! 特效编辑器 v1：egui 调参面板 + 实时预览 + vfx.ron 保存/热重载 + 武器挂点映射
//! 验收标准：做一个新特效并挂到武器上，全程不重启
use crate::vfx::{Blueprint, Emitter};
use crate::GameApp;
use egui::ComboBox;
use notify::Watcher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 运行时数据文件路径（编辑器读写；编译期嵌入仅作初始兜底）
// 资源路径统一由 crate::project 解析（工程优先，内置兜底）

const SHADERS: &[(&str, mge_render::renderer::ShaderKind)] = &[
    ("sprite.wgsl", mge_render::renderer::ShaderKind::Sprite),
    ("pixels.wgsl", mge_render::renderer::ShaderKind::Pixels),
    ("composite.wgsl", mge_render::renderer::ShaderKind::Composite),
    ("bloom.wgsl", mge_render::renderer::ShaderKind::Bloom),
];

/// 启动文件监听（notify）：监视数据表与着色器目录，任何变更经通道通知 tick
/// 立即执行热重载检查（替代纯 0.5s mtime 轮询；轮询保留作兜底）。
/// 返回 (watcher, receiver)：watcher 必须保活（drop 即停止监听）。
pub fn spawn_watcher() -> (
    Option<notify::RecommendedWatcher>,
    Option<std::sync::mpsc::Receiver<()>>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(
        move |res: Result<notify::Event, notify::Error>| {
            if res.is_ok() {
                // 只发信号不做 IO，重载仍由主线程 mtime 校验驱动
                let _ = tx.send(());
            }
        },
    ) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("文件监听不可用，退回 mtime 轮询: {e}");
            return (None, None);
        }
    };
    // 三个监视目录：引擎数据表（materials/vegetation）、游戏数据表（vfx/weapons/animations）、着色器
    let dirs = crate::project::watch_dirs();
    for d in dirs {
        if let Err(e) = watcher.watch(&d, notify::RecursiveMode::NonRecursive) {
            tracing::warn!("watch {d:?} 失败: {e}");
        }
    }
    tracing::info!("文件监听热重载已启动（notify，3 目录）");
    (Some(watcher), Some(rx))
}

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
    veg_mtime: Option<std::time::SystemTime>,
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
    // ---- 植被页 ----
    /// 选中植被序号
    pub veg_sel: usize,
    /// "重新生长"请求（tick 中消费：需要 &mut world）
    pub veg_regrow_req: bool,
    // ---- 人物页 ----
    /// 选中部件序号
    pub char_sel: usize,
    /// 画笔颜色
    pub char_color: [f32; 3],
    /// 擦除模式（左键变擦除）
    pub char_erase: bool,
    /// 待上传图集的部件（tick 中消费：需要 renderer）
    pub char_dirty: Vec<&'static str>,
    /// 画布缩放（每像素格边长 px；<=0 视为默认 26）
    pub char_zoom: f32,
    /// 撤销栈（部件序号 + 编辑前快照），笔画级
    pub char_undo: Vec<(usize, image::RgbaImage)>,
    /// 重做栈
    pub char_redo: Vec<(usize, image::RgbaImage)>,
    /// 正在进行的笔画（拖拽全程只压一次快照）
    pub char_stroke: bool,
    // ---- 物品页 ----
    /// 选中物品 id
    pub item_sel: String,
    /// 新建物品的 id 草稿
    pub item_new_id: String,
    items_mtime: Option<std::time::SystemTime>,
    /// 图标精灵图路径（外部文件，导入后逐帧入图集）
    pub icon_path: String,
    /// "导入精灵图"请求（tick 中消费：需要 renderer）
    pub icon_import_req: bool,
    /// 已切入的精灵图帧（选择器展示用；键 = 帧序号）
    pub icon_sheet: Vec<image::RgbaImage>,
    /// "选用第 N 帧作为当前物品图标"请求（tick 中消费：存 PNG + 上传图集）
    pub icon_pick: Option<usize>,
    pub item_err: Option<String>,
}

/// 读取武器映射（文件缺失/损坏时用默认）
pub fn load_weapons() -> WeaponFx {
    match std::fs::read_to_string(crate::project::path_of("data/weapons.ron")) {
        Ok(s) => ron::from_str(&s).unwrap_or_else(|e| {
            tracing::warn!("weapons.ron 解析失败，使用默认: {e}");
            WeaponFx::default()
        }),
        Err(_) => WeaponFx::default(),
    }
}

/// 读取植被定义（文件缺失/损坏时用编译期嵌入表）
pub fn load_veg() -> mge_world::veg::VegFile {
    match std::fs::read_to_string(crate::project::path_of("data/vegetation.ron")) {
        Ok(s) => mge_world::veg::VegFile::from_ron(&s).unwrap_or_else(|e| {
            tracing::warn!("vegetation.ron 解析失败，使用默认: {e}");
            mge_world::veg::VegFile::embedded()
        }),
        Err(_) => mge_world::veg::VegFile::embedded(),
    }
}

/// 保存植被定义到 vegetation.ron
pub fn save_veg(app: &mut GameApp) {
    match ron::ser::to_string_pretty(&app.veg, Default::default()) {
        Ok(s) => {
            if let Err(e) = std::fs::write(crate::project::path_of("data/vegetation.ron"), &s) {
                tracing::error!("vegetation.ron 保存失败: {e}");
            } else {
                tracing::info!("vegetation.ron 已保存（{} 种植被）", app.veg.plants.len());
                if let Ok(m) = mtime(&crate::project::path_of("data/vegetation.ron")) {
                    app.editor.veg_mtime = Some(m);
                }
            }
        }
        Err(e) => tracing::error!("vegetation.ron 序列化失败: {e}"),
    }
}

/// 保存武器映射
pub fn save_weapons(app: &mut GameApp) {
    match ron::ser::to_string_pretty(&app.weapons, Default::default()) {
        Ok(s) => {
            if let Err(e) = std::fs::write(crate::project::path_of("data/weapons.ron"), &s) {
                tracing::error!("weapons.ron 保存失败: {e}");
            } else {
                tracing::info!("weapons.ron 已保存");
                if let Ok(m) = mtime(&crate::project::path_of("data/weapons.ron")) {
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
            if let Err(e) = std::fs::write(crate::project::path_of("data/vfx.ron"), &s) {
                tracing::error!("vfx.ron 保存失败: {e}");
            } else {
                tracing::info!("vfx.ron 已保存（{} 个蓝图）", app.vfx.bps.len());
                if let Ok(m) = mtime(&crate::project::path_of("data/vfx.ron")) {
                    app.editor.vfx_mtime = Some(m);
                }
            }
        }
        Err(e) => tracing::error!("vfx.ron 序列化失败: {e}"),
    }
}

fn mtime(path: &std::path::Path) -> std::io::Result<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified())
}

/// mtime 轮询热重载（每 N tick 调一次；编辑器打开时跳过 vfx 覆盖防丢编辑）。
/// 返回 true 表示材质表被重载（调用方需重建调色板）。
pub fn reload_if_changed(app: &mut GameApp) -> bool {
    let mut mat_reloaded = false;
    // 资源路径：工程内优先，否则内置（M18）
    let mat_path = crate::project::path_of("data/materials.ron");
    let wpn_path = crate::project::path_of("data/weapons.ron");
    let vfx_path = crate::project::path_of("data/vfx.ron");
    let veg_path = crate::project::path_of("data/vegetation.ron");
    let anims_path = crate::project::path_of("data/animations.ron");
    let shader_dir = crate::project::dir_of("shaders");
    // ---- materials.ron：追加式新材质（改 id 顺序会破坏存档与既有像素）----
    if let Ok(m) = mtime(&mat_path) {
        let changed = app.editor.mat_mtime.map(|b| b != m).unwrap_or(true);
        if changed && !app.editor.open {
            match std::fs::read_to_string(&mat_path) {
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
    if let Ok(m) = mtime(&wpn_path) {
        let changed = app.editor.wpn_mtime.map(|b| b != m).unwrap_or(true);
        if changed {
            app.weapons = load_weapons();
            app.editor.wpn_mtime = Some(m);
        }
    }
    // ---- vfx.ron：面板打开时不自动覆盖 ----
    if let Ok(m) = mtime(&vfx_path) {
        let changed = app.editor.vfx_mtime.map(|b| b != m).unwrap_or(true);
        if changed && !app.editor.open {
            match std::fs::read_to_string(&vfx_path) {
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
    if let Ok(m) = mtime(&anims_path) {
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
    // ---- vegetation.ron：面板打开时跳过（防丢编辑）----
    if let Ok(m) = mtime(&veg_path) {
        let changed = app.editor.veg_mtime.map(|b| b != m).unwrap_or(true);
        if changed && !app.editor.open {
            app.veg = load_veg();
            tracing::info!("vegetation.ron 热重载完成（{} 种植被）", app.veg.plants.len());
            app.editor.veg_mtime = Some(m);
        } else if app.editor.veg_mtime.is_none() {
            app.editor.veg_mtime = Some(m);
        }
    }
    // ---- items.ron：物品页打开时跳过（防丢编辑）----
    if let Ok(m) = mtime(&crate::items::ItemDb::path()) {
        let changed = app.editor.items_mtime.map(|b| b != m).unwrap_or(true);
        let editing = app.editor.open && app.editor.tab == 4;
        if changed && !editing {
            match app.db.reload_from_disk() {
                Ok(()) => tracing::info!("items.ron 热重载完成（{} 个物品）", app.db.defs.len()),
                Err(e) => tracing::warn!("items.ron 重载失败: {e}"),
            }
            app.editor.items_mtime = Some(m);
        } else if app.editor.items_mtime.is_none() {
            app.editor.items_mtime = Some(m);
        }
    }
    // ---- WGSL 着色器热重载（改文件不重启）----
    for (i, (name, kind)) in SHADERS.iter().enumerate() {
        let path = shader_dir.join(name);
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
        .default_width(760.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.editor.tab, 0, "特效");
                ui.selectable_value(&mut app.editor.tab, 1, "动画");
                ui.selectable_value(&mut app.editor.tab, 2, "植被");
                ui.selectable_value(&mut app.editor.tab, 3, "人物");
                ui.selectable_value(&mut app.editor.tab, 4, "物品");
            });
            ui.separator();
            match app.editor.tab {
                0 => tab_vfx(ui, app),
                1 => tab_anim(ui, app),
                2 => tab_veg(ui, app),
                3 => tab_char(ui, app),
                _ => tab_items(ui, app),
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
                    if let Ok(s) =
                        std::fs::read_to_string(crate::project::path_of("data/vfx.ron"))
                    {
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

/// 植被编辑页：增删改植被定义 / 重新生长 / 保存（全程不重启）
fn tab_veg(ui: &mut egui::Ui, app: &mut GameApp) {
    let mat_names = app.world.mats.names();
    let n_plants = app.veg.plants.len();

    // ---- 选择 / 新建 / 删除 ----
    ui.horizontal(|ui| {
        ui.label("植被");
        let cur = app
            .veg
            .plants
            .get(app.editor.veg_sel)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "（无）".into());
        ComboBox::from_id_salt("veg_sel")
            .selected_text(cur)
            .show_ui(ui, |ui| {
                for (i, p) in app.veg.plants.iter().enumerate() {
                    ui.selectable_value(&mut app.editor.veg_sel, i, p.name.clone());
                }
            });
        if ui.button("新建").clicked() {
            app.veg.plants.push(mge_world::veg::PlantDef {
                name: format!("新植被_{}", n_plants + 1),
                biomes: vec!["forest".into()],
                ground: vec!["grass".into()],
                density: 0.05,
                gap: 3,
                kind: mge_world::veg::VegKind::Grass,
                body: "tall_grass".into(),
                stem: None,
                h: (3, 7),
                w: (2, 4),
            });
            app.editor.veg_sel = app.veg.plants.len() - 1;
        }
        if ui.button("删除").clicked() && !app.veg.plants.is_empty() {
            app.veg.plants.remove(app.editor.veg_sel.min(app.veg.plants.len() - 1));
            app.editor.veg_sel = app.editor.veg_sel.min(app.veg.plants.len().saturating_sub(1));
        }
    });

    let sel = app.editor.veg_sel;
    let Some(def) = app.veg.plants.get_mut(sel) else {
        ui.small("尚无植被，点击\"新建\"创建。");
        return;
    };

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("名称");
        ui.text_edit_singleline(&mut def.name);
    });
    // 生物群系
    ui.horizontal(|ui| {
        ui.label("生物群系");
        for (label, key) in [("森林", "forest"), ("雪原", "snow"), ("沙漠", "desert")] {
            let mut on = def.biomes.iter().any(|b| b == key);
            if ui.checkbox(&mut on, label).changed() {
                if on {
                    if !def.biomes.iter().any(|b| b == key) {
                        def.biomes.push(key.into());
                    }
                } else {
                    def.biomes.retain(|b| b != key);
                }
            }
        }
        ui.label("空 = 全部");
    });
    // 地表材质（可多选）
    ui.horizontal(|ui| {
        ui.label("地表材质");
        let mut del: Option<usize> = None;
        for (i, g) in def.ground.iter().enumerate() {
            ui.monospace(g.as_str());
            if ui.small_button("✕").clicked() {
                del = Some(i);
            }
        }
        if let Some(i) = del {
            def.ground.remove(i);
        }
    });
    ui.horizontal(|ui| {
        ui.label("添加地表");
        ComboBox::from_id_salt("veg_ground_add")
            .selected_text("选择材质…")
            .show_ui(ui, |ui| {
                for n in &mat_names {
                    if ui.selectable_label(!def.ground.contains(n), n).clicked() && !def.ground.contains(n) {
                        def.ground.push(n.clone());
                    }
                }
            });
        ui.label("空 = 任意");
    });
    // 形态
    ui.horizontal(|ui| {
        ui.label("形态");
        for (label, kind) in [
            ("草", mge_world::veg::VegKind::Grass),
            ("花", mge_world::veg::VegKind::Flower),
            ("灌木", mge_world::veg::VegKind::Bush),
            ("蘑菇", mge_world::veg::VegKind::Mushroom),
            ("仙人掌", mge_world::veg::VegKind::Cactus),
        ] {
            if ui.selectable_label(def.kind == kind, label).clicked() {
                def.kind = kind;
            }
        }
    });
    // 材质
    ui.horizontal(|ui| {
        ui.label("主体材质");
        ComboBox::from_id_salt("veg_body")
            .selected_text(def.body.clone())
            .show_ui(ui, |ui| {
                for n in &mat_names {
                    ui.selectable_value(&mut def.body, n.clone(), n);
                }
            });
        ui.label("茎干材质");
        let stem_txt = def.stem.clone().unwrap_or_else(|| "（无）".into());
        ComboBox::from_id_salt("veg_stem")
            .selected_text(stem_txt)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut def.stem, None, "（无）");
                for n in &mat_names {
                    if ui
                        .selectable_label(def.stem.as_deref() == Some(n.as_str()), n)
                        .clicked()
                    {
                        def.stem = Some(n.clone());
                    }
                }
            });
    });
    // 参数
    ui.horizontal(|ui| {
        ui.label("密度");
        ui.add(egui::Slider::new(&mut def.density, 0.0..=0.5));
    });
    ui.horizontal(|ui| {
        ui.label("间距");
        ui.add(egui::DragValue::new(&mut def.gap).range(0..=128).suffix("px"));
        ui.label("高度");
        ui.add(egui::DragValue::new(&mut def.h.0).range(1..=64));
        ui.label("~");
        ui.add(egui::DragValue::new(&mut def.h.1).range(1..=64));
        ui.label("宽度");
        ui.add(egui::DragValue::new(&mut def.w.0).range(1..=32));
        ui.label("~");
        ui.add(egui::DragValue::new(&mut def.w.1).range(1..=32));
    });

    // ---- 操作 ----
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("🌱 重新生长").clicked() {
            app.editor.veg_regrow_req = true;
        }
        if ui.button("💾 保存 vegetation.ron").clicked() {
            save_veg(app);
        }
    });
    ui.small("植被为背景层（与树同层，不碰撞）。改完点\"重新生长\"立即在世界地表生效；新颜色先在 materials.ron 里加材质。");
}

/// 人物形象编辑页：逐像素绘制部件贴图（实时生效，保存持久化）
fn tab_char(ui: &mut egui::Ui, app: &mut GameApp) {
    // 部件选择
    ui.horizontal(|ui| {
        for (i, def) in crate::character::PARTS.iter().enumerate() {
            ui.selectable_value(&mut app.editor.char_sel, i, def.label);
        }
    });
    let def = &crate::character::PARTS[app.editor.char_sel];
    ui.label(format!(
        "{}（{}×{} 像素，左键涂色 / 右键擦除 / 中键取色）",
        def.label, def.w, def.h
    ));
    ui.separator();

    // 画笔颜色 + 快捷色板
    ui.horizontal(|ui| {
        ui.label("画笔");
        ui.color_edit_button_rgb(&mut app.editor.char_color);
        for (label, c) in [
            ("肤", [0.88, 0.68, 0.55]),
            ("衣", [0.24, 0.47, 0.78]),
            ("裤", [0.26, 0.26, 0.34]),
            ("发", [0.35, 0.24, 0.12]),
            ("白", [0.95, 0.95, 0.95]),
            ("黑", [0.1, 0.1, 0.12]),
        ] {
            let btn = egui::Button::new(label).fill(egui::Color32::from_rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            ));
            if ui.add(btn).clicked() {
                app.editor.char_color = c;
            }
        }
        if ui.button("擦除").clicked() {
            app.editor.char_color = [0.0, 0.0, 0.0];
            app.editor.char_erase = true;
        }
    });

    // 画布缩放 + 撤销/重做
    let mut zoom = if app.editor.char_zoom <= 0.0 { 26.0 } else { app.editor.char_zoom };
    ui.horizontal(|ui| {
        let zr = ui.add(egui::Slider::new(&mut zoom, 8.0..=48.0).text("画布缩放"));
        if zr.changed() {
            app.editor.char_zoom = zoom;
        }
        if ui.button("↶ 撤销 (Ctrl+Z)").clicked() {
            char_undo(app);
        }
        if ui.button("↷ 重做 (Ctrl+Y)").clicked() {
            char_redo(app);
        }
    });
    if ui.ctx().input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
        char_undo(app);
    }
    if ui.ctx().input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Y)) {
        char_redo(app);
    }

    // 逐像素画布（可变借用作用域内完成绘制与重置）
    let cell = zoom;
    let mut interacted = false;
    {
        let Some(pt) = app.skin.get_mut(def.key) else { return; };
        egui::Grid::new("char_canvas")
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                for y in 0..def.h {
                    for x in 0..def.w {
                        let px = pt.img.get_pixel(x, y).0;
                        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
                        let shown = if a == 0 {
                            egui::Color32::from_rgb(44, 44, 52) // 透明格底色
                        } else {
                            egui::Color32::from_rgb(r, g, b)
                        };
                        let resp = ui
                            .allocate_response(egui::vec2(cell, cell), egui::Sense::click_and_drag());
                        ui.painter().rect_filled(resp.rect, 3.0, shown);
                        ui.painter().rect_filled(resp.rect.shrink(1.0), 2.0, shown);
                        let lmb = resp.dragged_by(egui::PointerButton::Primary)
                            || resp.clicked();
                        // 擦除：擦除模式下左键，或任意模式右键（必须伴随指针交互，防止开启擦除模式瞬间清空整图）
                        let erase = (app.editor.char_erase && lmb)
                            || resp.dragged_by(egui::PointerButton::Secondary)
                            || resp.secondary_clicked();
                        let paint = !erase && lmb;
                        // 中键取色
                        if resp.clicked_by(egui::PointerButton::Middle) && a != 0 {
                            app.editor.char_color =
                                [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
                            app.editor.char_erase = false;
                        }
                        // 笔画开始：压入一次撤销快照（拖拽全程只压一次）
                        if (paint || erase) && !app.editor.char_stroke {
                            app.editor
                                .char_undo
                                .push((app.editor.char_sel, pt.img.clone()));
                            if app.editor.char_undo.len() > 40 {
                                app.editor.char_undo.remove(0);
                            }
                            app.editor.char_redo.clear();
                            app.editor.char_stroke = true;
                        }
                        if paint || erase {
                            interacted = true;
                        }
                        if paint {
                            let c = app.editor.char_color;
                            pt.img.put_pixel(
                                x,
                                y,
                                image::Rgba([
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                    255,
                                ]),
                            );
                            app.editor.char_dirty.push(def.key);
                        } else if erase && a != 0 {
                            pt.img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
                            app.editor.char_dirty.push(def.key);
                        }
                    }
                    ui.end_row();
                }
            });
        if ui.button("↺ 重置此部件").clicked() {
            // 重置可撤销：先压快照
            let snap = pt.img.clone();
            crate::character::reset_part(pt);
            app.editor.char_undo.push((app.editor.char_sel, snap));
            if app.editor.char_undo.len() > 40 {
                app.editor.char_undo.remove(0);
            }
            app.editor.char_redo.clear();
            app.editor.char_dirty.push(def.key);
        }
        // 无指针交互的帧视为笔画结束
        if !interacted {
            app.editor.char_stroke = false;
        }
    }

    // 操作
    ui.horizontal(|ui| {
        if ui.button("💾 保存形象").clicked() {
            match crate::character::save(&app.skin) {
                Ok(_) => tracing::info!("人物形象已保存到 assets/character/"),
                Err(e) => tracing::error!("人物形象保存失败: {e}"),
            }
        }
        if ui.button("擦除模式").clicked() {
            app.editor.char_erase = !app.editor.char_erase;
        }
        if app.editor.char_erase {
            ui.colored_label(egui::Color32::YELLOW, "擦除中");
        }
    });
    ui.small("形象实时生效（程序化动画保留）；保存后下次启动自动加载 assets/character/*.png。");
}

/// 人物编辑：撤销上一次笔画/重置（Ctrl+Z）
fn char_undo(app: &mut GameApp) {
    let Some((part, snap)) = app.editor.char_undo.pop() else { return };
    let key = crate::character::PARTS[part].key;
    if let Some(pt) = app.skin.get_mut(key) {
        let cur = pt.img.clone();
        pt.img = snap;
        app.editor.char_redo.push((part, cur));
        app.editor.char_stroke = false;
    }
    app.editor.char_dirty.push(key);
}

/// 人物编辑：重做（Ctrl+Y）
fn char_redo(app: &mut GameApp) {
    let Some((part, snap)) = app.editor.char_redo.pop() else { return };
    let key = crate::character::PARTS[part].key;
    if let Some(pt) = app.skin.get_mut(key) {
        let cur = pt.img.clone();
        pt.img = snap;
        app.editor.char_undo.push((part, cur));
        app.editor.char_stroke = false;
    }
    app.editor.char_dirty.push(key);
}

/// 物品页：左侧物品列表 + 保存；右侧按类型展示不同配置参数（选中即显示）
fn tab_items(ui: &mut egui::Ui, app: &mut GameApp) {
    use crate::items::Slot;
    if app.editor.icon_path.is_empty() {
        // 预填上次的精灵图路径（首次使用可直接导入）
        app.editor.icon_path =
            r"C:\Users\Huang\Downloads\Free - Raven Fantasy Icons\Full Spritesheet\64x64.png"
                .to_string();
    }
    ui.columns(2, |cols| {
        // ============ 左列：仅物品选择 ============
        {
            let ui = &mut cols[0];
            ui.heading("物品列表");
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("item_list_scroll")
                .max_height(430.0)
                .show(ui, |ui| {
                    let mut ids: Vec<String> = app.db.defs.keys().cloned().collect();
                    ids.sort();
                    for id in ids {
                        let (name, cat) = {
                            let d = &app.db.defs[&id];
                            (d.name.clone(), d.category())
                        };
                        if ui
                            .selectable_label(app.editor.item_sel == id, format!("{name} · {cat}"))
                            .on_hover_text(format!("id: {id}"))
                            .clicked()
                        {
                            app.editor.item_sel = id;
                        }
                    }
                });
            ui.small("← 选择物品，在右侧编辑\n（悬停列表项可看 id）");
        }
        // ============ 右列：新建 + 配置面板 + 保存 ============
        {
            let ui = &mut cols[1];
            ui.heading("编辑");
            ui.separator();

            // ---- 新建 ----
            ui.horizontal(|ui| {
                let id_hint = ui.add(
                    egui::TextEdit::singleline(&mut app.editor.item_new_id)
                        .hint_text("新物品 id（英文）")
                        .desired_width(140.0),
                );
                let id = app.editor.item_new_id.trim().to_string();
                if ui
                    .add_enabled(!id.is_empty(), egui::Button::new("＋ 新建"))
                    .clicked()
                {
                    if app.db.defs.contains_key(&id) {
                        app.editor.item_err = Some(format!("id「{id}」已存在"));
                    } else {
                        app.db.defs.insert(
                            id.clone(),
                            crate::items::ItemDef {
                                id: id.clone(),
                                name: id.clone(),
                                slot: Slot::Weapon,
                                stack: 1,
                                icon: None,
                                dmg: 5.0,
                                armor: 0.0,
                                speed: 1.0,
                                hp: 0.0,
                                lvl: 1,
                                value: 5,
                                crit: 0.0,
                                crit_dmg: 0.0,
                            },
                        );
                        app.editor.item_sel = id;
                        app.editor.item_new_id.clear();
                    }
                }
                id_hint.request_focus();
            });
            ui.separator();
            let sel = app.editor.item_sel.clone();
            let Some(def) = app.db.defs.get_mut(&sel) else {
                ui.weak("← 从左侧选择物品，或在上方输入 id 新建");
                return;
            };

            // ---- 类型 ----
            let cat = def.category();
            ui.horizontal(|ui| {
                ui.label("类型");
                ComboBox::from_id_salt("item_cat")
                    .selected_text(cat)
                    .show_ui(ui, |ui| {
                        for c in ["装备", "消耗品", "材料"] {
                            if ui.selectable_label(cat == c, c).clicked() {
                                match c {
                                    "装备" => def.stack = 1,
                                    "消耗品" => {
                                        def.stack = 99;
                                        def.hp = def.hp.max(30.0);
                                        def.dmg = 0.0;
                                        def.armor = 0.0;
                                    }
                                    _ => {
                                        def.stack = 99;
                                        def.hp = 0.0;
                                        def.dmg = 0.0;
                                        def.armor = 0.0;
                                        def.speed = 1.0;
                                    }
                                }
                            }
                        }
                    });
                // 装备才有装备位
                ui.add_enabled_ui(def.stack == 1, |ui| {
                    ui.label("装备位");
                    ComboBox::from_id_salt("item_slot")
                        .selected_text(def.slot.name())
                        .show_ui(ui, |ui| {
                            let slots = [Slot::Weapon, Slot::Head, Slot::Chest, Slot::Legs, Slot::Trinket];
                            for s in slots {
                                if ui.selectable_label(def.slot == s, s.name()).clicked() {
                                    def.slot = s;
                                }
                            }
                        });
                });
            });
            ui.add_space(4.0);

            // ---- 基础信息 ----
            ui.horizontal(|ui| {
                ui.label("名称");
                ui.text_edit_singleline(&mut def.name);
            });
            ui.horizontal(|ui| {
                ui.label("价值");
                ui.add(egui::DragValue::new(&mut def.value).range(0..=99999));
                ui.label("等级");
                ui.add(egui::DragValue::new(&mut def.lvl).range(1..=50));
            });

            // ---- 图标 ----
            ui.horizontal(|ui| {
                ui.label("图标");
                match def.icon.clone() {
                    Some(k) => match app.icons.egui_image(ui.ctx(), &k, 40.0) {
                        Some(img) => {
                            ui.add(img);
                        }
                        None => {
                            ui.colored_label(egui::Color32::RED, "缺失");
                        }
                    },
                    None => {
                        ui.weak("未设置");
                    }
                }
                ui.label("精灵图路径（64×64/帧）");
                ui.add(
                    egui::TextEdit::singleline(&mut app.editor.icon_path).desired_width(220.0),
                );
                if ui.button("📂 导入").clicked() {
                    app.editor.icon_import_req = true;
                }
            });
            if !app.editor.icon_sheet.is_empty() {
                ui.small(format!(
                    "已导入 {} 帧：点击选用为「{}」的图标",
                    app.editor.icon_sheet.len(),
                    def.name
                ));
                egui::ScrollArea::vertical()
                    .id_salt("icon_pick_scroll")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        // 当前图标若来自本精灵图 → 解析帧序号用于金框标识
                        let sheet_stem = std::path::Path::new(app.editor.icon_path.trim())
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("icons")
                            .to_string();
                        let cur_idx = def.icon.as_ref().and_then(|k| {
                            k.strip_prefix(&format!("{sheet_stem}_"))
                                .and_then(|s| s.parse::<usize>().ok())
                        });
                        egui::Grid::new("icon_pick_grid")
                            .spacing([2.0, 2.0])
                            .show(ui, |ui| {
                                let n = app.editor.icon_sheet.len();
                                for i in 0..n {
                                    let key = crate::icons::IconBank::sheet_key(i);
                                    let sel = cur_idx == Some(i);
                                    // 金框 = 当前物品已选用的图标
                                    let stroke = if sel {
                                        egui::Stroke::new(2.0_f32, egui::Color32::GOLD)
                                    } else {
                                        egui::Stroke::NONE
                                    };
                                    let fr = egui::Frame::NONE
                                        .stroke(stroke)
                                        .inner_margin(1.0)
                                        .show(ui, |ui| {
                                            if let Some(img) =
                                                app.icons.egui_image(ui.ctx(), &key, 32.0)
                                            {
                                                ui.add(img);
                                            } else {
                                                ui.weak("?");
                                            }
                                        });
                                    // 独立 id 注册点击区（Frame 复合响应不带 click，直接依赖它点击永远不触发）
                                    let resp = ui.interact(
                                        fr.response.rect,
                                        egui::Id::new("icon_pick").with(i),
                                        egui::Sense::click(),
                                    );
                                    if resp.clicked() {
                                        app.editor.icon_pick = Some(i);
                                    }
                                    if resp.hovered() && !sel {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if (i + 1) % 10 == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                ui.small("金框 = 当前选中；点击选用");
            }
            ui.add_space(4.0);

            // ---- 类型相关属性 ----
            if def.stack == 1 {
                ui.separator();
                ui.label(match def.slot {
                    Slot::Weapon => "武器属性",
                    _ => "护甲/饰品属性",
                });
                egui::Grid::new("item_stats")
                    .num_columns(4)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        if def.slot == Slot::Weapon {
                            ui.label("伤害");
                            ui.add(egui::DragValue::new(&mut def.dmg).range(0.0..=999.0).speed(0.5));
                            ui.label("攻速倍率");
                            ui.add(egui::DragValue::new(&mut def.speed).range(0.3..=3.0).speed(0.05));
                            ui.end_row();
                            ui.label("暴击率%");
                            ui.add(egui::DragValue::new(&mut def.crit).range(0.0..=100.0).speed(0.5));
                            ui.label("暴击伤害%");
                            ui.add(egui::DragValue::new(&mut def.crit_dmg).range(0.0..=500.0).speed(1.0));
                            ui.end_row();
                        } else {
                            ui.label("护甲");
                            ui.add(egui::DragValue::new(&mut def.armor).range(0.0..=999.0).speed(0.5));
                            ui.end_row();
                        }
                    });
            } else {
                ui.separator();
                if def.hp > 0.0 {
                    ui.label("恢复效果");
                    ui.add(egui::DragValue::new(&mut def.hp).range(0.0..=999.0).speed(1.0));
                    ui.small("背包内点击/右键使用，恢复生命");
                } else {
                    ui.weak("合成材料：用于配方（合成区），无使用效果");
                }
                ui.horizontal(|ui| {
                    ui.label("堆叠上限");
                    ui.add(egui::DragValue::new(&mut def.stack).range(2..=999));
                });
            }

            // ---- 保存 / 错误提示（右列底部）----
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("💾 保存 items.ron").clicked() {
                    match app.db.save() {
                        Ok(()) => {
                            app.editor.item_err = None;
                            if let Ok(m) = std::fs::metadata(crate::items::ItemDb::path())
                                .and_then(|m| m.modified().map_err(|e| e.into()))
                            {
                                app.editor.items_mtime = Some(m);
                            }
                        }
                        Err(e) => app.editor.item_err = Some(format!("保存失败: {e}")),
                    }
                }
                if let Some(e) = &app.editor.item_err {
                    ui.colored_label(egui::Color32::RED, e);
                } else {
                    ui.small("保存后经热重载进游戏（背包/掉落/合成即时生效）");
                }
            });
        }
    });
}
