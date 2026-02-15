/// SIMD-accelerated audio utilities.
///
/// These provide optimized paths for common DSP operations when
/// available on the target platform. Falls back to scalar code
/// on unsupported architectures.

/// Multiply-add: dst[i] += src[i] * gain, for `n` samples.
#[inline]
pub fn mix_add(dst: &mut [f32], src: &[f32], gain: f32, n: usize) {
    let n = n.min(dst.len()).min(src.len());

    // On x86_64 with AVX, process 8 floats at a time
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx") {
            // SAFETY: feature detection guarantees instruction availability
            unsafe { mix_add_avx(dst, src, gain, n) };
            return;
        }
    }

    // Scalar fallback
    mix_add_scalar(dst, src, gain, n);
}

/// Apply gain to a buffer in-place.
#[inline]
pub fn apply_gain(buf: &mut [f32], gain: f32, n: usize) {
    let n = n.min(buf.len());

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx") {
            unsafe { apply_gain_avx(buf, gain, n) };
            return;
        }
    }

    apply_gain_scalar(buf, gain, n);
}

/// Scalar fallback for mix_add.
#[inline]
fn mix_add_scalar(dst: &mut [f32], src: &[f32], gain: f32, n: usize) {
    for i in 0..n {
        dst[i] += src[i] * gain;
    }
}

/// Scalar fallback for apply_gain.
#[inline]
fn apply_gain_scalar(buf: &mut [f32], gain: f32, n: usize) {
    for sample in buf[..n].iter_mut() {
        *sample *= gain;
    }
}

// ── AVX implementations ──

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn mix_add_avx(dst: &mut [f32], src: &[f32], gain: f32, n: usize) {
    use std::arch::x86_64::*;

    let gain_vec = _mm256_set1_ps(gain);
    let chunks = n / 8;
    let remainder = n % 8;

    for i in 0..chunks {
        let offset = i * 8;
        let s = _mm256_loadu_ps(src.as_ptr().add(offset));
        let d = _mm256_loadu_ps(dst.as_ptr().add(offset));
        let result = _mm256_add_ps(d, _mm256_mul_ps(s, gain_vec));
        _mm256_storeu_ps(dst.as_mut_ptr().add(offset), result);
    }

    // Handle remaining samples
    let start = chunks * 8;
    for i in start..(start + remainder) {
        dst[i] += src[i] * gain;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn apply_gain_avx(buf: &mut [f32], gain: f32, n: usize) {
    use std::arch::x86_64::*;

    let gain_vec = _mm256_set1_ps(gain);
    let chunks = n / 8;
    let remainder = n % 8;

    for i in 0..chunks {
        let offset = i * 8;
        let s = _mm256_loadu_ps(buf.as_ptr().add(offset));
        let result = _mm256_mul_ps(s, gain_vec);
        _mm256_storeu_ps(buf.as_mut_ptr().add(offset), result);
    }

    let start = chunks * 8;
    for sample in buf[start..(start + remainder)].iter_mut() {
        *sample *= gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mix_add() {
        let mut dst = vec![1.0_f32; 16];
        let src = vec![2.0_f32; 16];
        mix_add(&mut dst, &src, 0.5, 16);
        for v in &dst {
            assert!((v - 2.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_apply_gain() {
        let mut buf = vec![0.5_f32; 16];
        apply_gain(&mut buf, 2.0, 16);
        for v in &buf {
            assert!((v - 1.0).abs() < 1e-6);
        }
    }
}
