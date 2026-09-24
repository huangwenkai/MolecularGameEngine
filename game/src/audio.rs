//! 音频：kira 程序化合成音效（无外部资源），headless 自动降级为静音
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{AudioManager, AudioManagerSettings, DefaultBackend};
use std::collections::HashMap;

const SR: u32 = 44100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    Jump,
    Swing,
    Hit,
    Crit,
    Mine,
    Place,
    Pickup,
    Hurt,
    LevelUp,
    Explode,
    Shoot,
}

pub struct Audio {
    manager: Option<AudioManager>,
    sounds: HashMap<Sfx, StaticSoundData>,
    handles: Vec<StaticSoundHandle>,
    /// 主音量 0~1
    pub volume: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    /// 初始化（设备不可用/无头模式 → 静音）
    pub fn new() -> Self {
        let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!("音频设备不可用，静音运行: {e}");
                None
            }
        };
        let mut sounds = HashMap::new();
        if manager.is_some() {
            sounds.insert(Sfx::Jump, tone(0.12, |t| 300.0 + 2600.0 * t, wave_sine, 0.35));
            sounds.insert(Sfx::Swing, tone(0.09, |t| 700.0 - 2600.0 * t, wave_saw, 0.18));
            sounds.insert(Sfx::Hit, tone(0.08, |t| 190.0 + 400.0 * t, wave_square, 0.4));
            sounds.insert(Sfx::Crit, tone(0.14, |t| 320.0 + 900.0 * t, wave_square, 0.45));
            sounds.insert(Sfx::Mine, noise(0.06, 0.3));
            sounds.insert(Sfx::Place, tone(0.05, |_t| 420.0, wave_square, 0.25));
            sounds.insert(Sfx::Pickup, tone(0.1, |t| 600.0 + 1800.0 * t, wave_sine, 0.3));
            sounds.insert(Sfx::Hurt, tone(0.16, |t| 500.0 - 3000.0 * t, wave_saw, 0.35));
            sounds.insert(
                Sfx::LevelUp,
                tone(0.35, |t| 520.0 + 2400.0 * t * t * 3.0, wave_sine, 0.4),
            );
            sounds.insert(Sfx::Explode, noise(0.4, 0.55));
            sounds.insert(Sfx::Shoot, tone(0.07, |_t| 900.0, wave_saw, 0.2));
        }
        Self { manager, sounds, handles: Vec::new(), volume: 1.0 }
    }

    /// 播放音效（静音模式 no-op）
    pub fn play(&mut self, sfx: Sfx) {
        let Some(m) = &mut self.manager else { return };
        let Some(data) = self.sounds.get(&sfx) else { return };
        // 同音效限制并发，避免刷屏爆音
        if self.handles.len() > 12 {
            self.handles.clear();
        }
        let mut data = data.clone();
        if self.volume < 1.0 {
            data = data.volume(self.volume);
        }
        if let Ok(h) = m.play(data) {
            self.handles.push(h);
        }
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
    }
}

// ---- 波形生成 ----

type FreqFn = fn(f32) -> f32;

fn wave_sine(ph: f32) -> f32 {
    (ph * std::f32::consts::TAU).sin()
}
fn wave_square(ph: f32) -> f32 {
    if ph.fract() < 0.5 {
        0.6
    } else {
        -0.6
    }
}
fn wave_saw(ph: f32) -> f32 {
    2.0 * (ph.fract()) - 1.0
}

/// 扫频音（freq: 归一化时间 t∈0..1 → 瞬时频率），指数衰减包络
fn tone(dur: f32, freq: FreqFn, wave: fn(f32) -> f32, amp: f32) -> StaticSoundData {
    let n = (dur * SR as f32) as usize;
    let mut samples = Vec::with_capacity(n);
    let mut ph = 0.0f32;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let f = freq(t);
        ph += f / SR as f32;
        let env = (-4.0 * t).exp(); // 指数衰减
        samples.push(wave(ph) * amp * env);
    }
    to_sound_data(samples)
}

/// 白噪声 + 低通（指数平滑），衰减包络
fn noise(dur: f32, amp: f32) -> StaticSoundData {
    let n = (dur * SR as f32) as usize;
    let mut samples = Vec::with_capacity(n);
    let mut state = 0.0f32;
    let mut seed = 0x2545F4914F6CDD1Du64;
    for i in 0..n {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = ((seed >> 33) as f32 / (u32::MAX >> 1) as f32) - 1.0;
        state += (r - state) * 0.12; // 低通
        let t = i as f32 / n as f32;
        let env = (-5.0 * t).exp();
        samples.push(state * amp * env);
    }
    to_sound_data(samples)
}

fn to_sound_data(samples: Vec<f32>) -> StaticSoundData {
    // kira 0.12：直接构造（单声道 → 双声道同值）
    let frames: Vec<kira::Frame> =
        samples.into_iter().map(|v| kira::Frame { left: v, right: v }).collect();
    StaticSoundData {
        sample_rate: SR,
        frames: frames.into(),
        settings: Default::default(),
        slice: None,
    }
}
