//! mge-sim：像素物理模拟（下降沙/液体/气体/反应）
pub mod materials;
pub mod world;
pub use materials::{MaterialDef, MaterialId, Materials, Special};
pub use world::{Pixel, PixelWorld, Rect, SimHooks, SimIds, CHUNK_PX};
