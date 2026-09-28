//! IDE 插件系统：**功能 = 插件**，IDE 壳（ide.rs）只负责顶部菜单栏与左/中/右三栏分发。
//!
//! - 每个插件实现 `IdePlugin`：菜单名 + 左/中/右栏按需实现 + 每帧激活回调
//! - 新增功能 = 新增一个插件结构体并加入 `all_plugins()`，UI 壳零改动
//! - 调整 UI = 只改插件的面板实现，不触碰游戏功能（编辑器数据服务在 main.rs，与本层无关）
use crate::ide::{
    self, group, section, EntSel, BG_BAR, BG_PANEL, ACCENT, DIM, LINE, LINE_W, OK, WARN,
};
use crate::project;
use crate::GameApp;
use egui::{Margin, RichText, Stroke};

/// IDE 功能插件
pub trait IdePlugin {
    /// 顶部菜单栏显示名
    fn label(&self) -> &'static str;
    /// 左栏顶条标题（默认 = 菜单名）
    fn title(&self) -> &'static str {
        self.label()
    }
    /// 中栏是否需要不透明背景（默认透明，透出实时游戏画面）
    fn wants_opaque(&self, _app: &GameApp) -> bool {
        false
    }
    /// 是否渲染右栏
    fn has_right(&self) -> bool {
        false
    }
    /// 每帧激活回调（本功能处于激活状态时每帧调用；做状态同步等）
    fn on_active_frame(&self, _app: &mut GameApp) {}
    /// 左栏（资源列表 / 工程面板）
    fn left_panel(&self, _ui: &mut egui::Ui, _app: &mut GameApp) {}
    /// 中栏（主编辑区 / 视口）
    fn central_panel(&self, _ui: &mut egui::Ui, _app: &mut GameApp) {}
    /// 右栏（检查器等，可选）
    fn right_panel(&self, _ui: &mut egui::Ui, _app: &mut GameApp) {}
}

/// 全部已注册插件（顺序 = 菜单栏顺序；`app.ide.func` 为下标）
pub fn all_plugins() -> Vec<Box<dyn IdePlugin>> {
    vec![
        Box::new(ProjectPlugin),
        Box::new(VfxPlugin),
        Box::new(AnimPlugin),
        Box::new(VegPlugin),
        Box::new(CharPlugin),
        Box::new(ItemPlugin),
    ]
}

// ---------------------------------------------------------------------------
// 工程插件（原 IDE 主体：视口 / 文件编辑 / 检查器）
// ---------------------------------------------------------------------------

struct ProjectPlugin;

impl IdePlugin for ProjectPlugin {
    fn label(&self) -> &'static str {
        "工程"
    }
    fn title(&self) -> &'static str {
        "工程"
    }
    fn wants_opaque(&self, app: &GameApp) -> bool {
        app.ide.tab == 1
    }
    fn has_right(&self) -> bool {
        true
    }

    fn left_panel(&self, ui: &mut egui::Ui, app: &mut GameApp) {
        let proj = app.project.current.as_ref().map(|p| p.name.clone());
        // ---- 工程 ----
        section(ui, "工程", &mut app.ide.sec_proj, |ui| {
            ui.horizontal(|ui| {
                ui.label("新建");
                ui.text_edit_singleline(&mut app.project.new_name);
                if ui.button("创建").clicked() {
                    let name = app.project.new_name.clone();
                    app.project.create(&name);
                    app.project.new_name.clear();
                }
            });
            ui.horizontal(|ui| {
                ui.label("切换");
                egui::ComboBox::from_id_salt("ide_proj_combo")
                    .width(ui.available_width() - 8.0)
                    .selected_text(proj.clone().unwrap_or_else(|| "内置资源".into()))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(app.project.current.is_none(), "内置资源（关闭工程）")
                            .clicked()
                        {
                            app.project.close();
                        }
                        for name in project::list_projects() {
                            let cur = proj.as_deref() == Some(name.as_str());
                            if ui.selectable_label(cur, &name).clicked() {
                                app.project.open(&name);
                            }
                        }
                    });
            });
            if let Some(msg) = &app.project.msg {
                ui.colored_label(OK, msg);
            }
            ui.weak(
                app.project
                    .current
                    .as_ref()
                    .map(|p| p.root.display().to_string())
                    .unwrap_or_else(|| "内置 assets/".to_string()),
            );
        });

        // ---- 素材树 ----
        section(ui, "素材", &mut app.ide.sec_assets, |ui| {
            for (kind, (label, _d, _e)) in project::GROUPS.iter().enumerate() {
                let files = project::list_group(kind);
                egui::CollapsingHeader::new(format!("{} ({})", label, files.len()))
                    .id_salt(format!("ide_assets_group_{kind}"))
                    .default_open(kind == 0)
                    .show(ui, |ui| {
                        for p in files {
                            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                            let sel = app.ide.sel.as_deref() == Some(p.as_path());
                            if ui.selectable_label(sel, name).clicked() {
                                app.ide.sel = Some(p.clone());
                                app.ide.text_of = None;
                                app.ide.dirty = false;
                                app.ide.confirm_del = false;
                                app.ide.ent = None;
                                app.ide.tab = 1;
                            }
                        }
                    });
            }
        });

        // ---- 场景树 ----
        section(ui, "场景", &mut app.ide.sec_scene, |ui| {
            group(ui, "玩家", 1);
            if ui
                .selectable_label(app.ide.ent == Some(EntSel::Player), "玩家")
                .clicked()
            {
                app.ide.ent = Some(EntSel::Player);
                app.ide.sel = None;
            }
            group(ui, "怪物", app.monsters.list.len());
            for i in 0..app.monsters.list.len() {
                let m = &app.monsters.list[i];
                let label = format!("{}  生命 {:.0}", m.kind.name(), m.hp);
                if ui
                    .selectable_label(app.ide.ent == Some(EntSel::Monster(i)), label)
                    .clicked()
                {
                    app.ide.ent = Some(EntSel::Monster(i));
                    app.ide.sel = None;
                }
            }
            group(ui, "居民", app.npcs.list.len());
            for i in 0..app.npcs.list.len() {
                let n = &app.npcs.list[i];
                let label = format!("居民 {}  {}", i, n.state.name());
                if ui
                    .selectable_label(app.ide.ent == Some(EntSel::Npc(i)), label)
                    .clicked()
                {
                    app.ide.ent = Some(EntSel::Npc(i));
                    app.ide.sel = None;
                }
            }
            let mut q = app.ecs.query::<(&crate::entities::Transform, &crate::entities::Dummy)>();
            let dummies: Vec<(hecs::Entity, glam::Vec2, f32)> =
                q.iter().map(|(e, (tr, d))| (e, tr.pos, d.hp)).collect();
            drop(q);
            group(ui, "假人", dummies.len());
            for (e, pos, hp) in dummies {
                let label = format!("假人 ({:.0},{:.0}) 生命 {:.0}", pos.x, pos.y, hp);
                if ui
                    .selectable_label(app.ide.ent == Some(EntSel::Dummy(e)), label)
                    .clicked()
                {
                    app.ide.ent = Some(EntSel::Dummy(e));
                    app.ide.sel = None;
                }
            }
        });
    }

    fn central_panel(&self, ui: &mut egui::Ui, app: &mut GameApp) {
        let proj = app.project.current.as_ref().map(|p| p.name.clone());
        // 顶部工具条：直角、满宽、底部分隔线（圆角会在四角透出游戏画面）
        egui::Frame::NONE
            .fill(BG_BAR)
            .stroke(Stroke::new(LINE_W, LINE))
            .inner_margin(Margin::symmetric(6, 5))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let t0 = if app.ide.tab == 0 { ACCENT } else { DIM };
                    let t1 = if app.ide.tab == 1 { ACCENT } else { DIM };
                    if ui.button(RichText::new("视口").color(t0)).clicked() {
                        app.ide.tab = 0;
                    }
                    if ui.button(RichText::new("文件").color(t1)).clicked() {
                        app.ide.tab = 1;
                    }
                    ui.separator();
                    if app.ide.tab == 0 {
                        let (txt, col) = if app.ide.run { ("暂停", WARN) } else { ("运行", OK) };
                        if ui.button(RichText::new(txt).color(col)).clicked() {
                            app.ide.run = !app.ide.run;
                        }
                        ui.label("昼夜");
                        ui.add(
                            egui::Slider::new(&mut app.world.time, 0.0..=1.0).show_value(false),
                        );
                        ui.monospace(format!("{:.2}", app.world.time));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let st = if app.ide.run { "运行中" } else { "已暂停" };
                        ui.colored_label(if app.ide.run { OK } else { WARN }, format!("{st}"));
                        ui.weak(proj.clone().unwrap_or_else(|| "内置资源".into()));
                    });
                });
            });

        if app.ide.tab == 1 {
            // 文件区紧接工具条（无间距、直角、不透明）
            egui::Frame::NONE
                .fill(BG_PANEL)
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ide::draw_file_view(ui, app);
                });
        }

        // 底部状态栏（直角满宽，无圆角透视）
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            egui::Frame::NONE
                .fill(BG_BAR)
                .stroke(Stroke::new(LINE_W, LINE))
                .inner_margin(Margin::symmetric(6, 5))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.weak(if app.ide.tab == 0 {
                            "视口显示实时游戏画面（可暂停/运行，拖昼夜滑块看光照）"
                        } else {
                            "文件编辑：Ctrl+S 保存，保存后热重载立即生效"
                        });
                    });
                });
        });
    }

    fn right_panel(&self, ui: &mut egui::Ui, app: &mut GameApp) {
        // ---- 实体属性 ----
        let mut open = app.ide.sec_ent;
        section(ui, "实体属性", &mut open, |ui| {
            match app.ide.ent {
                None => {
                    ui.weak("未选中实体（左侧场景树点选）");
                }
                Some(e) => ide::draw_entity_props(ui, app, e),
            }
        });
        app.ide.sec_ent = open;
        // ---- 文件 ----
        let mut open = app.ide.sec_file;
        section(ui, "文件", &mut open, |ui| {
            ide::draw_file_props(ui, app);
        });
        app.ide.sec_file = open;
        // ---- 素材工具 ----
        let mut open = app.ide.sec_tools;
        section(ui, "素材工具", &mut open, |ui| {
            ide::draw_asset_tools(ui, app);
        });
        app.ide.sec_tools = open;
        // ---- IDE 配置 ----
        let mut open = app.ide.sec_cfg;
        section(ui, "配置", &mut open, |ui| {
            ide::draw_ide_config(ui, app);
        });
        app.ide.sec_cfg = open;
    }
}

// ---------------------------------------------------------------------------
// 编辑器插件（特效 / 动画 / 植被 / 人物 / 物品）：左栏=资源列表，中栏=编辑器页
// ---------------------------------------------------------------------------

macro_rules! editor_plugin {
    ($name:ident, $label:literal, $tab:expr, $list:expr, $central:expr) => {
        struct $name;
        impl IdePlugin for $name {
            fn label(&self) -> &'static str {
                $label
            }
            fn wants_opaque(&self, _app: &GameApp) -> bool {
                true
            }
            fn on_active_frame(&self, app: &mut GameApp) {
                // 编辑器状态同步：热重载保护 + 表单页签
                app.editor.open = true;
                app.editor.tab = $tab;
            }
            fn left_panel(&self, ui: &mut egui::Ui, app: &mut GameApp) {
                $list(ui, app);
            }
            fn central_panel(&self, ui: &mut egui::Ui, app: &mut GameApp) {
                egui::ScrollArea::vertical()
                    .id_salt(concat!("ide_plugin_", stringify!($name)))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        $central(ui, app);
                    });
            }
        }
    };
}

/// 特效蓝图列表
fn vfx_list(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.heading("蓝图");
    ui.separator();
    let mut names: Vec<String> = app.vfx.bps.keys().cloned().collect();
    names.sort();
    for n in names {
        if ui.selectable_label(app.editor.sel == n, &n).clicked() {
            app.editor.sel = n;
        }
    }
}

/// 动画列表
fn anim_list(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.heading("动画");
    ui.separator();
    let mut names: Vec<String> = app.anims.defs.keys().cloned().collect();
    names.sort();
    for n in names {
        let cnt = app.anims.frame_count(&n);
        if ui
            .selectable_label(app.editor.anim_sel == n, format!("{n} ({cnt}帧)"))
            .clicked()
        {
            app.editor.anim_sel = n;
        }
    }
}

/// 植被列表
fn veg_list(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.heading("植被");
    ui.separator();
    for (i, p) in app.veg.plants.iter().enumerate() {
        if ui
            .selectable_label(app.editor.veg_sel == i, &p.name)
            .clicked()
        {
            app.editor.veg_sel = i;
        }
    }
}

/// 人物部件列表
fn char_list(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.heading("部件");
    ui.separator();
    for (i, p) in crate::character::PARTS.iter().enumerate() {
        if ui
            .selectable_label(app.editor.char_sel == i, p.label)
            .clicked()
        {
            app.editor.char_sel = i;
        }
    }
}

/// 物品列表
fn item_list(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.heading("物品");
    ui.separator();
    let mut ids: Vec<String> = app.db.defs.keys().cloned().collect();
    ids.sort();
    egui::ScrollArea::vertical()
        .id_salt("plugin_item_list")
        .max_height(430.0)
        .show(ui, |ui| {
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
}

editor_plugin!(VfxPlugin, "特效", 0, vfx_list, crate::editor::tab_vfx);
editor_plugin!(AnimPlugin, "动画", 1, anim_list, crate::editor::tab_anim);
editor_plugin!(VegPlugin, "植被", 2, veg_list, crate::editor::tab_veg);
editor_plugin!(CharPlugin, "人物", 3, char_list, crate::editor::tab_char);
editor_plugin!(ItemPlugin, "物品", 4, item_list, crate::editor::tab_items);
