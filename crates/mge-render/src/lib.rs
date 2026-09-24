//! mge-render：wgpu 渲染（图集、批处理、像素世界纹理、光照合成、egui UI）
pub mod atlas;
pub mod batcher;
pub mod camera;
pub mod egui;
pub mod gpu;
pub mod renderer;
pub use atlas::{AtlasBuilder, Region};
pub use batcher::{SpriteBatch, SpriteVertex};
pub use camera::Camera;
pub use egui::EguiRenderer;
pub use gpu::Gpu;
