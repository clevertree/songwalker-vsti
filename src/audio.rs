use nih_plug::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::editor::visualizer::VisualizerState;
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
    /// Master output buffers — filled by render_and_mix(), read by callers.
    pub output_left: Vec<f32>,
    pub output_right: Vec<f32>,
    /// Current sample rate.
    sample_rate: f32,
    /// Max buffer size from the host.
    max_buffer_size: usize,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            slot_buffer: MixBuffer::new(MAX_BLOCK_SIZE),
            output_left: vec![0.0; MAX_BLOCK_SIZE],
            output_right: vec![0.0; MAX_BLOCK_SIZE],
            sample_rate: 44100.0,
            max_buffer_size: MAX_BLOCK_SIZE,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32, max_buffer_size: usize) {
        self.sample_rate = sample_rate;
        self.max_buffer_size = max_buffer_size;
        self.slot_buffer = MixBuffer::new(max_buffer_size);
        self.output_left.resize(max_buffer_size, 0.0);
        self.output_right.resize(max_buffer_size, 0.0);
    }

    pub fn reset(&mut self) {
        self.slot_buffer.clear();
        self.output_left.fill(0.0);
        self.output_right.fill(0.0);
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn max_buffer_size(&self) -> usize {
        self.max_buffer_size
    }
}

/// Main audio processing entry point. Called once per process block.
///
/// This function:
/// 1. Drains MIDI events from the host and routes them to slots
/// 2. Calls render_and_mix to render all slots and produce final output
pub fn process_block(
    buffer: &mut Buffer,
    context: &mut impl ProcessContext<crate::SongWalkerPlugin>,
    engine: &mut AudioEngine,
    slot_manager: &mut SlotManager,
    transport: &TransportState,
    params: &SongWalkerParams,
    visualizer_state: &Arc<VisualizerState>,
    voice_count: &Arc<AtomicU32>,
) {
    let num_samples = buffer.samples();
    if num_samples == 0 {
        return;
    }

    // Safety: ensure we never process more samples than our pre-allocated buffers.
    let num_samples = num_samples.min(engine.slot_buffer.capacity());
    if num_samples == 0 {
        return;
    }

    // --- 1. Collect and route MIDI events ---
    while let Some(event) = context.next_event() {
        crate::midi::route_event(&event, slot_manager, transport);
    }

    // --- 2. Render and mix into output buffer ---
    let master_gain = params.master_volume.value();
    let master_pan = params.master_pan.value();
    render_and_mix(
        num_samples, engine, slot_manager, transport,
        master_gain, master_pan, visualizer_state, voice_count,
    );

    // --- 3. Copy rendered audio to host buffer ---
    let output = buffer.as_slice();
    for i in 0..num_samples {
        output[0][i] = engine.output_left[i];
        if output.len() > 1 {
            output[1][i] = engine.output_right[i];
        }
    }
}

/// Core render-and-mix function used by both the plugin and standalone audio backends.
///
/// Renders all active slots into the engine's internal output buffers,
/// applies master volume/pan, feeds the visualizer, and updates the voice count.
/// After calling this, read the result from `engine.output_left` / `engine.output_right`.
pub fn render_and_mix(
    num_samples: usize,
    engine: &mut AudioEngine,
    slot_manager: &mut SlotManager,
    transport: &TransportState,
    master_gain: f32,
    master_pan: f32,
    visualizer_state: &Arc<VisualizerState>,
    voice_count: &Arc<AtomicU32>,
) {
    let num_samples = num_samples.min(engine.slot_buffer.capacity());
    if num_samples == 0 {
        return;
    }

    let sample_rate = engine.sample_rate;

    // --- 1. Clear output buffers ---
    engine.output_left[..num_samples].fill(0.0);
    engine.output_right[..num_samples].fill(0.0);

    // --- 2. Render each active slot and mix into output ---
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

        for i in 0..num_samples {
            engine.output_left[i] += left_out[i] * slot_gain * pan_l;
            engine.output_right[i] += right_out[i] * slot_gain * pan_r;
        }
    }

    // --- 3. Apply master volume and pan ---
    let (master_pan_l, master_pan_r) = constant_power_pan(master_pan);

    for i in 0..num_samples {
        engine.output_left[i] *= master_gain * master_pan_l;
        engine.output_right[i] *= master_gain * master_pan_r;
    }

    // --- 4. Feed visualizer levels and ring buffer (lock-free) ---
    {
        let mut peak_l = 0.0_f32;
        let mut peak_r = 0.0_f32;
        let mut sum_sq_l = 0.0_f64;
        let mut sum_sq_r = 0.0_f64;

        for i in 0..num_samples {
            let l = engine.output_left[i];
            let r = engine.output_right[i];
            peak_l = peak_l.max(l.abs());
            peak_r = peak_r.max(r.abs());
            sum_sq_l += (l as f64) * (l as f64);
            sum_sq_r += (r as f64) * (r as f64);
        }

        let rms_l = (sum_sq_l / num_samples as f64).sqrt() as f32;
        let rms_r = (sum_sq_r / num_samples as f64).sqrt() as f32;
        
        // Always succeeds (lock-free atomics)
        visualizer_state.update_levels(peak_l, peak_r, rms_l, rms_r);

        // Waveform uses try_lock internally, may skip if UI holds lock
        let step = (num_samples / 64).max(1);
        for i in (0..num_samples).step_by(step) {
            visualizer_state.try_push(engine.output_left[i], engine.output_right[i]);
        }
    }

    // --- 5. Update live voice count ---
    let total_voices: usize = (0..slot_manager.slot_count())
        .map(|i| slot_manager.slots()[i].active_voice_count())
        .sum();
    voice_count.store(total_voices as u32, Ordering::Relaxed);
}

/// Constant-power pan law. Returns (left_gain, right_gain).
/// `pan` ranges from -1.0 (hard left) to 1.0 (hard right), 0.0 = center.
#[inline]
pub fn constant_power_pan(pan: f32) -> (f32, f32) {
    let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
    (angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_power_pan_center() {
        let (l, r) = constant_power_pan(0.0);
        // At center, both channels should be equal (~0.707 = 1/sqrt(2))
        assert!((l - r).abs() < 0.001, "Center pan: L={} R={} should be equal", l, r);
        assert!((l - 0.707).abs() < 0.01, "Center pan: L={} should be ~0.707", l);
    }

    #[test]
    fn test_constant_power_pan_hard_left() {
        let (l, r) = constant_power_pan(-1.0);
        assert!((l - 1.0).abs() < 0.001, "Hard left: L={} should be ~1.0", l);
        assert!(r.abs() < 0.001, "Hard left: R={} should be ~0.0", r);
    }

    #[test]
    fn test_constant_power_pan_hard_right() {
        let (l, r) = constant_power_pan(1.0);
        assert!(l.abs() < 0.001, "Hard right: L={} should be ~0.0", l);
        assert!((r - 1.0).abs() < 0.001, "Hard right: R={} should be ~1.0", r);
    }

    #[test]
    fn test_constant_power_pan_symmetry() {
        // pan(x).L should equal pan(-x).R
        for &p in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            let (l_pos, r_pos) = constant_power_pan(p);
            let (l_neg, r_neg) = constant_power_pan(-p);
            assert!(
                (l_pos - r_neg).abs() < 0.001,
                "Symmetry failed at pan={}: L({})={}, L({})=R={}",
                p, p, l_pos, -p, r_neg
            );
            assert!(
                (r_pos - l_neg).abs() < 0.001,
                "Symmetry failed at pan={}: R({})={}, R({})=L={}",
                p, p, r_pos, -p, l_neg
            );
        }
    }

    #[test]
    fn test_constant_power_pan_unity_gain() {
        // L² + R² should be approximately 1.0 at all positions (constant power)
        for &p in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
            let (l, r) = constant_power_pan(p);
            let power = l * l + r * r;
            assert!(
                (power - 1.0).abs() < 0.01,
                "Power at pan={}: L²+R²={} should be ~1.0",
                p, power
            );
        }
    }

    #[test]
    fn test_audio_engine_new() {
        let engine = AudioEngine::new();
        assert_eq!(engine.sample_rate(), 44100.0);
    }

    #[test]
    fn test_audio_engine_initialize() {
        let mut engine = AudioEngine::new();
        engine.initialize(48000.0, 1024);
        assert_eq!(engine.sample_rate(), 48000.0);
    }

    // ── Visualizer Integration ──────────────────────────────────

    #[test]
    fn test_visualizer_receives_audio_levels() {
        // Simulate what process_block() does: render audio, then feed visualizer
        use crate::editor::visualizer::VisualizerState;

        let vis = Arc::new(VisualizerState::new(64));

        // Simulate a block of audio with known signal level
        let num_samples = 128;
        let amplitude = 0.5_f32;
        let fake_output_l: Vec<f32> = vec![amplitude; num_samples];
        let fake_output_r: Vec<f32> = vec![-amplitude; num_samples];

        // Feed visualizer (same logic as in render_and_mix step 4)
        {
            let mut peak_l = 0.0_f32;
            let mut peak_r = 0.0_f32;
            let mut sum_sq_l = 0.0_f64;
            let mut sum_sq_r = 0.0_f64;

            for i in 0..num_samples {
                let l = fake_output_l[i];
                let r = fake_output_r[i];
                peak_l = peak_l.max(l.abs());
                peak_r = peak_r.max(r.abs());
                sum_sq_l += (l as f64) * (l as f64);
                sum_sq_r += (r as f64) * (r as f64);
            }

            let rms_l = (sum_sq_l / num_samples as f64).sqrt() as f32;
            let rms_r = (sum_sq_r / num_samples as f64).sqrt() as f32;
            vis.update_levels(peak_l, peak_r, rms_l, rms_r);

            let step = (num_samples / 64).max(1);
            for i in (0..num_samples).step_by(step) {
                vis.try_push(fake_output_l[i], fake_output_r[i]);
            }
        }

        let (peak_left, peak_right) = vis.peak_levels();
        let (rms_left, _rms_right) = vis.rms_levels();
        assert!(
            (peak_left - amplitude).abs() < 0.001,
            "peak_left should be {amplitude}, got {}",
            peak_left
        );
        assert!(
            (peak_right - amplitude).abs() < 0.001,
            "peak_right should be {amplitude}, got {}",
            peak_right
        );
        assert!(
            (rms_left - amplitude).abs() < 0.001,
            "rms_left should be ~{amplitude} for constant signal, got {}",
            rms_left
        );
        // Waveform buffer should have non-zero samples
        let has_data = vis.with_waveform(|left, _, _| {
            left.iter().any(|&s| s != 0.0)
        }).unwrap_or(false);
        assert!(has_data, "waveform buffer should contain pushed samples");
    }

    #[test]
    fn test_visualizer_decay_after_silence() {
        use crate::editor::visualizer::VisualizerState;

        let vis = VisualizerState::new(64);
        // Set some levels
        vis.update_levels(0.8, 0.6, 0.5, 0.3);
        let (peak_left, _) = vis.peak_levels();
        assert_eq!(peak_left, 0.8);

        // Decay several times (simulating ~10 frames of no audio)
        for _ in 0..10 {
            vis.decay_levels(0.92);
        }

        // Peak should have decayed significantly
        let (peak_left, _) = vis.peak_levels();
        assert!(
            peak_left < 0.4,
            "peak should decay after silence, got {}",
            peak_left
        );
    }

    #[test]
    fn test_slot_render_feeds_visualizer_end_to_end() {
        // Full pipeline: load preset → NoteOn → render → feed visualizer → check levels
        use crate::editor::visualizer::VisualizerState;
        use crate::slots::SlotManager;
        use songwalker_core::preset::instance::{LoadedZone, PresetInstance};
        use songwalker_core::preset::{
            AudioCodec, AudioReference, KeyRange, PresetCategory, PresetDescriptor, PresetNode,
            SampleZone, SamplerConfig, ZonePitch,
        };

        // Create slot manager with one slot
        let mut slot_manager = SlotManager::new_empty();
        slot_manager.initialize(44100.0);
        slot_manager.allocate_all();

        // Create a 440 Hz sine preset
        let num_frames = 44100;
        let pcm: Vec<f32> = (0..num_frames)
            .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin())
            .collect();

        let zone = SampleZone {
            key_range: KeyRange { low: 0, high: 127 },
            velocity_range: None,
            pitch: ZonePitch { root_note: 69, fine_tune_cents: 0.0 },
            sample_rate: 44100,
            r#loop: None,
            audio: AudioReference::External {
                url: "test.mp3".into(), codec: AudioCodec::Mp3, sha256: None,
            },
        };
        let loaded_zone = LoadedZone {
            zone: zone.clone(),
            pcm_data: Arc::from(pcm),
            channels: 1,
            sample_rate: 44100,
        };
        let preset = Arc::new(PresetInstance {
            descriptor: PresetDescriptor {
                format: None, version: None,
                id: "test".into(), name: "Test".into(),
                category: PresetCategory::Sampler,
                tags: vec![], metadata: None, tuning: None,
                graph: PresetNode::Sampler {
                    config: SamplerConfig { zones: vec![zone], is_drum_kit: false, envelope: None },
                },
            },
            zones: vec![loaded_zone],
        });

        // Load preset and trigger note (simulating preview)
        let transport = crate::transport::TransportState::default();
        slot_manager.slots_mut()[0]
            .preset_state_mut()
            .load_preset(Arc::new("test/e2e".to_string()), preset);

        let note_on = nih_plug::prelude::NoteEvent::NoteOn {
            timing: 0, voice_id: None, channel: 0, note: 69, velocity: 0.8,
        };
        slot_manager.slots_mut()[0].handle_midi_event(&note_on, &transport);
        assert_eq!(slot_manager.slots()[0].active_voice_count(), 1);

        // Render into a scratch buffer
        let num_samples = 512;
        let mut scratch = crate::perf::pool::MixBuffer::new(num_samples);
        scratch.clear_n(num_samples);

        let (sl, sr) = scratch.channels_mut();
        slot_manager.slots_mut()[0].render(sl, sr, num_samples, 44100.0, &transport);

        // Check scratch buffer has audio
        let peak_scratch = scratch.left()[..num_samples]
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        assert!(peak_scratch > 0.01, "slot render should produce audio, peak={peak_scratch}");

        // Feed to visualizer
        let vis = Arc::new(VisualizerState::new(64));
        {
            let left = scratch.left();
            let right = scratch.right();
            let mut peak_l = 0.0_f32;
            let mut peak_r = 0.0_f32;
            let mut sum_sq_l = 0.0_f64;
            let mut sum_sq_r = 0.0_f64;

            for i in 0..num_samples {
                peak_l = peak_l.max(left[i].abs());
                peak_r = peak_r.max(right[i].abs());
                sum_sq_l += (left[i] as f64).powi(2);
                sum_sq_r += (right[i] as f64).powi(2);
            }
            let rms_l = (sum_sq_l / num_samples as f64).sqrt() as f32;
            let rms_r = (sum_sq_r / num_samples as f64).sqrt() as f32;
            vis.update_levels(peak_l, peak_r, rms_l, rms_r);

            let step = (num_samples / 64).max(1);
            for i in (0..num_samples).step_by(step) {
                vis.try_push(left[i], right[i]);
            }
        }

        // Verify visualizer received the data
        let (peak_left, _) = vis.peak_levels();
        let (rms_left, _) = vis.rms_levels();
        assert!(peak_left > 0.01, "visualizer peak_left should show activity, got {}", peak_left);
        assert!(rms_left > 0.001, "visualizer rms_left should show activity, got {}", rms_left);
        let waveform_has_data = vis.with_waveform(|left, _, _| {
            left.iter().any(|&s| s.abs() > 0.001)
        }).unwrap_or(false);
        assert!(waveform_has_data, "visualizer waveform should have non-zero data");
    }

    /// Test the full channel-based pipeline: simulates what happens at runtime
    /// when the browser sends a PresetLoadedEvent through the two-hop relay.
    #[test]
    fn test_full_channel_relay_pipeline() {
        use crate::editor::visualizer::VisualizerState;
        use crate::editor::PresetLoadedEvent;
        use crate::slots::SlotManager;
        use songwalker_core::preset::instance::{LoadedZone, PresetInstance};
        use songwalker_core::preset::{
            AudioCodec, AudioReference, KeyRange, PresetCategory, PresetDescriptor, PresetNode,
            SampleZone, SamplerConfig, ZonePitch,
        };

        // Create channels like the real app
        let (audio_preset_loaded_tx, audio_preset_loaded_rx) =
            crossbeam_channel::bounded::<PresetLoadedEvent>(16);
        let (ui_preset_loaded_tx, ui_preset_loaded_rx) =
            crossbeam_channel::unbounded::<PresetLoadedEvent>();

        // Create slot manager and engine
        let mut slot_manager = SlotManager::new_empty();
        slot_manager.initialize(44100.0);
        slot_manager.allocate_all();

        let mut engine = AudioEngine::new();
        engine.initialize(44100.0, 1024);
        let transport = crate::transport::TransportState::default();
        let visualizer_state = Arc::new(VisualizerState::new(512));
        let voice_count = Arc::new(AtomicU32::new(0));

        // Build a test preset
        let num_frames = 44100;
        let pcm: Vec<f32> = (0..num_frames)
            .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin())
            .collect();
        let zone = SampleZone {
            key_range: KeyRange { low: 0, high: 127 },
            velocity_range: None,
            pitch: ZonePitch { root_note: 69, fine_tune_cents: 0.0 },
            sample_rate: 44100,
            r#loop: None,
            audio: AudioReference::External {
                url: "test.mp3".into(), codec: AudioCodec::Mp3, sha256: None,
            },
        };
        let loaded_zone = LoadedZone {
            zone: zone.clone(),
            pcm_data: Arc::from(pcm),
            channels: 1,
            sample_rate: 44100,
        };
        let instance = Arc::new(PresetInstance {
            descriptor: PresetDescriptor {
                format: None, version: None,
                id: "test".into(), name: "Test".into(),
                category: PresetCategory::Sampler,
                tags: vec![], metadata: None, tuning: None,
                graph: PresetNode::Sampler {
                    config: SamplerConfig { zones: vec![zone], is_drum_kit: false, envelope: None },
                },
            },
            zones: vec![loaded_zone],
        });

        // Simulate browser: send preset to ui_preset_loaded_tx with play_note
        let event = PresetLoadedEvent {
            slot_index: 0,
            preset_id: Arc::new("test/relay".to_string()),
            instance: instance.clone(),
            play_note: Some(60),
        };
        ui_preset_loaded_tx.send(event).unwrap();

        // Simulate UI frame: drain ui_preset_loaded_rx → forward to audio_preset_loaded_tx
        let mut active_presets_ui = std::collections::HashMap::new();
        while let Ok(loaded) = ui_preset_loaded_rx.try_recv() {
            active_presets_ui.insert(
                loaded.slot_index,
                (loaded.preset_id.clone(), loaded.instance.clone()),
            );
            audio_preset_loaded_tx.try_send(loaded).unwrap();
        }
        assert!(!active_presets_ui.is_empty(), "UI should have received the preset");

        // Simulate audio callback: drain audio_preset_loaded_rx → load preset → NoteOn
        while let Ok(loaded) = audio_preset_loaded_rx.try_recv() {
            assert_eq!(loaded.slot_index, 0);
            assert!(loaded.play_note.is_some());
            let slot = &mut slot_manager.slots_mut()[loaded.slot_index];
            slot.preset_state_mut()
                .load_preset(loaded.preset_id.clone(), loaded.instance.clone());
            if let Some(note) = loaded.play_note {
                let note_event = nih_plug::prelude::NoteEvent::NoteOn {
                    timing: 0, voice_id: None, channel: 0,
                    note, velocity: 0.8,
                };
                slot_manager.slots_mut()[loaded.slot_index]
                    .handle_midi_event(&note_event, &transport);
            }
        }

        // Verify voice was allocated
        let voices = slot_manager.slots()[0].active_voice_count();
        assert!(voices > 0, "Expected active voices after NoteOn, got {}", voices);

        // Render via render_and_mix (the exact function used in the audio callback)
        render_and_mix(
            512,
            &mut engine,
            &mut slot_manager,
            &transport,
            1.0,  // master_gain
            0.0,  // master_pan (center)
            &visualizer_state,
            &voice_count,
        );

        // Check engine output has audio
        let engine_peak = engine.output_left[..512].iter()
            .chain(engine.output_right[..512].iter())
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        assert!(engine_peak > 0.01, "engine output should have audio, peak={}", engine_peak);

        // Check visualizer received the data
        let (vis_peak_l, vis_peak_r) = visualizer_state.peak_levels();
        let (vis_rms_l, _vis_rms_r) = visualizer_state.rms_levels();
        assert!(vis_peak_l > 0.01, "vis peak_left should show activity, got {}", vis_peak_l);
        assert!(vis_peak_r > 0.01, "vis peak_right should show activity, got {}", vis_peak_r);
        assert!(vis_rms_l > 0.001, "vis rms_left should show activity, got {}", vis_rms_l);

        let waveform_ok = visualizer_state.with_waveform(|left, _, _| {
            left.iter().any(|&s| s.abs() > 0.001)
        }).unwrap_or(false);
        assert!(waveform_ok, "visualizer waveform should have non-zero data");
    }
}
