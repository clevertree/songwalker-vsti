/// Pre-allocated stereo mix buffer to avoid per-block heap allocation.
///
/// Uses split left/right channel storage for direct slice access,
/// which matches the `Slot::render(&mut [f32], &mut [f32], ...)` API.
pub struct MixBuffer {
    left_data: Vec<f32>,
    right_data: Vec<f32>,
    /// Number of sample frames.
    capacity: usize,
}

impl MixBuffer {
    /// Create a new mix buffer that can hold `capacity` sample frames.
    pub fn new(capacity: usize) -> Self {
        Self {
            left_data: vec![0.0; capacity],
            right_data: vec![0.0; capacity],
            capacity,
        }
    }

    /// Zero the entire buffer.
    pub fn clear(&mut self) {
        self.left_data.fill(0.0);
        self.right_data.fill(0.0);
    }

    /// Zero only the first `n` frames.
    pub fn clear_n(&mut self, n: usize) {
        let end = n.min(self.capacity);
        self.left_data[..end].fill(0.0);
        self.right_data[..end].fill(0.0);
    }

    /// Immutable left channel slice.
    pub fn left(&self) -> &[f32] {
        &self.left_data
    }

    /// Mutable left channel slice.
    pub fn left_mut(&mut self) -> &mut [f32] {
        &mut self.left_data
    }

    /// Immutable right channel slice.
    pub fn right(&self) -> &[f32] {
        &self.right_data
    }

    /// Mutable right channel slice.
    pub fn right_mut(&mut self) -> &mut [f32] {
        &mut self.right_data
    }

    /// Get mutable references to both channels simultaneously (avoids double borrow).
    pub fn channels_mut(&mut self) -> (&mut [f32], &mut [f32]) {
        (&mut self.left_data, &mut self.right_data)
    }

    /// Number of sample frames this buffer can hold.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Add a stereo sample pair at frame index `i`.
    #[inline(always)]
    pub fn add(&mut self, i: usize, left: f32, right: f32) {
        if i < self.capacity {
            self.left_data[i] += left;
            self.right_data[i] += right;
        }
    }

    /// Read a stereo sample pair at frame index `i`.
    #[inline(always)]
    pub fn get(&self, i: usize) -> (f32, f32) {
        if i < self.capacity {
            (self.left_data[i], self.right_data[i])
        } else {
            (0.0, 0.0)
        }
    }

    /// Set a stereo sample pair at frame index `i`.
    #[inline(always)]
    pub fn set(&mut self, i: usize, left: f32, right: f32) {
        if i < self.capacity {
            self.left_data[i] = left;
            self.right_data[i] = right;
        }
    }

    /// Mix another buffer's contents into this one (additive), for `n` frames.
    pub fn mix_from(&mut self, other: &MixBuffer, n: usize) {
        let n = n.min(self.capacity).min(other.capacity);
        for i in 0..n {
            self.left_data[i] += other.left_data[i];
            self.right_data[i] += other.right_data[i];
        }
    }

    /// Apply gain to `n` frames.
    pub fn apply_gain(&mut self, gain: f32, n: usize) {
        let n = n.min(self.capacity);
        for s in &mut self.left_data[..n] {
            *s *= gain;
        }
        for s in &mut self.right_data[..n] {
            *s *= gain;
        }
    }

    /// Apply stereo panning to `n` frames.
    ///
    /// `pan` ranges from −1.0 (full left) to +1.0 (full right).
    /// Uses constant-power panning law.
    pub fn apply_pan(&mut self, pan: f32, n: usize) {
        let pan_clamped = pan.clamp(-1.0, 1.0);
        let angle = (pan_clamped + 1.0) * std::f32::consts::FRAC_PI_4;
        let gain_l = angle.cos();
        let gain_r = angle.sin();

        let n = n.min(self.capacity);
        for s in &mut self.left_data[..n] {
            *s *= gain_l;
        }
        for s in &mut self.right_data[..n] {
            *s *= gain_r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mix_buffer_basic() {
        let mut buf = MixBuffer::new(4);
        buf.set(0, 1.0, 0.5);
        buf.set(1, -0.5, 0.25);

        assert_eq!(buf.get(0), (1.0, 0.5));
        assert_eq!(buf.get(1), (-0.5, 0.25));
        assert_eq!(buf.get(2), (0.0, 0.0));
    }

    #[test]
    fn test_mix_buffer_add() {
        let mut buf = MixBuffer::new(4);
        buf.set(0, 0.5, 0.5);
        buf.add(0, 0.25, 0.25);
        assert_eq!(buf.get(0), (0.75, 0.75));
    }

    #[test]
    fn test_mix_buffer_clear_n() {
        let mut buf = MixBuffer::new(4);
        buf.set(0, 1.0, 1.0);
        buf.set(1, 1.0, 1.0);
        buf.set(2, 1.0, 1.0);
        buf.clear_n(2);
        assert_eq!(buf.get(0), (0.0, 0.0));
        assert_eq!(buf.get(1), (0.0, 0.0));
        assert_eq!(buf.get(2), (1.0, 1.0));
    }

    #[test]
    fn test_mix_from() {
        let mut a = MixBuffer::new(4);
        let mut b = MixBuffer::new(4);
        a.set(0, 0.5, 0.5);
        b.set(0, 0.25, 0.25);
        a.mix_from(&b, 1);
        assert_eq!(a.get(0), (0.75, 0.75));
    }

    #[test]
    fn test_left_right_slices() {
        let mut buf = MixBuffer::new(4);
        buf.set(0, 0.1, 0.2);
        buf.set(1, 0.3, 0.4);
        assert!((buf.left()[0] - 0.1).abs() < 1e-6);
        assert!((buf.right()[0] - 0.2).abs() < 1e-6);
        assert!((buf.left()[1] - 0.3).abs() < 1e-6);
        assert!((buf.right()[1] - 0.4).abs() < 1e-6);
    }
}
