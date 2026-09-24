//! 输入状态：窗口事件汇入，逻辑帧查询
pub use crate::action::Action;
use crate::action::{ActionMap};
use glam::Vec2;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::PhysicalKey;

#[derive(Default)]
pub struct InputState {
    down: Vec<Action>,
    just_pressed: Vec<Action>,
    just_released: Vec<Action>,
    map: ActionMap,
    pub mouse_pos: Vec2,
    pub mouse_wheel: f32,
    mouse_down: Vec<MouseButton>,
    mouse_just: Vec<MouseButton>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- 事件汇入（窗口模式由 runtime 调用）----

    pub fn key_event(&mut self, physical: PhysicalKey, state: ElementState) {
        let PhysicalKey::Code(code) = physical else { return };
        let Some(a) = self.map.key_action(code) else { return };
        match state {
            ElementState::Pressed => {
                if !self.down.contains(&a) {
                    self.down.push(a);
                    self.just_pressed.push(a);
                }
            }
            ElementState::Released => {
                self.down.retain(|x| *x != a);
                self.just_released.push(a);
            }
        }
    }

    pub fn mouse_event(&mut self, button: MouseButton, state: ElementState) {
        match state {
            ElementState::Pressed => {
                if !self.mouse_down.contains(&button) {
                    self.mouse_down.push(button);
                    self.mouse_just.push(button);
                }
            }
            ElementState::Released => {
                self.mouse_down.retain(|b| *b != button);
            }
        }
    }

    // ---- 查询（逻辑帧内）----

    pub fn pressed(&self, a: Action) -> bool {
        self.down.contains(&a) || (a == Action::Attack && self.mouse_down.contains(&MouseButton::Left))
    }

    pub fn just_pressed(&self, a: Action) -> bool {
        self.just_pressed.contains(&a)
            || (a == Action::Attack && self.mouse_just.contains(&MouseButton::Left))
    }

    pub fn just_released(&self, a: Action) -> bool {
        self.just_released.contains(&a)
    }

    /// 逻辑帧结束：清空“刚刚按下/松开”缓存（无头模式由 selftest 调用）
    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.mouse_just.clear();
        self.mouse_wheel = 0.0;
    }

    /// 无头/测试模式：直接注入按键
    pub fn inject(&mut self, a: Action, state: ElementState) {
        match self.map.key_for(a) {
            Some(code) => self.key_event(PhysicalKey::Code(code), state),
            None => {
                // 鼠标动作直接写入
                if a == Action::Attack {
                    self.mouse_event(MouseButton::Left, state);
                }
            }
        }
    }

    pub fn map(&self) -> &ActionMap {
        &self.map
    }
}
