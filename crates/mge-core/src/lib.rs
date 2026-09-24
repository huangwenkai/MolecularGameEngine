//! mge-core：数学、随机数、事件、日志等基础能力
pub mod events;
pub mod logging;
pub mod math;
pub mod rng;

/// Tile 结构层已并入像素层（Noita 式 1 像素粒度地形），此常量保留兼容旧代码（=1）
pub const TILE_PX: i32 = 1;
/// 固定逻辑帧率
pub const LOGIC_FPS: u32 = 60;
