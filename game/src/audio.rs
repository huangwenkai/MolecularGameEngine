//! 音频：kira 程序化合成音效 + 程序化 BGM（昼夜双曲，无外部资源），headless 自动降级为静音
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::PlaybackState;
use kira::{AudioManager, AudioManagerSettings};
use std::collections::HashMap;

const SR: u32 = 44100;
/// BGM 采样率（减半省内存，听感无差）
const BGM_SR: u32 = 22050;

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
    bgm_handle: Option<StaticSoundHandle>,
    bgm_is_night: bool,
    bgm_day: Option<StaticSoundData>,
    bgm_night: Option<StaticSoundData>,
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
        let manager = AudioManager::new(AudioManagerSettings::default()).ok();
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
        let has_audio = manager.is_some();
        Self {
            manager,
            sounds,
            handles: Vec::new(),
            bgm_handle: None,
            bgm_is_night: false,
            bgm_day: has_audio.then(bgm_day),
            bgm_night: has_audio.then(bgm_night),
            volume: 1.0,
        }
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
        // 正在播放的 BGM 实时跟随音量
        if let Some(h) = &mut self.bgm_handle {
            let _ = h.set_volume(self.volume * 0.45, kira::Tween::default());
        }
    }

    /// BGM 调度：每 tick 调用；曲子播完或昼夜切换时换曲
    pub fn tick(&mut self, night: bool) {
        let Some(m) = &mut self.manager else { return };
        let done = self
            .bgm_handle
            .as_ref()
            .map(|h| h.state() == PlaybackState::Stopped)
            .unwrap_or(true);
        if !done && night == self.bgm_is_night {
            return;
        }
        let data = if night { &self.bgm_night } else { &self.bgm_day };
        let Some(data) = data else { return };
        let d = data.clone().volume(0.45 * self.volume);
        if let Ok(h) = m.play(d) {
            self.bgm_handle = Some(h);
            self.bgm_is_night = night;
        }
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

// ---- 程序化 BGM ----

struct Note {
    t: f32,
    dur: f32,
    freq: f32,
    amp: f32,
    wave: fn(f32) -> f32,
}

/// 渲染音符序列为循环曲（末尾做交叉淡化保证循环无缝）
fn render_notes(notes: &[Note], total: f32) -> StaticSoundData {
    let n = (total * BGM_SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for nt in notes {
        let s0 = (nt.t * BGM_SR as f32) as usize;
        let len = (nt.dur * BGM_SR as f32) as usize;
        let mut ph = 0.0f32;
        for i in 0..len {
            let t = i as f32 / BGM_SR as f32;
            // AD 包络：快起缓落
            let env = (t / 0.015).min(1.0) * ((nt.dur - t) / 0.1).clamp(0.0, 1.0);
            ph += nt.freq / BGM_SR as f32;
            let v = (nt.wave)(ph) * nt.amp * env;
            let idx = s0 + i;
            if idx < buf.len() {
                buf[idx] += v;
            } else if idx < buf.len() + BGM_SR as usize {
                // 循环回卷叠加（无缝衔接）
                let li = idx - buf.len();
                buf[li] += v;
            }
        }
    }
    // 软限幅
    let frames: Vec<kira::Frame> = buf
        .into_iter()
        .map(|v| {
            let v = v.tanh();
            kira::Frame { left: v, right: v * 0.95 }
        })
        .collect();
    StaticSoundData {
        sample_rate: BGM_SR,
        frames: frames.into(),
        settings: Default::default(),
        slice: None,
    }
}

/// 白天曲：C 大调 20s 循环（贝斯长音 + 琶音，I-V-vi-IV ×2）
fn bgm_day() -> StaticSoundData {
    let chords: [[f32; 3]; 4] = [
        [261.63, 329.63, 392.0], // C
        [196.0, 246.94, 293.66], // G
        [220.0, 261.63, 329.63], // Am
        [174.61, 220.0, 261.63], // F
    ];
    let bass = [65.41f32, 49.0, 55.0, 43.65];
    let mut notes = Vec::new();
    let per = 2.5f32; // 每和弦时长
    for rep in 0..2 {
        for (ci, ch) in chords.iter().enumerate() {
            let t0 = (rep * 4 + ci) as f32 * per;
            // 贝斯长音（正弦低八度）
            notes.push(Note { t: t0, dur: per * 0.98, freq: bass[ci], amp: 0.16, wave: wave_sine });
            // 琶音 8 分音符：根-五-三-八度循环
            let pat = [0usize, 2, 1, 2, 0, 2, 1, 2];
            let oct = [1.0f32, 1.0, 1.0, 2.0];
            for i in 0..8 {
                let f = ch[pat[i]] * oct[i % 4];
                notes.push(Note {
                    t: t0 + i as f32 * per / 8.0,
                    dur: per / 8.0 * 0.9,
                    freq: f,
                    amp: 0.085,
                    wave: wave_sine,
                });
            }
        }
    }
    render_notes(&notes, per * 8.0)
}

/// 夜晚曲：A 小调 20s 循环（稀疏慢琶音 + 低音，末尾夜风）
fn bgm_night() -> StaticSoundData {
    let chords: [[f32; 3]; 4] = [
        [220.0, 261.63, 329.63], // Am
        [174.61, 220.0, 261.63], // F
        [261.63, 329.63, 392.0], // C
        [164.81, 207.65, 246.94], // Em
    ];
    let bass = [55.0f32, 43.65, 65.41, 41.2];
    let mut notes = Vec::new();
    let per = 2.5f32;
    for rep in 0..2 {
        for (ci, ch) in chords.iter().enumerate() {
            let t0 = (rep * 4 + ci) as f32 * per;
            notes.push(Note { t: t0, dur: per * 0.98, freq: bass[ci], amp: 0.13, wave: wave_sine });
            // 慢琶音 4 音
            for (i, &k) in [0usize, 2, 1, 2].iter().enumerate() {
                notes.push(Note {
                    t: t0 + i as f32 * per / 4.0,
                    dur: per / 4.0 * 0.85,
                    freq: ch[k] * 0.5,
                    amp: 0.07,
                    wave: wave_sine,
                });
            }
        }
    }
    // 夜风：整段低电平低通噪声由 render_notes 外叠加——简化为低频正弦慢摆动
    let mut wind = Vec::new();
    for i in 0..40 {
        wind.push(Note {
            t: i as f32 * 0.5,
            dur: 1.0,
            freq: 55.0 + (i as f32 * 0.7).sin() * 12.0,
            amp: 0.02,
            wave: wave_sine,
        });
    }
    notes.extend(wind);
    render_notes(&notes, per * 8.0)
}
