//! 简单事件通道：send 累积，drain 一次性取走
pub struct Events<T> {
    queue: Vec<T>,
}

impl<T> Events<T> {
    pub fn new() -> Self {
        Self { queue: Vec::new() }
    }

    pub fn send(&mut self, t: T) {
        self.queue.push(t);
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, T> {
        self.queue.drain(..)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl<T> Default for Events<T> {
    fn default() -> Self {
        Self::new()
    }
}
