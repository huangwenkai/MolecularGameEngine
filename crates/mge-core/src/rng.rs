//! 轻量快速随机数（xorshift64*），用于世界生成与像素模拟
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut s = Self(seed);
        if s.0 == 0 {
            s.0 = 0x9E37_79B9_7F4A_7C15;
        }
        // 预热几次避免低质量种子
        for _ in 0..4 {
            s.next_u64();
        }
        s
    }

    pub fn from_entropy() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678);
        Self::new(t ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// [0, 1)
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// [lo, hi] 闭区间整数
    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % (hi - lo + 1) as u64) as i32
    }

    /// [lo, hi)
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.f32() * (hi - lo)
    }

    /// 概率 p (0..1) 为真
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// 派生一个新随机流
    pub fn fork(&mut self) -> Rng {
        Rng::new(self.next_u64())
    }
}
