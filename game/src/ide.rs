//! 引擎 IDE（F4）：左中右三栏 —— 左=工程+素材树+场景树，中=视口/文件编辑，右=检查器
//!
//! - 中栏「视口」：面板透明，直接透出实时游戏画面（可运行/暂停），边改边看
//! - 左栏「场景树」：玩家 / 怪物 / NPC / 假人，可点选
//! - 右栏「检查器」：选中实体显示组件属性（实时数值）；选中文件显示文件属性与操作
//! - 素材工具：精灵表导入向导（PNG → 切帧 → 写入 animations.ron）、工程导出副本
use crate::project;
use crate::GameApp;
use egui::{Color32, RichText};
use std::path::PathBuf;

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
    /// 视口运行开关（IDE 打开时是否推进世界模拟）
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
        }
    }
}

fn is_text(p: &PathBuf) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("ron") | Some("wgsl") | Some("txt") | Some("md")
    )
}

pub fn draw(app: &mut GameApp, ctx: &egui::Context) {
    if !app.ide.open {
        return;
    }
    let title = match &app.project.current {
        Some(p) => format!("引擎 IDE — 工程「{}」", p.name),
        None => "引擎 IDE — 内置资源（未打开工程）".to_string(),
    };

    // ---------------- 左：工程 + 素材树 + 场景树 ----------------
    egui::SidePanel::left("ide_left")
        .resizable(true)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.heading(&title);
            ui.separator();

            // ---- 工程区 ----
            ui.label(RichText::new("工程").strong());
            ui.horizontal(|ui| {
                ui.label("新建：");
                ui.text_edit_singleline(&mut app.project.new_name);
                if ui.button("创建工程").clicked() {
                    let name = app.project.new_name.clone();
                    app.project.create(&name);
                    app.project.new_name.clear();
                    app.ide.sel = None;
                    app.ide.text_of = None;
                }
            });
            ui.horizontal(|ui| {
                ui.label("打开：");
                egui::ComboBox::from_id_salt("ide_open_proj")
                    .selected_text(
                        app.project
                            .current
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "内置资源".to_string()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(app.project.current.is_none(), "内置资源（关闭工程）")
                            .clicked()
                        {
                            app.project.close();
                        }
                        for name in project::list_projects() {
                            let cur = app.project.current.as_ref().map(|p| &p.name) == Some(&name);
                            if ui.selectable_label(cur, &name).clicked() {
                                app.project.open(&name);
                            }
                        }
                    });
            });
            if let Some(msg) = &app.project.msg {
                ui.colored_label(Color32::from_rgb(150, 220, 160), msg);
            }
            ui.small(format!(
                "工程根：{}",
                app.project
                    .current
                    .as_ref()
                    .map(|p| p.root.display().to_string())
                    .unwrap_or_else(|| "（内置 assets/）".to_string())
            ));
            ui.separator();

            // ---- 素材树 ----
            ui.label(RichText::new("素材").strong());
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for (kind, (label, _dir, _ext)) in project::GROUPS.iter().enumerate() {
                        let files = project::list_group(kind);
                        egui::CollapsingHeader::new(format!("{} ({})", label, files.len()))
                            .default_open(kind < 2)
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
            ui.separator();

            // ---- 场景树 ----
            ui.label(RichText::new("场景").strong());
            egui::ScrollArea::vertical().show(ui, |ui| {
                let p = EntSel::Player;
                if ui
                    .selectable_label(app.ide.ent == Some(p), "玩家 Player")
                    .clicked()
                {
                    app.ide.ent = Some(p);
                    app.ide.sel = None;
                }
                ui.small(format!("怪物 ({})", app.monsters.list.len()));
                for i in 0..app.monsters.list.len() {
                    let m = &app.monsters.list[i];
                    let label = format!("{:?}  hp {:.0}", m.kind, m.hp);
                    if ui
                        .selectable_label(app.ide.ent == Some(EntSel::Monster(i)), label)
                        .clicked()
                    {
                        app.ide.ent = Some(EntSel::Monster(i));
                        app.ide.sel = None;
                    }
                }
                ui.small(format!("NPC ({})", app.npcs.list.len()));
                for i in 0..app.npcs.list.len() {
                    let n = &app.npcs.list[i];
                    let label = format!("NPC {}  {:?}", i, n.state);
                    if ui
                        .selectable_label(app.ide.ent == Some(EntSel::Npc(i)), label)
                        .clicked()
                    {
                        app.ide.ent = Some(EntSel::Npc(i));
                        app.ide.sel = None;
                    }
                }
                // 训练假人（ECS）
                let mut q = app.ecs.query::<(&crate::entities::Transform, &crate::entities::Dummy)>();
                let dummies: Vec<(hecs::Entity, glam::Vec2, f32)> =
                    q.iter().map(|(e, (tr, d))| (e, tr.pos, d.hp)).collect();
                drop(q);
                ui.small(format!("假人 ({})", dummies.len()));
                for (e, pos, hp) in dummies {
                    if ui
                        .selectable_label(
                            app.ide.ent == Some(EntSel::Dummy(e)),
                            format!("Dummy ({:.0},{:.0}) hp {:.0}", pos.x, pos.y, hp),
                        )
                        .clicked()
                    {
                        app.ide.ent = Some(EntSel::Dummy(e));
                        app.ide.sel = None;
                    }
                }
            });
        });

    // ---------------- 右：检查器 ----------------
    egui::SidePanel::right("ide_right")
        .resizable(true)
        .default_width(250.0)
        .show(ctx, |ui| {
            ui.heading("检查器");
            ui.separator();
            if let Some(ent) = app.ide.ent {
                draw_entity_inspector(ui, app, ent);
                ui.separator();
            }
            draw_file_inspector(ui, app);
            ui.separator();
            draw_asset_tools(ui, app);
        });

    // ---------------- 中：视口 / 文件 ----------------
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
        .show(ctx, |ui| {
            // 顶部工具条（不透明，便于阅读）
            egui::Frame::NONE
                .fill(Color32::from_rgba_premultiplied(24, 24, 30, 220))
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut app.ide.tab, 0, "🎬 视口");
                        ui.selectable_value(&mut app.ide.tab, 1, "📄 文件");
                        ui.separator();
                        if app.ide.tab == 0 {
                            if ui
                                .button(if app.ide.run { "⏸ 暂停" } else { "▶ 运行" })
                                .clicked()
                            {
                                app.ide.run = !app.ide.run;
                            }
                            ui.label("昼夜");
                            ui.add(
                                egui::Slider::new(&mut app.world.time, 0.0..=1.0)
                                    .show_value(false),
                            );
                            ui.monospace(format!("t={:.2}", app.world.time));
                        }
                    });
                });
            if app.ide.tab == 1 {
                // 文件内容区（不透明底）
                egui::Frame::NONE
                    .fill(Color32::from_rgba_premultiplied(20, 20, 26, 235))
                    .show(ui, |ui| {
                        draw_file_view(ui, app);
                    });
            } else {
                ui.with_layout(
                    egui::Layout::bottom_up(egui::Align::LEFT),
                    |ui| {
                        ui.label(
                            RichText::new(
                                "视口：直接显示实时游戏画面（暂停/运行在上方切换）",
                            )
                            .weak(),
                        );
                    },
                );
            }
        });
}

// ---------------------------------------------------------------------------

fn draw_entity_inspector(ui: &mut egui::Ui, app: &mut GameApp, ent: EntSel) {
    match ent {
        EntSel::Player => {
            ui.label(RichText::new("玩家 Player").strong());
            let p = &app.player;
            ui.monospace(format!("位置 ({:.1}, {:.1})", p.pos.x, p.pos.y));
            ui.monospace(format!("速度 ({:.1}, {:.1})", p.vel.x, p.vel.y));
            ui.monospace(format!("生命 {:.0} / {:.0}", p.hp, p.max_hp));
            ui.monospace(format!("朝向 {:.0}  落地 {}", p.facing, p.on_ground));
            ui.monospace(format!("等级 {}  经验 {}", app.inv.level, app.inv.xp));
            ui.monospace(format!(
                "工具 {:?}  技能点 {}",
                app.tool.tool, app.skills.pts
            ));
        }
        EntSel::Monster(i) => {
            let Some(m) = app.monsters.list.get(i) else {
                app.ide.ent = None;
                return;
            };
            ui.label(RichText::new(format!("怪物 {:?}", m.kind)).strong());
            ui.monospace(format!("位置 ({:.1}, {:.1})", m.pos.x, m.pos.y));
            ui.monospace(format!("生命 {:.0} / {:.0}", m.hp, m.max_hp));
            ui.monospace(format!("状态 {:?}", m.state));
            ui.monospace(format!("速度 {:.0}  伤害 {:.0}", m.speed, m.dmg));
            ui.monospace(format!("阶段 {}  路径点 {}", m.phase, m.path.len()));
        }
        EntSel::Npc(i) => {
            let Some(n) = app.npcs.list.get(i) else {
                app.ide.ent = None;
                return;
            };
            ui.label(RichText::new(format!("NPC #{}", i)).strong());
            ui.monospace(format!("位置 ({:.1}, {:.1})", n.pos.x, n.pos.y));
            ui.monospace(format!("状态 {:?}", n.state));
            ui.monospace(format!("饥饿 {:.0}  口渴 {:.0}", n.hunger, n.thirst));
            ui.monospace(format!("疲劳 {:.0}", n.fatigue));
            ui.monospace(format!("家 ({:.0}, {:.0})", n.home.x, n.home.y));
        }
        EntSel::Dummy(e) => {
            ui.label(RichText::new("训练假人 Dummy").strong());
            if let Ok(tr) = app.ecs.get::<&crate::entities::Transform>(e) {
                let pos = tr.pos;
                drop(tr);
                ui.monospace(format!("位置 ({:.1}, {:.1})", pos.x, pos.y));
            }
            if let Ok(d) = app.ecs.get::<&crate::entities::Dummy>(e) {
                ui.monospace(format!("生命 {:.0}  重生 {:?}", d.hp, d.respawn));
            }
        }
    }
}

fn draw_file_inspector(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.label(RichText::new("文件").strong());
    let Some(sel) = app.ide.sel.clone() else {
        ui.weak("未选中文件（左侧素材树）");
        return;
    };
    let name = sel.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
    ui.monospace(&name);
    ui.weak(sel.display().to_string());
    if let Ok(md) = std::fs::metadata(&sel) {
        ui.monospace(format!("大小 {} 字节", md.len()));
    }
    if is_text(&sel) {
        ui.monospace(format!("行数 {}", app.ide.text.lines().count()));
    } else if sel.extension().and_then(|e| e.to_str()) == Some("png") {
        if let Ok(img) = image::open(&sel) {
            ui.monospace(format!("图像 {}×{}", img.width(), img.height()));
        }
    }
    ui.separator();
    if let Some(err) = &app.ide.err {
        ui.colored_label(Color32::from_rgb(230, 120, 90), err);
    }
    ui.horizontal(|ui| {
        if ui.button("📂 资源管理器").clicked() {
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
            .add_enabled(app.ide.confirm_del, egui::Button::new("🗑 删除文件"))
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

/// 素材工具：精灵表导入向导 + 工程导出副本
fn draw_asset_tools(ui: &mut egui::Ui, app: &mut GameApp) {
    ui.label(RichText::new("素材工具").strong());
    egui::CollapsingHeader::new("📥 导入精灵表")
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
                ui.colored_label(Color32::from_rgb(150, 220, 160), m);
            }
            ui.small("PNG 会复制到工程 assets/anims/ 并写入 animations.ron（热重载立即生效）");
        });
    if ui.button("📦 导出工程副本").clicked() {
        export_project(app);
    }
    if let Some(m) = &app.ide.export_msg {
        ui.colored_label(Color32::from_rgb(150, 220, 160), m);
    }
}

/// 精灵表导入：复制 PNG → 计算帧数 → 写入 animations.ron
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
    // 复制到工程/内置 anims 目录
    let dir = project::dir_of("anims");
    let _ = std::fs::create_dir_all(&dir);
    let file = format!("{name}.png");
    if let Err(e) = std::fs::copy(&src, dir.join(&file)) {
        app.ide.err = Some(format!("复制失败: {e}"));
        return;
    }
    // 写入 animations.ron
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

/// 导出工程副本到 exports/<name>/
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
            RichText::new(sel.file_name().and_then(|n| n.to_str()).unwrap_or("?")).strong(),
        );
        if is_text(&sel) {
            if app.ide.dirty {
                ui.colored_label(Color32::from_rgb(240, 200, 90), "已修改未保存");
            }
            if ui.button("💾 保存 (Ctrl+S)").clicked() {
                save_text(app);
            }
            if ui.button("↺ 重新加载").clicked() {
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
        egui::ScrollArea::both().show(ui, |ui| {
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
                ui.monospace(format!("PNG {}×{}", w, h));
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
                ui.colored_label(Color32::from_rgb(230, 120, 90), format!("无法读取: {e}"));
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
