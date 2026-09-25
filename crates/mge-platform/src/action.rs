//! 逻辑动作抽象：键位可重映射
use std::collections::HashMap;
use winit::keyboard::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    Jump,
    Attack,
    Slot1,
    Slot2,
    Slot3,
    Slot4,
    Slot5,
    Slot6,
    Slot7,
    Slot8,
    ToggleDebug,
    ToggleEditor,
    /// 系统设置菜单（ESC）
    ToggleMenu,
    Inventory,
    Potion,
    QuickSave,
    QuickLoad,
}

pub struct ActionMap {
    keys: HashMap<KeyCode, Action>,
}

impl Default for ActionMap {
    fn default() -> Self {
        Self::standard()
    }
}

impl ActionMap {
    /// 默认键位：AD 移动 / W 或空格跳 / S 下 / 鼠标左键攻击 / 1-6 快捷栏 / F3 调试
    pub fn standard() -> Self {
        let mut keys = HashMap::new();
        let mut put = |k: KeyCode, a: Action| {
            keys.insert(k, a);
        };
        put(KeyCode::KeyA, Action::Left);
        put(KeyCode::ArrowLeft, Action::Left);
        put(KeyCode::KeyD, Action::Right);
        put(KeyCode::ArrowRight, Action::Right);
        put(KeyCode::KeyW, Action::Up);
        put(KeyCode::ArrowUp, Action::Up);
        put(KeyCode::KeyS, Action::Down);
        put(KeyCode::ArrowDown, Action::Down);
        put(KeyCode::Space, Action::Jump);
        put(KeyCode::Digit1, Action::Slot1);
        put(KeyCode::Digit2, Action::Slot2);
        put(KeyCode::Digit3, Action::Slot3);
        put(KeyCode::Digit4, Action::Slot4);
        put(KeyCode::Digit5, Action::Slot5);
        put(KeyCode::Digit6, Action::Slot6);
        put(KeyCode::Digit7, Action::Slot7);
        put(KeyCode::Digit8, Action::Slot8);
        put(KeyCode::F3, Action::ToggleDebug);
        put(KeyCode::F1, Action::ToggleEditor);
        put(KeyCode::Escape, Action::ToggleMenu);
        put(KeyCode::KeyI, Action::Inventory);
        put(KeyCode::KeyQ, Action::Potion);
        put(KeyCode::F5, Action::QuickSave);
        put(KeyCode::F9, Action::QuickLoad);
        Self { keys }
    }

    pub fn key_action(&self, key: KeyCode) -> Option<Action> {
        self.keys.get(&key).copied()
    }

    pub fn key_for(&self, action: Action) -> Option<KeyCode> {
        self.keys.iter().find(|(_, v)| **v == action).map(|(k, _)| *k)
    }

    /// 重绑定：该动作换新键（移除动作旧键，覆盖目标键旧动作）
    pub fn set_binding(&mut self, key: KeyCode, action: Action) {
        self.keys.retain(|_, v| *v != action);
        self.keys.insert(key, action);
    }

    pub fn all(&self) -> impl Iterator<Item = (KeyCode, Action)> + '_ {
        self.keys.iter().map(|(k, v)| (*k, *v))
    }
}
