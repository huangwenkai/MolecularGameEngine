//! 引擎 IDE（F4/F1）：**插件化工作台**。
//!
//! 本文件只是 UI 壳：顶部菜单栏（由插件列表驱动）+ 左/中/右三栏分发。
//! 每个功能是一个插件（见 `plugins.rs` 的 `IdePlugin`），实现自己的左/中/右面板；
//! 改 UI 展示只动插件实现，游戏功能（编辑器数据服务、热重载等在 main.rs）不受影响。
//!
//! - 工程插件：左=工程/素材树/场景树，中=视口/文件编辑，右=检查器
//! - 编辑器插件（特效/动画/植被/人物/物品）：左=资源列表，中=编辑器页（不透明）
use crate::project;
use crate::GameApp;
use egui::{Color32, Margin, RichText, Stroke};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// IDE 配置持久化路径
const CFG_PATH: &str = "saves/ide_config.ron";
/// 默认字号（px）
pub const DEFAULT_FONT: f32 = 13.0;

/// IDE 配置（持久化到 saves/ide_config.ron）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeConfig {
    /// 界面字号（px，默认 13）
    #[serde(default = "default_font")]
    pub font_size: f32,
}

fn default_font() -> f32 {
    DEFAULT_FONT
}

impl Default for IdeConfig {
    fn default() -> Self {
        Self { font_size: DEFAULT_FONT }
    }
}

impl IdeConfig {
    pub fn load() -> Self {
        match std::fs::read_to_string(CFG_PATH) {
            Ok(s) => ron::from_str::<Self>(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all("saves");
        if let Ok(txt) = ron::ser::to_string_pretty(self, Default::default()) {
            let tmp = format!("{CFG_PATH}.tmp");
            if std::fs::write(&tmp, txt).is_ok() {
                let _ = std::fs::rename(&tmp, CFG_PATH);
            }
        }
    }
}

/// 把当前 Ui（及其子 Ui）的全部文本样式字号统一设为 size
pub(crate) fn set_font(ui: &mut egui::Ui, size: f32) {
    let s = ui.style_mut();
    for (_kind, fid) in s.text_styles.iter_mut() {
        fid.size = size;
    }
}

// ---------------------------------------------------------------------------
// 主题色（IDE 统一配色）
// ---------------------------------------------------------------------------
pub(crate) const BG_PANEL: Color32 = Color32::from_rgb(26, 27, 32);
pub(crate) const BG_SECTION: Color32 = Color32::from_rgb(32, 33, 40);
pub(crate) const BG_BAR: Color32 = Color32::from_rgb(22, 23, 28);
pub(crate) const LINE: Color32 = Color32::from_rgb(52, 54, 66);
pub(crate) const LINE_W: f32 = 1.0;
pub(crate) const ACCENT: Color32 = Color32::from_rgb(96, 165, 250);
pub(crate) const OK: Color32 = Color32::from_rgb(134, 220, 160);
pub(crate) const WARN: Color32 = Color32::from_rgb(240, 200, 90);
pub(crate) const ERR: Color32 = Color32::from_rgb(232, 118, 100);
pub(crate) const DIM: Color32 = Color32::from_rgb(150, 154, 168);

/// 场景树中的可选实体
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntSel {
    Player,
    Monster(usize),
    Npc(usize),
    Dummy(hecs::Entity),
}

pub struct Ide {
    pub open: bool,
    /// 顶部菜单栏激活功能 = 插件下标（顺序见 plugins::all_plugins）
    pub func: usize,
    /// 中栏页：0 视口 / 1 文件（仅工程插件内使用）
    pub tab: usize,
    /// 视口运行开关
    pub run: bool,
    // ---- 文件编辑 ----
    pub sel: Option<PathBuf>,
    pub text: String,
    pub text_of: Option<PathBuf>,
    pub dirty: bool,
    pub err: Option<String>,
    pub new_file: String,
    pub confirm_del: bool,
    // ---- 场景树 ----
    pub ent: Option<EntSel>,
    // ---- 精灵表导入向导 ----
    pub imp_src: String,
    pub imp_name: String,
    pub imp_fw: u32,
    pub imp_fh: u32,
    pub imp_msg: Option<String>,
    // ---- 工程导出 ----
    pub export_msg: Option<String>,
    // ---- IDE 配置 ----
    pub font_size: f32,
    pub cfg_loaded: bool,
    // ---- 各区块展开状态 ----
    pub sec_proj: bool,
    pub sec_assets: bool,
    pub sec_scene: bool,
    pub sec_ent: bool,
    pub sec_file: bool,
    pub sec_tools: bool,
    pub sec_cfg: bool,
}

impl Default for Ide {
    fn default() -> Self {
        Self {
            open: false,
            func: 0,
            tab: 0,
            run: true,
            sel: None,
            text: String::new(),
            text_of: None,
            dirty: false,
            err: None,
            new_file: String::new(),
            confirm_del: false,
            ent: None,
            imp_src: String::new(),
            imp_name: String::new(),
            imp_fw: 16,
            imp_fh: 16,
            imp_msg: None,
            export_msg: None,
            font_size: DEFAULT_FONT,
            cfg_loaded: false,
            sec_proj: true,
            sec_assets: true,
            sec_scene: true,
            sec_ent: true,
            sec_file: true,
            sec_tools: false,
            sec_cfg: true,
        }
    }
}

pub(crate) fn is_text(p: &PathBuf) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("ron") | Some("wgsl") | Some("txt") | Some("md")
    )
}

/// 可折叠区块：直角卡片 + 三角图标 + 标题，展开时展示内容
pub(crate) fn section(
    ui: &mut egui::Ui,
    title: &str,
    open: &mut bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    // 直角、无外边距缝隙：模块之间连成一片（仅保留内部内边距）
    egui::Frame::NONE
        .fill(BG_SECTION)
        .stroke(Stroke::new(LINE_W, LINE))
        .inner_margin(Margin::symmetric(6, 5))
        .show(ui, |ui| {
            // 区块内控件统一包一层 ID 作用域，避免 ScrollArea/按钮等 ID 冲突
            ui.push_id(title, |ui| {
                ui.horizontal(|ui| {
                    let icon = if *open { "▼" } else { "▶" };
                    let label = RichText::new(format!("{icon} {title}"))
                        .color(if *open { ACCENT } else { DIM })
                        .strong();
                    let resp = ui.add(
                        egui::Button::new(label)
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE)
                            .min_size(egui::vec2(ui.available_width(), 20.0)),
                    );
                    if resp.clicked() {
                        *open = !*open;
                    }
                });
                if *open {
                    ui.separator();
                    // 模块内部保留正常行距（模块之间由外层置 0）
                    ui.spacing_mut().item_spacing.y = 4.0;
                    body(ui);
                }
            });
        });
}

/// 属性行：左键名（弱化）右键值（等宽）
pub(crate) fn prop(ui: &mut egui::Ui, k: &str, v: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.weak(k);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.monospace(v.into());
        });
    });
}

/// 分组标题（树内小节）
pub(crate) fn group(ui: &mut egui::Ui, name: &str, count: usize) {
    ui.horizontal(|ui| {
        ui.weak(name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(DIM, RichText::new(format!("{count}")).small());
        });
    });
}

/// IDE 主入口：菜单栏 + 三栏分发（全部交给激活插件）
pub fn draw(app: &mut GameApp, ctx: &egui::Context) {
    if !app.ide.open {
        return;
    }
    let plugins = crate::plugins::all_plugins();
    let idx = app.ide.func.min(plugins.len() - 1);
    let proj = app.project.current.as_ref().map(|p| p.name.clone());
    let title = match &proj {
        Some(n) => format!("{} — 工程「{n}」", plugins[idx].title()),
        None => format!("{} — 内置资源", plugins[idx].title()),
    };

    // ---------------- 顶部传统菜单栏（插件驱动）----------------
    egui::TopBottomPanel::top("ide_menubar")
        .frame(
            egui::Frame::NONE
                .fill(BG_BAR)
                .stroke(Stroke::new(LINE_W, LINE))
                .inner_margin(Margin::symmetric(6, 4)),
        )
        .show(ctx, |ui| {
            set_font(ui, app.ide.font_size);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                for (i, p) in plugins.iter().enumerate() {
                    let active = i == idx;
                    if ui
                        .button(RichText::new(p.label()).color(if active { ACCENT } else { DIM }))
                        .clicked()
                    {
                        app.ide.func = i;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(proj.clone().unwrap_or_else(|| "内置资源".into()));
                });
            });
        });

    // ---------------- 激活插件（每帧回调：状态同步等）----------------
    let active = &*plugins[idx];
    active.on_active_frame(app);

    // ---------------- 左栏 ----------------
    egui::SidePanel::left("ide_left")
        .resizable(true)
        .default_width(250.0)
        .frame(
            egui::Frame::NONE
                .fill(BG_PANEL)
                .stroke(Stroke::new(LINE_W, LINE))
                .inner_margin(Margin::ZERO),
        )
        .show(ctx, |ui| {
            set_font(ui, app.ide.font_size);
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::NONE
                .fill(BG_BAR)
                .stroke(Stroke::new(LINE_W, LINE))
                .inner_margin(Margin::symmetric(6, 5))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new(&title).strong().color(ACCENT));
                });
            egui::ScrollArea::vertical()
                .id_salt("ide_left_scroll")
                .show(ui, |ui| {
                    active.left_panel(ui, app);
                });
        });

    // ---------------- 右栏（插件可选）----------------
    if active.has_right() {
        egui::SidePanel::right("ide_right")
            .resizable(true)
            .default_width(260.0)
            .frame(
                egui::Frame::NONE
                    .fill(BG_PANEL)
                    .stroke(Stroke::new(LINE_W, LINE))
                    .inner_margin(Margin::ZERO),
            )
            .show(ctx, |ui| {
                set_font(ui, app.ide.font_size);
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::Frame::NONE
                    .fill(BG_BAR)
                    .stroke(Stroke::new(LINE_W, LINE))
                    .inner_margin(Margin::symmetric(6, 5))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new("检查器 Inspector").strong().color(ACCENT));
                    });
                egui::ScrollArea::vertical()
                    .id_salt("ide_right_scroll")
                    .show(ui, |ui| {
                        active.right_panel(ui, app);
                    });
            });
    }

    // ---------------- 中栏 ----------------
    let opaque = active.wants_opaque(app);
    egui::CentralPanel::default()
        .frame(if opaque {
            // 不透明：编辑器/文件模式（避免透出游戏画面）
            egui::Frame::NONE.fill(BG_PANEL)
        } else {
            // 透明：直接透出实时游戏画面
            egui::Frame::NONE.fill(Color32::TRANSPARENT)
        })
        .show(ctx, |ui| {
            set_font(ui, app.ide.font_size);
            ui.spacing_mut().item_spacing.y = 0.0;
            active.central_panel(ui, app);
        });
}

// ---------------------------------------------------------------------------
// 以下为插件使用的渲染实现（由 plugins.rs 的插件调用）
// ---------------------------------------------------------------------------

pub(crate) fn draw_entity_props(ui: &mut egui::Ui, app: &mut GameApp, ent: EntSel) {
    match ent {
        EntSel::Player => {
            let p = &app.player;
            prop(ui, "类型", "玩家");
            prop(ui, "位置", format!("({:.1}, {:.1})", p.pos.x, p.pos.y));
            prop(ui, "速度", format!("({:.1}, {:.1})", p.vel.x, p.vel.y));
            prop(ui, "生命", format!("{:.0} / {:.0}", p.hp, p.max_hp));
            prop(ui, "朝向", format!("{:.0}", p.facing));
            prop(ui, "落地", p.on_ground.to_string());
            prop(ui, "等级", app.inv.level.to_string());
            prop(ui, "经验", app.inv.xp.to_string());
            prop(ui, "工具", app.tool.tool.name());
            prop(ui, "技能点", app.skills.pts.to_string());
        }
        EntSel::Monster(i) => {
            let Some(m) = app.monsters.list.get(i) else {
                app.ide.ent = None;
                return;
            };
            prop(ui, "类型", m.kind.name());
            prop(ui, "位置", format!("({:.1}, {:.1})", m.pos.x, m.pos.y));
            prop(ui, "生命", format!("{:.0} / {:.0}", m.hp, m.max_hp));
            prop(ui, "状态", m.state.name());
            prop(ui, "速度", format!("{:.0}", m.speed));
            prop(ui, "伤害", format!("{:.0}", m.dmg));
            prop(ui, "阶段", m.phase.to_string());
            prop(ui, "路径点", m.path.len().to_string());
        }
        EntSel::Npc(i) => {
            let Some(n) = app.npcs.list.get(i) else {
                app.ide.ent = None;
                return;
            };
            prop(ui, "类型", format!("居民 #{}", i));
            prop(ui, "位置", format!("({:.1}, {:.1})", n.pos.x, n.pos.y));
            prop(ui, "状态", n.state.name());
            prop(ui, "饥饿", format!("{:.0}", n.hunger));
            prop(ui, "口渴", format!("{:.0}", n.thirst));
            prop(ui, "疲劳", format!("{:.0}", n.fatigue));
            prop(ui, "家", format!("({:.0}, {:.0})", n.home.x, n.home.y));
        }
        EntSel::Dummy(e) => {
            prop(ui, "类型", "训练假人");
            if let Ok(tr) = app.ecs.get::<&crate::entities::Transform>(e) {
                let pos = tr.pos;
                drop(tr);
                prop(ui, "位置", format!("({:.1}, {:.1})", pos.x, pos.y));
            }
            if let Ok(d) = app.ecs.get::<&crate::entities::Dummy>(e) {
                prop(ui, "生命", format!("{:.0}", d.hp));
                prop(ui, "重生倒计时", d.respawn.to_string());
            }
        }
    }
}

pub(crate) fn draw_file_props(ui: &mut egui::Ui, app: &mut GameApp) {
    let Some(sel) = app.ide.sel.clone() else {
        ui.weak("未选中文件（左侧素材树点选）");
        return;
    };
    let name = sel.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
    prop(ui, "名称", name);
    prop(ui, "路径", sel.display().to_string());
    if let Ok(md) = std::fs::metadata(&sel) {
        prop(ui, "大小", format!("{} 字节", md.len()));
    }
    if is_text(&sel) {
        prop(ui, "行数", app.ide.text.lines().count().to_string());
    } else if sel.extension().and_then(|e| e.to_str()) == Some("png") {
        if let Ok(img) = image::open(&sel) {
            prop(ui, "尺寸", format!("{}×{}", img.width(), img.height()));
        }
    }
    ui.separator();
    if let Some(err) = &app.ide.err {
        ui.colored_label(ERR, err);
    }
    ui.horizontal(|ui| {
        if ui.button("打开目录").clicked() {
            let dir = sel.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let _ = std::process::Command::new("explorer").arg(dir).spawn();
        }
        if ui.button("复制路径").clicked() {
            ui.ctx().copy_text(sel.display().to_string());
        }
    });
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut app.ide.new_file);
        if ui.button("新建").clicked() {
            let n = app.ide.new_file.trim().to_string();
            if !n.is_empty() {
                let dir = sel.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let target = dir.join(&n);
                if target.exists() {
                    app.ide.err = Some("同名文件已存在".to_string());
                } else {
                    let content = if n.ends_with(".wgsl") {
                        "// 新建着色器\n".to_string()
                    } else if n.ends_with(".ron") {
                        "(\n)\n".to_string()
                    } else {
                        String::new()
                    };
                    match std::fs::write(&target, content) {
                        Ok(_) => {
                            app.ide.err = None;
                            app.ide.sel = Some(target);
                            app.ide.text_of = None;
                        }
                        Err(e) => app.ide.err = Some(e.to_string()),
                    }
                }
                app.ide.new_file.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.ide.confirm_del, "确认删除");
        if ui
            .add_enabled(app.ide.confirm_del, egui::Button::new("删除"))
            .clicked()
        {
            match std::fs::remove_file(&sel) {
                Ok(_) => {
                    app.ide.sel = None;
                    app.ide.text_of = None;
                    app.ide.err = None;
                    app.ide.confirm_del = false;
                }
                Err(e) => app.ide.err = Some(e.to_string()),
            }
        }
    });
}

/// IDE 配置面板：界面字号（默认 13px，持久化到 saves/ide_config.ron）
pub(crate) fn draw_ide_config(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.horizontal(|ui| {
        ui.label("界面字号");
        let r = ui.add(
            egui::Slider::new(&mut app.ide.font_size, 10.0..=20.0)
                .step_by(1.0)
                .suffix(" px"),
        );
        if r.drag_stopped() || r.changed() {
            IdeConfig { font_size: app.ide.font_size }.save();
        }
    });
    if ui.button(format!("恢复默认（{} px）", DEFAULT_FONT)).clicked() {
        app.ide.font_size = DEFAULT_FONT;
        IdeConfig { font_size: DEFAULT_FONT }.save();
    }
    ui.weak("字号对整个 IDE 界面生效，保存在 saves/ide_config.ron");
}

pub(crate) fn draw_asset_tools(ui: &mut egui::Ui, app: &mut GameApp) {
    egui::CollapsingHeader::new("导入精灵表")
        .id_salt("ide_import_sheet")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("源 PNG");
                ui.text_edit_singleline(&mut app.ide.imp_src);
            });
            ui.horizontal(|ui| {
                ui.label("名称");
                ui.text_edit_singleline(&mut app.ide.imp_name);
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut app.ide.imp_fw).range(1..=512).prefix("帧宽 "));
                ui.add(egui::DragValue::new(&mut app.ide.imp_fh).range(1..=512).prefix("帧高 "));
            });
            if ui.button("导入并注册动画").clicked() {
                import_sprite_sheet(app);
            }
            if let Some(m) = &app.ide.imp_msg {
                ui.colored_label(OK, m);
            }
            ui.weak("PNG 复制到工程 anims/ 并写入 animations.ron，热重载即时生效");
        });
    if ui.button("导出工程副本").clicked() {
        export_project(app);
    }
    if let Some(m) = &app.ide.export_msg {
        ui.colored_label(OK, m);
    }
}

fn import_sprite_sheet(app: &mut GameApp) {
    let src = app.ide.imp_src.trim().trim_matches('"').to_string();
    let name = app.ide.imp_name.trim().to_string();
    if src.is_empty() || name.is_empty() {
        app.ide.imp_msg = None;
        app.ide.err = Some("请填写源 PNG 路径与动画名称".to_string());
        return;
    }
    let Ok(img) = image::open(&src) else {
        app.ide.err = Some(format!("无法打开图片: {src}"));
        return;
    };
    let (w, h) = (img.width(), img.height());
    let (fw, fh) = (app.ide.imp_fw.max(1), app.ide.imp_fh.max(1));
    if w < fw || h < fh {
        app.ide.err = Some(format!("图片 {w}×{h} 小于帧尺寸 {fw}×{fh}"));
        return;
    }
    let frames = (w / fw) * (h / fh);
    let dir = project::dir_of("anims");
    let _ = std::fs::create_dir_all(&dir);
    let file = format!("{name}.png");
    if let Err(e) = std::fs::copy(&src, dir.join(&file)) {
        app.ide.err = Some(format!("复制失败: {e}"));
        return;
    }
    let def = crate::anim::AnimDef {
        name: name.clone(),
        sheet: file,
        frame_w: fw,
        frame_h: fh,
        frame_times: vec![0.12; frames as usize],
        events: Vec::new(),
        r#loop: true,
    };
    app.anims.defs.insert(name.clone(), def);
    match app.anims.save() {
        Ok(_) => {
            app.ide.err = None;
            app.ide.imp_msg = Some(format!("已导入「{name}」{frames} 帧（{fw}×{fh}）"));
            tracing::info!("IDE 导入精灵表 {} 帧数 {}", name, frames);
        }
        Err(e) => app.ide.err = Some(e),
    }
}

fn export_project(app: &mut GameApp) {
    let Some(p) = app.project.current.clone() else {
        app.ide.export_msg = None;
        app.ide.err = Some("请先打开一个工程再导出".to_string());
        return;
    };
    let out = PathBuf::from("exports").join(&p.name);
    match copy_dir(&p.root, &out) {
        Ok(n) => {
            app.ide.err = None;
            app.ide.export_msg = Some(format!("已导出 {n} 个文件 → {}", out.display()));
        }
        Err(e) => app.ide.err = Some(format!("导出失败: {e}")),
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<usize, String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let mut n = 0;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let tp = dst.join(e.file_name());
        if e.path().is_dir() {
            n += copy_dir(&e.path(), &tp)?;
        } else {
            std::fs::copy(e.path(), &tp).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

// ---------------------------------------------------------------------------

pub(crate) fn draw_file_view(ui: &mut egui::Ui, app: &mut GameApp) {
    let Some(sel) = app.ide.sel.clone() else {
        ui.centered_and_justified(|ui| {
            ui.heading("从左侧素材树选择文件");
        });
        return;
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(sel.file_name().and_then(|n| n.to_str()).unwrap_or("?"))
                .strong()
                .color(ACCENT),
        );
        if is_text(&sel) {
            if app.ide.dirty {
                ui.colored_label(WARN, "已修改未保存");
            } else {
                ui.colored_label(OK, "已保存");
            }
            if ui.button("保存 (Ctrl+S)").clicked() {
                save_text(app);
            }
            if ui.button("重载").clicked() {
                app.ide.text_of = None;
                app.ide.dirty = false;
            }
        }
    });
    ui.separator();

    if is_text(&sel) {
        if app.ide.text_of.as_deref() != Some(sel.as_path()) {
            app.ide.text = std::fs::read_to_string(&sel).unwrap_or_default();
            app.ide.text_of = Some(sel.clone());
            app.ide.dirty = false;
        }
        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            save_text(app);
        }
        egui::ScrollArea::both().id_salt("ide_file_text_scroll").show(ui, |ui| {
            let r = ui.add_sized(
                ui.available_size(),
                egui::TextEdit::multiline(&mut app.ide.text).font(egui::TextStyle::Monospace),
            );
            if r.changed() {
                app.ide.dirty = true;
            }
        });
    } else if sel.extension().and_then(|e| e.to_str()) == Some("png") {
        app.ide.text_of = Some(sel.clone());
        match image::open(&sel) {
            Ok(img) => {
                let (w, h) = (img.width(), img.height());
                ui.monospace(format!("PNG  {}×{}", w, h));
                let rgba = img.to_rgba8();
                let scale = (240.0 / w.max(1) as f32).clamp(0.5, 8.0);
                let size = egui::vec2(w as f32 * scale, h as f32 * scale);
                let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
                for y in 0..h.min(256) {
                    for x in 0..w.min(256) {
                        let p = rgba.get_pixel(x, y).0;
                        if p[3] == 0 {
                            continue;
                        }
                        let rect = egui::Rect::from_min_size(
                            resp.rect.min + egui::vec2(x as f32 * scale, y as f32 * scale),
                            egui::vec2(scale, scale),
                        );
                        painter.rect_filled(
                            rect,
                            0.0,
                            Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]),
                        );
                    }
                }
            }
            Err(e) => {
                ui.colored_label(ERR, format!("无法读取: {e}"));
            }
        }
    } else {
        ui.weak("不支持预览的文件类型");
    }
}

fn save_text(app: &mut GameApp) {
    let Some(sel) = app.ide.sel.clone() else { return };
    match std::fs::write(&sel, &app.ide.text) {
        Ok(_) => {
            app.ide.dirty = false;
            app.ide.err = None;
            tracing::info!("IDE 已保存 {}", sel.display());
        }
        Err(e) => app.ide.err = Some(e.to_string()),
    }
}
