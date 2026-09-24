//! 固定步长累加器：60Hz 逻辑 + 可变渲染
pub struct Stepper {
    acc: f64,
    pub dt: f64,
    pub max_steps: u32,
}

impl Default for Stepper {
    fn default() -> Self {
        Self { acc: 0.0, dt: 1.0 / 60.0, max_steps: 5 }
    }
}

impl Stepper {
    pub fn new() -> Self {
        Self::default()
    }

    /// 输入真实流逝秒数，返回本应执行的逻辑步数
    pub fn step(&mut self, frame_dt: f64) -> u32 {
        self.acc += frame_dt.min(0.25);
        let mut n = 0;
        while self.acc >= self.dt && n < self.max_steps {
            self.acc -= self.dt;
            n += 1;
        }
        n
    }
}
