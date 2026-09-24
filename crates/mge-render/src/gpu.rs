//! GPU 上下文创建（窗口 / 无头两种模式）
use wgpu;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
}

pub fn create_instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor::default())
}

/// 创建适配器与设备；surface 可为 None（无头模式）
pub fn create_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> (Gpu, wgpu::Adapter) {
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: surface,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found");

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("mge-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .expect("Failed to create GPU device");

    let format = match surface {
        Some(s) => {
            let caps = s.get_capabilities(&adapter);
            // 优先非 sRGB 的 8bit 格式（颜色由光照合成阶段统一处理）
            caps.formats
                .iter()
                .copied()
                .find(|f| matches!(f, wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm))
                .unwrap_or(caps.formats[0])
        }
        None => wgpu::TextureFormat::Rgba8Unorm,
    };

    (Gpu { device, queue, format }, adapter)
}
