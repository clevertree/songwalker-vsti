//! Standalone parameter storage using atomics.
//!
//! Shared between the UI thread and audio callback without locks.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::editor::GlobalParams;

/// Atomic f32 helper — stores f32 as u32 bits for lock-free sharing.
fn load_f32(atom: &AtomicU32) -> f32 {
    f32::from_bits(atom.load(Ordering::Relaxed))
}

fn store_f32(atom: &AtomicU32, val: f32) {
    atom.store(val.to_bits(), Ordering::Relaxed);
}

fn load_i32(atom: &AtomicU32) -> i32 {
    atom.load(Ordering::Relaxed) as i32
}

fn store_i32(atom: &AtomicU32, val: i32) {
    atom.store(val as u32, Ordering::Relaxed);
}

/// Standalone parameter storage — uses atomics for lock-free audio thread access.
#[derive(Clone)]
pub struct StandaloneParams {
    pub master_volume: Arc<AtomicU32>,
    pub master_pan: Arc<AtomicU32>,
    pub max_voices: Arc<AtomicU32>,
    pub pitch_bend_range: Arc<AtomicU32>,
}

impl Default for StandaloneParams {
    fn default() -> Self {
        Self {
            master_volume: Arc::new(AtomicU32::new(1.0_f32.to_bits())),  // 0 dB
            master_pan: Arc::new(AtomicU32::new(0.0_f32.to_bits())),     // center
            max_voices: Arc::new(AtomicU32::new(256)),
            pitch_bend_range: Arc::new(AtomicU32::new(2)),
        }
    }
}

impl StandaloneParams {
    /// Read master volume as gain (linear, not dB).
    pub fn master_volume_gain_value(&self) -> f32 {
        load_f32(&self.master_volume)
    }

    /// Read master pan value (-1..+1).
    pub fn master_pan_value(&self) -> f32 {
        load_f32(&self.master_pan)
    }
}

/// GlobalParams implementation for the standalone UI.
/// Wraps a reference to StandaloneParams.
pub struct StandaloneGlobalParams<'a> {
    pub params: &'a StandaloneParams,
}

impl GlobalParams for StandaloneGlobalParams<'_> {
    fn master_volume_gain(&self) -> f32 {
        load_f32(&self.params.master_volume)
    }
    fn set_master_volume_gain(&self, gain: f32) {
        store_f32(&self.params.master_volume, gain);
    }
    fn max_voices(&self) -> i32 {
        load_i32(&self.params.max_voices)
    }
    fn set_max_voices(&self, v: i32) {
        store_i32(&self.params.max_voices, v);
    }
    fn pitch_bend_range(&self) -> i32 {
        load_i32(&self.params.pitch_bend_range)
    }
    fn set_pitch_bend_range(&self, v: i32) {
        store_i32(&self.params.pitch_bend_range, v);
    }
}
