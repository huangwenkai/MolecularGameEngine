//! mge-world：像素世界、程序化生成、光照、昼夜（地形即像素）
pub mod gen;
pub mod light;
pub mod world;
pub use light::{LightMap, LIGHT_CELL};
pub use world::{LightUpload, World, WorldEvent};
