//! 引擎 IDE（F4）：左中右三栏 —— 左=工程+素材树+场景树，中=视口/文件编辑，右=检查器
//!
//! UI 约定：所有模块统一用 `section()` 可折叠区块（状态存在 Ide 里），配色与图标保持一致。
//! - 中栏「视口」：面板透明，直接透出实时游戏画面（可运行/暂停）
//! - 左栏「场景树」：玩家 / 怪物 / 居民 / 训练假人，可点选
//! - 右栏「检查器」：选中实体显示组件属性；选中文件显示文件属性与操作
//! - 素材工具：精灵表导入向导（PNG → 切帧 → 写入 animations.ron）、工程导出副本
use crate::project;
use crate::GameApp;
use egui::{Color32, Margin, RichText, Stroke};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// 主题色（IDE 统一配色）
// ---------------------------------------------------------------------------
const BG_PANEL: Color32 = Color32::from_rgb(26, 27, 32);
const BG_SECTION: Color32 = Color32::from_rgb(32, 33, 40);
const BG_BAR: Color32 = Color32::from_rgb(22, 23, 28);
const LINE: Color32 = Color32::from_rgb(52, 54, 66);
const LINE_W: f32 = 1.0;
const ACCENT: Color32 = Color32::from_rgb(96, 165, 250);
const OK: Color32 = Color32::from_rgb(134, 220, 160);
const WARN: Color32 = Color32::from_rgb(240, 200, 90);
const ERR: Color32 = Color32::from_rgb(232, 118, 100);
const DIM: Color32 = Color32::from_rgb(150, 154, 168);

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
    /// 中栏页：0 视口 / 1 文件
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
    // ---- 各区块展开状态 ----
    pub sec_proj: bool,
    pub sec_assets: bool,
    pub sec_scene: bool,
    pub sec_ent: bool,
    pub sec_file: bool,
    pub sec_tools: bool,
}

impl Default for Ide {
    fn default() -> Self {
        Self {
            open: false,
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
            sec_proj: true,
            sec_assets: true,
            sec_scene: true,
            sec_ent: true,
            sec_file: true,
            sec_tools: false,
        }
    }
}

fn is_text(p: &PathBuf) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("ron") | Some("wgsl") | Some("txt") | Some("md")
    )
}

/// 可折叠区块：圆角卡片 + 三角图标 + 标题，展开时展示内容
fn section(
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
fn prop(ui: &mut egui::Ui, k: &str, v: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.weak(k);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.monospace(v.into());
        });
    });
}

/// 分组标题（树内小节）
fn group(ui: &mut egui::Ui, name: &str, count: usize) {
    ui.horizontal(|ui| {
        ui.weak(name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(DIM, RichText::new(format!("{count}")).small());
        });
    });
}

pub fn draw(app: &mut GameApp, ctx: &egui::Context) {
    if !app.ide.open {
        return;
    }
    let proj = app.project.current.as_ref().map(|p| p.name.clone());
    let title = match &proj {
        Some(n) => format!("引擎 IDE — 工程「{n}」"),
        None => "引擎 IDE — 内置资源".to_string(),
    };

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
            // 模块之间零间距（标题栏与区块紧贴）
            ui.spacing_mut().item_spacing.y = 0.0;
            // 标题栏（直角满宽）
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
                                    .selectable_label(
                                        app.project.current.is_none(),
                                        "内置资源（关闭工程）",
                                    )
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
                    let mut q =
                        app.ecs.query::<(&crate::entities::Transform, &crate::entities::Dummy)>();
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
            });
        });

    // ---------------- 右栏 ----------------
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
                // ---- 实体属性 ----
                let mut open = app.ide.sec_ent;
                section(ui, "实体属性", &mut open, |ui| {
                    match app.ide.ent {
                        None => {
                            ui.weak("未选中实体（左侧场景树点选）");
                        }
                        Some(e) => draw_entity_props(ui, app, e),
                    }
                });
                app.ide.sec_ent = open;
                // ---- 文件 ----
                let mut open = app.ide.sec_file;
                section(ui, "文件", &mut open, |ui| {
                    draw_file_props(ui, app);
                });
                app.ide.sec_file = open;
                // ---- 素材工具 ----
                let mut open = app.ide.sec_tools;
                section(ui, "素材工具", &mut open, |ui| {
                    draw_asset_tools(ui, app);
                });
                app.ide.sec_tools = open;
            });
        });

    // ---------------- 中栏：视口 / 文件 ----------------
    egui::CentralPanel::default()
        .frame(if app.ide.tab == 1 {
            // 文件模式：整块不透明（避免透出游戏画面）
            egui::Frame::NONE.fill(BG_PANEL)
        } else {
            // 视口模式：透明，直接透出实时游戏画面
            egui::Frame::NONE.fill(Color32::TRANSPARENT)
        })
        .show(ctx, |ui| {
            // 工具条与内容区零间距堆叠（消除缝隙）
            ui.spacing_mut().item_spacing.y = 0.0;
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
                            let (txt, col) = if app.ide.run {
                                ("暂停", WARN)
                            } else {
                                ("运行", OK)
                            };
                            if ui.button(RichText::new(txt).color(col)).clicked() {
                                app.ide.run = !app.ide.run;
                            }
                            ui.label("昼夜");
                            ui.add(
                                egui::Slider::new(&mut app.world.time, 0.0..=1.0)
                                    .show_value(false),
                            );
                            ui.monospace(format!("{:.2}", app.world.time));
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                let st = if app.ide.run { "运行中" } else { "已暂停" };
                                ui.colored_label(
                                    if app.ide.run { OK } else { WARN },
                                    format!("{st}"),
                                );
                                ui.weak(proj.clone().unwrap_or_else(|| "内置资源".into()));
                            },
                        );
                    });
                });

            if app.ide.tab == 1 {
                // 文件区紧接工具条（无间距、直角、不透明）
                egui::Frame::NONE
                    .fill(BG_PANEL)
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        draw_file_view(ui, app);
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
        });
}

// ---------------------------------------------------------------------------

fn draw_entity_props(ui: &mut egui::Ui, app: &mut GameApp, ent: EntSel) {
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

fn draw_file_props(ui: &mut egui::Ui, app: &mut GameApp) {
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

fn draw_asset_tools(ui: &mut egui::Ui, app: &mut GameApp) {
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

fn draw_file_view(ui: &mut egui::Ui, app: &mut GameApp) {
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
            Err(e) => { ui.colored_label(ERR, format!("无法读取: {e}")); }
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
