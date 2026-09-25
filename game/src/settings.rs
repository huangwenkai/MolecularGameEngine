//! 系统设置：主音量 / 震屏强度 / 按键配置（saves/settings.ron 持久化）
//! ESC 打开面板：操作方式说明 + 滑条设置 + 按键重绑定
use crate::audio::Audio;
use mge_platform::action::{Action, ActionMap};
use mge_platform::input::InputState;
use serde::{Deserialize, Serialize};
use winit::keyboard::KeyCode;

const PATH: &str = "saves/settings.ron";

// ---------------------------------------------------------------------------
// 设置数据
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 主音量 0~1
    pub volume: f32,
    /// 震屏强度倍率 0~2（0 = 关闭震动）
    pub shake: f32,
    /// 键位覆盖项 (动作名, 键名)
    pub bindings: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { volume: 0.8, shake: 1.0, bindings: Vec::new() }
    }
}

impl Settings {
    pub fn load() -> Self {
        match std::fs::read_to_string(PATH) {
            Ok(s) => ron::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all("saves");
        let Ok(txt) = ron::ser::to_string(self) else { return };
        let tmp = format!("{PATH}.tmp");
        if std::fs::write(&tmp, txt).is_ok() {
            let _ = std::fs::rename(&tmp, PATH);
        }
    }

    /// 应用键位覆盖到键位表（启动 / 恢复默认时调用）
    pub fn apply(&self, map: &mut ActionMap) {
        for (a, k) in &self.bindings {
            if let (Some(a), Some(k)) = (action_from_name(a), parse_key(k)) {
                map.set_binding(k, a);
            }
        }
    }

    /// 记录一次重绑定
    fn record(&mut self, a: Action, k: KeyCode) {
        let name = action_name(a).to_string();
        let key = key_name(k);
        match self.bindings.iter_mut().find(|(an, _)| *an == name) {
            Some(e) => e.1 = key,
            None => self.bindings.push((name, key)),
        }
    }
}

// ---------------------------------------------------------------------------
// 动作 ↔ 名称映射
// ---------------------------------------------------------------------------

/// 持久化用动作 id
pub fn action_name(a: Action) -> &'static str {
    match a {
        Action::Left => "left",
        Action::Right => "right",
        Action::Up => "up",
        Action::Down => "down",
        Action::Jump => "jump",
        Action::Attack => "attack",
        Action::Slot1 => "slot1",
        Action::Slot2 => "slot2",
        Action::Slot3 => "slot3",
        Action::Slot4 => "slot4",
        Action::Slot5 => "slot5",
        Action::Slot6 => "slot6",
        Action::Slot7 => "slot7",
        Action::Slot8 => "slot8",
        Action::ToggleDebug => "debug",
        Action::ToggleEditor => "editor",
        Action::ToggleMenu => "menu",
        Action::Inventory => "inventory",
        Action::Potion => "potion",
        Action::QuickSave => "save",
        Action::QuickLoad => "load",
    }
}

pub fn action_from_name(s: &str) -> Option<Action> {
    Some(match s {
        "left" => Action::Left,
        "right" => Action::Right,
        "up" => Action::Up,
        "down" => Action::Down,
        "jump" => Action::Jump,
        "attack" => Action::Attack,
        "slot1" => Action::Slot1,
        "slot2" => Action::Slot2,
        "slot3" => Action::Slot3,
        "slot4" => Action::Slot4,
        "slot5" => Action::Slot5,
        "slot6" => Action::Slot6,
        "slot7" => Action::Slot7,
        "slot8" => Action::Slot8,
        "debug" => Action::ToggleDebug,
        "editor" => Action::ToggleEditor,
        "menu" => Action::ToggleMenu,
        "inventory" => Action::Inventory,
        "potion" => Action::Potion,
        "save" => Action::QuickSave,
        "load" => Action::QuickLoad,
        _ => return None,
    })
}

/// 设置面板可重绑定的动作（展示顺序）
const REBINDABLE: &[Action] = &[
    Action::Left,
    Action::Right,
    Action::Jump,
    Action::Slot1,
    Action::Slot2,
    Action::Slot3,
    Action::Slot4,
    Action::Slot5,
    Action::Slot6,
    Action::Slot7,
    Action::Slot8,
    Action::Inventory,
    Action::Potion,
    Action::ToggleMenu,
    Action::ToggleDebug,
    Action::ToggleEditor,
    Action::QuickSave,
    Action::QuickLoad,
];

/// 动作中文名（UI 展示）
fn action_label(a: Action) -> &'static str {
    match a {
        Action::Left => "向左移动",
        Action::Right => "向右移动",
        Action::Up => "向上",
        Action::Down => "向下",
        Action::Jump => "跳跃",
        Action::Attack => "攻击 / 使用",
        Action::Slot1 => "快捷 1 · 剑",
        Action::Slot2 => "快捷 2 · 镐",
        Action::Slot3 => "快捷 3 · 石块",
        Action::Slot4 => "快捷 4 · 火把",
        Action::Slot5 => "快捷 5 · 水",
        Action::Slot6 => "快捷 6 · 沙",
        Action::Slot7 => "快捷 7 · 弓",
        Action::Slot8 => "快捷 8 · 火球",
        Action::Inventory => "背包",
        Action::Potion => "喝药水",
        Action::ToggleMenu => "系统设置",
        Action::ToggleDebug => "调试面板",
        Action::ToggleEditor => "特效编辑器",
        Action::QuickSave => "快速存档",
        Action::QuickLoad => "快速读档",
    }
}

// ---------------------------------------------------------------------------
// 键名 ↔ KeyCode
// ---------------------------------------------------------------------------

pub fn key_name(k: KeyCode) -> String {
    format!("{k:?}")
}

pub fn parse_key(s: &str) -> Option<KeyCode> {
    macro_rules! keys {
        ($($v:ident),* $(,)?) => {
            match s {
                $(stringify!($v) => Some(KeyCode::$v),)*
                _ => None,
            }
        };
    }
    keys!(
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ, Digit0, Digit1, Digit2,
        Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, Numpad0, Numpad1, Numpad2, Numpad3,
        Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, F1, F2, F3, F4, F5, F6, F7, F8, F9,
        F10, F11, F12, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Space, Escape, Enter, Tab,
        Backspace, Delete, Insert, Home, End, PageUp, PageDown, ShiftLeft, ShiftRight, ControlLeft,
        ControlRight, AltLeft, AltRight, Minus, Equal, BracketLeft, BracketRight, Semicolon, Quote,
        Backquote, Backslash, Comma, Period, Slash,
    )
}

// ---------------------------------------------------------------------------
// 设置面板（ESC）
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    /// 等待重绑定的动作（Some = 点击了按键按钮，等待玩家按新键）
    listen: Option<Action>,
}

impl SettingsUi {
    /// 绘制面板。暂停时每渲染帧调用；rebind 按键捕获也在此消费。
    pub fn draw(
        &mut self,
        settings: &mut Settings,
        audio: &mut Audio,
        input: &mut InputState,
        egui: &egui::Context,
    ) {
        let mut open = self.open;
        egui::Window::new("系统设置 (ESC)")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .default_pos([360.0, 120.0])
            .show(egui, |ui| {
                let map = input.map();

                // ---- 操作方式（随键位配置动态显示）----
                ui.heading("操作方式");
                ui.separator();
                egui::Grid::new("controls")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        let row = |ui: &mut egui::Ui, label: &str, key: &str| {
                            ui.label(label);
                            ui.monospace(if key.is_empty() { "未绑定" } else { key });
                            ui.end_row();
                        };
                        row(
                            ui,
                            "移动",
                            &fmt_pair(map, Action::Left, Action::Right),
                        );
                        row(ui, "跳跃", &key_of(map, Action::Jump));
                        row(ui, "攻击 / 使用工具（单击触发）", "鼠标左键");
                        row(
                            ui,
                            "快捷栏 1~8（剑/镐/石块/火把/水/沙/弓/火球）",
                            &fmt_pair(map, Action::Slot1, Action::Slot8),
                        );
                        row(ui, "背包", &key_of(map, Action::Inventory));
                        row(ui, "喝药水", &key_of(map, Action::Potion));
                        row(ui, "系统设置", &key_of(map, Action::ToggleMenu));
                        row(ui, "调试面板", &key_of(map, Action::ToggleDebug));
                        row(ui, "特效编辑器", &key_of(map, Action::ToggleEditor));
                        row(ui, "快速存档 / 读档", &{
                            let s = key_of(map, Action::QuickSave);
                            let l = key_of(map, Action::QuickLoad);
                            format!("{s} / {l}")
                        });
                    });
                ui.add_space(6.0);

                // ---- 声音 ----
                ui.heading("声音");
                ui.separator();
                let vol = ui
                    .add(egui::Slider::new(&mut settings.volume, 0.0..=1.0).text("音量大小"));
                audio.set_volume(settings.volume);
                if vol.drag_stopped() {
                    settings.save();
                }
                ui.add_space(6.0);

                // ---- 震动 ----
                ui.heading("震动");
                ui.separator();
                let shk = ui.add(
                    egui::Slider::new(&mut settings.shake, 0.0..=2.0)
                        .text("震动强度（0 = 关闭）"),
                );
                if shk.drag_stopped() {
                    settings.save();
                }
                ui.add_space(6.0);

                // ---- 按键配置 ----
                ui.heading("按键配置");
                ui.separator();
                match self.listen {
                    Some(a) => {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!("为「{}」按下新按键……（ESC 取消）", action_label(a)),
                        );
                        if let Some(k) = input.take_raw_key() {
                            self.listen = None;
                            if k != KeyCode::Escape {
                                input.map_mut().set_binding(k, a);
                                settings.record(a, k);
                                settings.save();
                            }
                        }
                    }
                    None => {
                        egui::Grid::new("rebind")
                            .num_columns(4)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                let mut col = 0;
                                for &a in REBINDABLE {
                                    let name = action_label(a);
                                    let key = key_of(input.map(), a);
                                    if ui.button(format!("{name}  [{key}]")).clicked() {
                                        self.listen = Some(a);
                                    }
                                    col += 1;
                                    if col % 2 == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                        ui.add_space(4.0);
                        if ui.button("恢复默认键位").clicked() {
                            settings.bindings.clear();
                            *input.map_mut() = ActionMap::standard();
                            settings.save();
                        }
                    }
                }
            });
        self.open = open;
    }
}

fn key_of(map: &ActionMap, a: Action) -> String {
    map.key_for(a).map(key_name).unwrap_or_default()
}

fn fmt_pair(map: &ActionMap, a1: Action, a2: Action) -> String {
    format!("{} / {}", key_of(map, a1), key_of(map, a2))
}
