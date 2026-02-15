use nih_plug::prelude::*;

use crate::params::SongWalkerParams;
use crate::perf::pool::MixBuffer;
use crate::slots::SlotManager;
use crate::transport::TransportState;

/// Maximum number of samples in a single process block.
pub const MAX_BLOCK_SIZE: usize = 8192;

/// Pre-allocated audio engine resources.
///
/// All buffers are allocated at `initialize()` time.
/// Nothing is allocated during `process()`.
pub struct AudioEngine {
    /// Scratch buffer for per-slot rendering (stereo interleaved).
    slot_buffer: MixBuffer,
    /// Current sample rate.
    sample_rate: f32,
    /// Max buffer size from the host.
    max_buffer_size: usize,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            slot_buffer: MixBuffer::new(MAX_BLOCK_SIZE),
            sample_rate: 44100.0,
            max_buffer_size: MAX_BLOCK_SIZE,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32, max_buffer_size: usize) {
        self.sample_rate = sample_rate;
        self.max_buffer_size = max_buffer_size;
        self.slot_buffer = MixBuffer::new(max_buffer_size);
    }

    pub fn reset(&mut self) {
        self.slot_buffer.clear();
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

/// Main audio processing entry point. Called once per process block.
///
/// This function:
/// 1. Drains MIDI events from the host and routes them to slots
/// 2. Renders each slot into a scratch buffer
/// 3. Mixes all slot outputs into the final output buffer
/// 4. Applies master volume and pan
pub fn process_block(
    buffer: &mut Buffer,
    context: &mut impl ProcessContext<crate::SongWalkerPlugin>,
    engine: &mut AudioEngine,
    slot_manager: &mut SlotManager,
    transport: &TransportState,
    params: &SongWalkerParams,
) {
    let num_samples = buffer.samples();
    let sample_rate = engine.sample_rate;

    // --- 1. Collect and route MIDI events ---
    while let Some(event) = context.next_event() {
        crate::midi::route_event(&event, slot_manager, transport);
    }

    // --- 2. Clear output buffer ---
    for channel in buffer.as_slice() {
        for sample in channel.iter_mut() {
            *sample = 0.0;
        }
    }

    // --- 3. Render each active slot and mix into output ---
    let any_solo = slot_manager.any_solo();

    for slot_idx in 0..slot_manager.slot_count() {
        let slot = &mut slot_manager.slots_mut()[slot_idx];

        // Skip muted slots, or non-soloed slots when solo is active
        if slot.is_muted() || (any_solo && !slot.is_solo()) {
            continue;
        }

        // Clear scratch buffer
        engine.slot_buffer.clear_n(num_samples);

        // Render slot into scratch buffer (borrow both channels at once)
        let (slot_left, slot_right) = engine.slot_buffer.channels_mut();
        slot.render(
            slot_left,
            slot_right,
            num_samples,
            sample_rate,
            transport,
        );

        // Apply slot volume and pan, then mix into output
        let slot_gain = slot.volume();
        let slot_pan = slot.pan();
        let (pan_l, pan_r) = constant_power_pan(slot_pan);

        let left_out = engine.slot_buffer.left();
        let right_out = engine.slot_buffer.right();

        let output = buffer.as_slice();
        for i in 0..num_samples {
            output[0][i] += left_out[i] * slot_gain * pan_l;
            if output.len() > 1 {
                output[1][i] += right_out[i] * slot_gain * pan_r;
            }
        }
    }

    // --- 4. Apply master volume and pan ---
    let master_gain = params.master_volume.value();
    let master_pan = params.master_pan.value();
    let (master_pan_l, master_pan_r) = constant_power_pan(master_pan);

    let output = buffer.as_slice();
    for i in 0..num_samples {
        output[0][i] *= master_gain * master_pan_l;
        if output.len() > 1 {
            output[1][i] *= master_gain * master_pan_r;
        }
    }
}

/// Constant-power pan law. Returns (left_gain, right_gain).
/// `pan` ranges from -1.0 (hard left) to 1.0 (hard right), 0.0 = center.
#[inline]
fn constant_power_pan(pan: f32) -> (f32, f32) {
    let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
    (angle.cos(), angle.sin())
}
