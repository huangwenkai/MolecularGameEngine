//! 引擎 IDE（F4）：左中右三栏布局 —— 左=工程与素材树，中=内容编辑，右=检查器
//!
//! 第一期（素材管理）：工程创建/打开/最近列表、素材文件树、RON/WGSL 文本编辑保存、
//! PNG 信息查看、新建/删除文件。保存后由既有热重载链路立即生效。
use crate::project;
use crate::GameApp;
use egui::{Color32, RichText};
use std::path::PathBuf;

pub struct Ide {
    pub open: bool,
    /// 当前选中的文件
    pub sel: Option<PathBuf>,
    /// 文本缓冲区（对应 sel）
    pub text: String,
    /// 文本缓冲区对应的文件（用于判断是否需要重新加载）
    pub text_of: Option<PathBuf>,
    pub dirty: bool,
    pub err: Option<String>,
    /// 检查器：新建文件名
    pub new_file: String,
    /// 删除确认
    pub confirm_del: bool,
}

impl Default for Ide {
    fn default() -> Self {
        Self {
            open: false,
            sel: None,
            text: String::new(),
            text_of: None,
            dirty: false,
            err: None,
            new_file: String::new(),
            confirm_del: false,
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

    // ---------------- 左：工程 + 素材树 ----------------
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
            egui::ScrollArea::vertical().show(ui, |ui| {
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
                                    app.ide.text_of = None; // 触发重新加载
                                    app.ide.dirty = false;
                                    app.ide.confirm_del = false;
                                }
                            }
                        });
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
            let Some(sel) = app.ide.sel.clone() else {
                ui.weak("未选中文件");
                return;
            };
            let name = sel.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
            ui.label(RichText::new(&name).strong());
            ui.monospace(sel.display().to_string());
            if let Ok(md) = std::fs::metadata(&sel) {
                ui.monospace(format!("大小 {} 字节", md.len()));
                if let Ok(t) = md.modified() {
                    if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                        ui.monospace(format!("修改于 {} 秒前(epoch)", d.as_secs() % 100000));
                    }
                }
            }
            if is_text(&sel) {
                ui.monospace(format!("行数 {}", app.ide.text.lines().count()));
            } else if sel.extension().and_then(|e| e.to_str()) == Some("png") {
                if let Ok(img) = image::open(&sel) {
                    ui.monospace(format!(
                        "图像 {}×{}",
                        img.width(),
                        img.height()
                    ));
                }
            }
            ui.separator();

            // ---- 操作 ----
            ui.label(RichText::new("操作").strong());
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
            // 新建文件（同目录）
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
            ui.separator();
            ui.small("提示：RON/WGSL 修改并保存后，游戏内热重载会立即生效（无需重启）。");
        });

    // ---------------- 中：内容区 ----------------
    egui::CentralPanel::default().show(ctx, |ui| {
        let Some(sel) = app.ide.sel.clone() else {
            ui.centered_and_justified(|ui| {
                ui.heading("从左侧素材树选择文件");
            });
            return;
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(
                    sel.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                )
                .strong(),
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
            // 首次选中/切换文件时载入
            if app.ide.text_of.as_deref() != Some(sel.as_path()) {
                app.ide.text = std::fs::read_to_string(&sel).unwrap_or_default();
                app.ide.text_of = Some(sel.clone());
                app.ide.dirty = false;
            }
            if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
                save_text(app);
            }
            egui::ScrollArea::both().show(ui, |ui| {
                ui.add_sized_text_edit(
                    ui.available_size(),
                    &mut app.ide.text,
                );
                if app.ide.text.chars().count() > 0 {
                    app.ide.dirty = true;
                }
            });
        } else if sel.extension().and_then(|e| e.to_str()) == Some("png") {
            if app.ide.text_of.as_deref() != Some(sel.as_path()) {
                app.ide.text_of = Some(sel.clone());
            }
            match image::open(&sel) {
                Ok(img) => {
                    let (w, h) = (img.width(), img.height());
                    ui.monospace(format!("PNG {}×{}", w, h));
                    ui.weak("像素预览（缩放显示）");
                    // 用像素级缩放绘制缩略图（RGBA 逐像素→egui 颜色）
                    let rgba = img.to_rgba8();
                    let scale = (240.0 / w.max(1) as f32).min(6.0).max(0.5);
                    let size = egui::vec2(w as f32 * scale, h as f32 * scale);
                    let (resp, painter) =
                        ui.allocate_painter(size, egui::Sense::hover());
                    for y in 0..h.min(240) {
                        for x in 0..w.min(240) {
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
    });
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

trait SizedEdit {
    fn add_sized_text_edit(&mut self, size: egui::Vec2, text: &mut String);
}
impl SizedEdit for egui::Ui {
    fn add_sized_text_edit(&mut self, size: egui::Vec2, text: &mut String) {
        self.add_sized(
            size,
            egui::TextEdit::multiline(text).font(egui::TextStyle::Monospace),
        );
    }
}
