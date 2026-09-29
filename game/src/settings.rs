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
    /// 当前存档位 1~3（F5/F9 快存快读作用于该存档位）
    #[serde(default = "default_slot")]
    pub slot: u8,
    /// 刷怪开关（关闭后不再自然生成怪物，已有怪物保留）
    #[serde(default = "default_true")]
    pub spawn_on: bool,
    /// 时间锁定（锁定当前时刻：昼夜推进暂停）
    #[serde(default)]
    pub time_lock: bool,
    /// 拾取磁吸倍率 0~3（0 = 关闭磁吸，1 = 默认）
    #[serde(default = "default_magnet")]
    pub magnet: f32,
    /// 键位覆盖项 (动作名, 键名)
    pub bindings: Vec<(String, String)>,
}

fn default_slot() -> u8 {
    1
}

fn default_true() -> bool {
    true
}

fn default_magnet() -> f32 {
    1.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            shake: 1.0,
            slot: 1,
            spawn_on: true,
            time_lock: false,
            magnet: 1.0,
            bindings: Vec::new(),
        }
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
        Action::Slot9 => "slot9",
        Action::ToggleDebug => "debug",
        Action::ToggleEditor => "editor",
        Action::ToggleMenu => "menu",
        Action::Inventory => "inventory",
        Action::Potion => "potion",
        Action::QuickSave => "save",
        Action::QuickLoad => "load",
        Action::Skill1 => "skill1",
        Action::Skill2 => "skill2",
        Action::Skill3 => "skill3",
        Action::Skill4 => "skill4",
        Action::Skill5 => "skill5",
        Action::Skill6 => "skill6",
        Action::ToggleIde => "ide",
        Action::Dodge => "dodge",
        Action::Walk => "walk",
        Action::Pickup => "pickup",
        Action::SkillPanel => "skillpanel",
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
        "skill1" => Action::Skill1,
        "skill2" => Action::Skill2,
        "skill3" => Action::Skill3,
        "skill4" => Action::Skill4,
        "skill5" => Action::Skill5,
        "skill6" => Action::Skill6,
        "ide" => Action::ToggleIde,
        "dodge" => Action::Dodge,
        "walk" => Action::Walk,
        "pickup" => Action::Pickup,
        "skillpanel" => Action::SkillPanel,
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
    Action::Skill1,
    Action::Skill2,
    Action::Skill3,
    Action::Skill4,
    Action::Skill5,
    Action::Skill6,
    Action::ToggleIde,
    Action::Dodge,
    Action::Walk,
    Action::Pickup,
    Action::SkillPanel,
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
        Action::Slot9 => "快捷 9 · 平台",
        Action::Inventory => "背包",
        Action::Potion => "喝药水",
        Action::ToggleMenu => "系统设置",
        Action::ToggleDebug => "调试面板",
        Action::ToggleEditor => "特效编辑器",
        Action::QuickSave => "快速存档",
        Action::QuickLoad => "快速读档",
        Action::Skill1 => "技能 1 · 旋风斩",
        Action::Skill2 => "技能 2 · 火焰新星",
        Action::Skill3 => "技能 3 · 治疗术",
        Action::Skill4 => "技能 4 · 闪电术",
        Action::Skill5 => "技能 5 · 冰霜新星",
        Action::Skill6 => "技能 6 · 毒爆",
        Action::ToggleIde => "引擎 IDE（工程/素材）",
        Action::Dodge => "闪避",
        Action::Walk => "慢走（按住）",
        Action::Pickup => "拾取丢弃物",
        Action::SkillPanel => "技能面板",
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

/// 实验按钮请求（ESC 面板 → 主循环消费；面板不直接操作游戏状态）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LabReq {
    /// 生成毒抗怪物（史莱姆，毒抗 0.8）
    SpawnPoisonMob,
    /// 生成骷髅兵（毒抗 0.5 对照）
    SpawnSkeleton,
    /// 生成毒抗对照：魔化蘑菇（耐火 0.6）
    SpawnMushroom,
    /// 发一本毒法术魔法书
    GivePoisonTome,
    /// 发一本闪电魔法书
    GiveLightningTome,
    /// 发一瓶毒液瓶
    GivePoisonVial,
    /// 清空全部怪物
    ClearMonsters,
    /// 切换到单向平台工具
    SelectPlatformTool,
    /// 免费习得冰霜新星
    LearnFrostNova,
    /// 免费习得毒爆
    LearnVenomBurst,
}

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    /// 二级页面：按键设置窗口
    pub key_page: bool,
    /// 等待重绑定的动作（Some = 点击了按键按钮，等待玩家按新键）
    listen: Option<Action>,
    /// 实验按钮请求队列（主循环逐帧消费）
    pub lab_reqs: Vec<LabReq>,
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
            .default_width(720.0)
            .default_pos([320.0, 100.0])
            .show(egui, |ui| {
                // 高度限制：内容超过 540px 时面板内部滚动，窗口不再无限撑高
                egui::ScrollArea::vertical().max_height(540.0).show(ui, |ui| {
                    ui.columns(2, |cols| {
                        // ============ 左列：操作方式 + 按键配置 ============
                        let ui = &mut cols[0];
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
                        row(
                            ui,
                            "主动技能（旋风斩 / 火焰新星 / 治疗术）",
                            &fmt_pair(map, Action::Skill1, Action::Skill3),
                        );
                        row(ui, "闪电术", &key_of(map, Action::Skill4));
                        row(ui, "冰霜新星", &key_of(map, Action::Skill5));
                        row(ui, "毒爆", &key_of(map, Action::Skill6));
                        row(ui, "系统设置", &key_of(map, Action::ToggleMenu));
                        row(ui, "调试面板", &key_of(map, Action::ToggleDebug));
                        row(ui, "特效编辑器", &key_of(map, Action::ToggleEditor));
                        row(ui, "引擎 IDE", &key_of(map, Action::ToggleIde));
                        row(ui, "闪避", &key_of(map, Action::Dodge));
                        row(ui, "慢走", &key_of(map, Action::Walk));
                        row(ui, "拾取丢弃物", &key_of(map, Action::Pickup));
                        row(ui, "技能面板", &key_of(map, Action::SkillPanel));
                        row(ui, "快速存档 / 读档", &{
                            let s = key_of(map, Action::QuickSave);
                            let l = key_of(map, Action::QuickLoad);
                            format!("{s} / {l}")
                        });
                    });
                ui.add_space(8.0);

                // ---- 二级页面入口 ----
                ui.heading("按键配置");
                ui.separator();
                if ui.button("更改按键……").clicked() {
                    self.key_page = true;
                }
                ui.small("点击打开二级页面，点击要修改的项后按下新键");

                        // ============ 右列：各项配置 ============
                        let ui = &mut cols[1];

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

                // ---- 存档位 ----
                ui.heading("存档位");
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("F5/F9 作用的存档位：");
                    for s in 1..=3u8 {
                        if ui
                            .selectable_label(settings.slot == s, format!("存档 {s}"))
                            .clicked()
                        {
                            settings.slot = s;
                            settings.save();
                        }
                    }
                });
                ui.add_space(6.0);

                // ---- 世界 ----
                ui.heading("世界");
                ui.separator();
                if ui
                    .checkbox(&mut settings.spawn_on, "夜间刷怪")
                    .changed()
                {
                    settings.save();
                }
                if ui
                    .checkbox(&mut settings.time_lock, "时间锁定（暂停昼夜推进）")
                    .changed()
                {
                    settings.save();
                }
                let mag = ui
                    .add(
                        egui::Slider::new(&mut settings.magnet, 0.0..=3.0)
                            .text("拾取磁吸（0 = 关闭）"),
                    )
                    .drag_stopped();
                if mag {
                    settings.save();
                }
                ui.add_space(6.0);

                // ---- 实验（新功能测试按钮）----
                ui.heading("实验（功能测试）");
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("🧪 生成毒抗怪物").clicked() {
                        self.lab_reqs.push(LabReq::SpawnPoisonMob);
                    }
                    if ui.button("💀 生成骷髅兵(毒抗0.5)").clicked() {
                        self.lab_reqs.push(LabReq::SpawnSkeleton);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("🍄 生成蘑菇(耐火)").clicked() {
                        self.lab_reqs.push(LabReq::SpawnMushroom);
                    }
                    if ui.button("🧹 清空怪物").clicked() {
                        self.lab_reqs.push(LabReq::ClearMonsters);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("📕 毒法术魔法书").clicked() {
                        self.lab_reqs.push(LabReq::GivePoisonTome);
                    }
                    if ui.button("⚡ 闪电魔法书").clicked() {
                        self.lab_reqs.push(LabReq::GiveLightningTome);
                    }
                });
                if ui.button("🧴 毒液瓶(淬毒材料)").clicked() {
                    self.lab_reqs.push(LabReq::GivePoisonVial);
                }
                if ui.button("🪜 单向平台工具").clicked() {
                    self.lab_reqs.push(LabReq::SelectPlatformTool);
                }
                ui.horizontal(|ui| {
                    if ui.button("❄️ 学习冰霜新星").clicked() {
                        self.lab_reqs.push(LabReq::LearnFrostNova);
                    }
                    if ui.button("🟢 学习毒爆").clicked() {
                        self.lab_reqs.push(LabReq::LearnVenomBurst);
                    }
                });
                ui.small("新功能在此配测试按钮：毒抗怪物/毒法书/毒液瓶/单向平台/新技能，怪物生成在面前");
                ui.add_space(6.0);
                    }); // columns
                }); // ScrollArea
            });
        self.open = open;

        // ---- 二级页面：按键设置 ----
        if self.key_page {
            let mut kp = self.key_page;
            egui::Window::new("按键设置")
                .open(&mut kp)
                .collapsible(false)
                .default_width(420.0)
                .default_pos([520.0, 140.0])
                .show(egui, |ui| {
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
                            for &a in REBINDABLE {
                                let name = action_label(a);
                                let key = key_of(input.map(), a);
                                if ui.button(format!("{name}  [{key}]")).clicked() {
                                    self.listen = Some(a);
                                }
                            }
                            ui.separator();
                            if ui.button("恢复默认键位").clicked() {
                                settings.bindings.clear();
                                *input.map_mut() = ActionMap::standard();
                                settings.save();
                            }
                        }
                    }
                });
            self.key_page = kp;
        }
    }
}

fn key_of(map: &ActionMap, a: Action) -> String {
    map.key_for(a).map(key_name).unwrap_or_default()
}

fn fmt_pair(map: &ActionMap, a1: Action, a2: Action) -> String {
    format!("{} / {}", key_of(map, a1), key_of(map, a2))
}
