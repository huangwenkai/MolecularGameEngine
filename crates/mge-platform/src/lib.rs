//! mge-platform：输入、动作映射、固定步长计时
pub mod action;
pub mod input;
pub mod timer;
pub use action::{Action, ActionMap};
pub use input::InputState;
pub use timer::Stepper;
