use nih_plug::prelude::*;

use super::preset_slot::PresetSlotState;
use super::runner_slot::RunnerSlotState;
use crate::transport::TransportState;

/// Voice state for a single voice in the pre-allocated pool.
#[derive(Clone)]
pub struct Voice {
    /// Whether this voice is currently active.
    pub active: bool,
    /// MIDI note that triggered this voice.
    pub note: u8,
    /// Current velocity (0.0–1.0).
    pub velocity: f32,
    /// Current phase (for oscillator-based presets).
    pub phase: f64,
    /// Phase increment per sample.
    pub phase_inc: f64,
    /// Envelope state: current gain.
    pub env_gain: f32,
    /// Envelope stage: 0=attack, 1=decay, 2=sustain, 3=release, 4=off.
    pub env_stage: u8,
    /// Samples elapsed in current envelope stage.
    pub env_samples: u32,
    /// Whether voice is in release phase (waiting to finish).
    pub releasing: bool,
    /// Sample playback position (for sampler presets).
    pub sample_pos: f64,
    /// Sample playback rate (for sampler presets, accounting for pitch).
    pub sample_rate_ratio: f64,
    /// Transpose offset in semitones (for runner mode).
    pub transpose: i32,
    /// Index of the loaded zone (for sampler rendering).
    pub zone_index: Option<usize>,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            note: 0,
            velocity: 0.0,
            phase: 0.0,
            phase_inc: 0.0,
            env_gain: 0.0,
            env_stage: 4,
            env_samples: 0,
            releasing: false,
            sample_pos: 0.0,
            sample_rate_ratio: 1.0,
            transpose: 0,
            zone_index: None,
        }
    }
}

/// Pre-allocated voice pool for a single slot.
pub struct VoicePool {
    voices: Vec<Voice>,
    _max_polyphony: usize,
}

impl VoicePool {
    pub fn new(max_polyphony: usize) -> Self {
        Self {
            voices: vec![Voice::default(); max_polyphony],
            _max_polyphony: max_polyphony,
        }
    }

    /// Allocate a voice for a new note. Uses round-robin stealing if full.
    pub fn allocate(&mut self, note: u8, velocity: f32) -> Option<&mut Voice> {
        // Find an inactive voice, or steal the oldest releasing/oldest voice
        let idx = if let Some(idx) = self.voices.iter().position(|v| !v.active) {
            idx
        } else {
            // Steal: prefer releasing voices, fall back to index 0
            self.voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.releasing)
                .map(|(i, _)| i)
                .next()
                .unwrap_or(0)
        };

        let voice = &mut self.voices[idx];
        voice.active = true;
        voice.note = note;
        voice.velocity = velocity;
        voice.env_stage = 0;
        voice.env_samples = 0;
        voice.env_gain = 0.0;
        voice.releasing = false;
        voice.phase = 0.0;
        voice.sample_pos = 0.0;
        Some(voice)
    }

    /// Release all voices matching the given note.
    pub fn release(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note && !voice.releasing {
                voice.releasing = true;
                voice.env_stage = 3; // Jump to release stage
                voice.env_samples = 0;
            }
        }
    }

    /// Release all active voices.
    pub fn release_all(&mut self) {
        for voice in &mut self.voices {
            if voice.active && !voice.releasing {
                voice.releasing = true;
                voice.env_stage = 3;
                voice.env_samples = 0;
            }
        }
    }

    /// Get all active voices for rendering.
    pub fn active_voices_mut(&mut self) -> impl Iterator<Item = &mut Voice> {
        self.voices.iter_mut().filter(|v| v.active)
    }

    /// Count of currently active voices.
    pub fn active_count(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    /// Deactivate voices whose envelope has finished.
    pub fn cleanup_finished(&mut self) {
        for voice in &mut self.voices {
            if voice.active && voice.env_stage >= 4 {
                voice.active = false;
            }
        }
    }
}

/// A single instrument slot in the rack.
///
/// Each slot is a unified instrument that handles MIDI → preset playback.
/// If source code is loaded, it can also run `.sw` tracks.
/// This matches the web editor model where presets are loaded via
/// `loadPreset()` in source code.
pub struct Slot {
    /// Slot index in the rack.
    index: usize,
    /// Pre-allocated voice pool.
    voice_pool: VoicePool,
    /// Volume gain (linear).
    volume: f32,
    /// Pan position (-1 to 1).
    pan: f32,
    /// Whether muted.
    muted: bool,
    /// Whether soloed.
    solo: bool,
    /// MIDI channel (0 = all, 1–16 = specific).
    midi_channel: i32,
    /// Host sample rate.
    sample_rate: f32,
    /// Preset-specific state (sampler zones, envelope, etc.).
    preset_state: PresetSlotState,
    /// Runner-specific state (for .sw source code execution).
    runner_state: RunnerSlotState,
    /// Whether this slot has .sw source code loaded.
    has_source: bool,
    /// Display name for the slot.
    pub name: String,
}

impl Slot {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            voice_pool: VoicePool::new(64),
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            midi_channel: 0,
            sample_rate: 44100.0,
            preset_state: PresetSlotState::default(),
            runner_state: RunnerSlotState::default(),
            has_source: false,
            name: format!("Slot {}", index + 1),
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    pub fn reset(&mut self) {
        self.voice_pool.release_all();
        self.runner_state.reset();
    }

    pub fn set_index(&mut self, index: usize) {
        self.index = index;
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// Whether this slot has .sw source code loaded.
    pub fn has_source(&self) -> bool {
        self.has_source
    }

    /// Set whether this slot has source code.
    pub fn set_has_source(&mut self, has_source: bool) {
        self.has_source = has_source;
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol;
    }

    pub fn pan(&self) -> f32 {
        self.pan
    }

    pub fn set_pan(&mut self, pan: f32) {
        self.pan = pan;
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn is_solo(&self) -> bool {
        self.solo
    }

    pub fn set_solo(&mut self, solo: bool) {
        self.solo = solo;
    }

    pub fn midi_channel(&self) -> i32 {
        self.midi_channel
    }

    pub fn set_midi_channel(&mut self, ch: i32) {
        self.midi_channel = ch.clamp(0, 16);
    }

    pub fn active_voice_count(&self) -> usize {
        self.voice_pool.active_count()
    }

    pub fn preset_state(&self) -> &PresetSlotState {
        &self.preset_state
    }

    pub fn preset_state_mut(&mut self) -> &mut PresetSlotState {
        &mut self.preset_state
    }

    pub fn runner_state(&self) -> &RunnerSlotState {
        &self.runner_state
    }

    pub fn runner_state_mut(&mut self) -> &mut RunnerSlotState {
        &mut self.runner_state
    }

    /// Handle an incoming MIDI event.
    ///
    /// If the slot has source code, it routes to the runner.
    /// Otherwise, it routes to preset playback.
    pub fn handle_midi_event(&mut self, event: &NoteEvent<()>, transport: &TransportState) {
        if self.has_source {
            self.handle_runner_midi(event, transport);
        } else {
            self.handle_preset_midi(event);
        }
    }

    fn handle_preset_midi(&mut self, event: &NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn { note, velocity, .. } => {
                if let Some(voice) = self.voice_pool.allocate(*note, *velocity) {
                    let freq = crate::midi::midi_to_freq(*note);
                    voice.phase_inc = freq as f64 / self.sample_rate as f64;

                    // If a sampler preset is loaded, configure sample playback
                    if let Some(ref preset_instance) = self.preset_state.active_preset {
                        if let Some((zone_idx, zone)) = preset_instance.find_zone_indexed(*note, *velocity) {
                            let pitch = zone.pitch();
                            let rate = songwalker_core::preset::sample_playback_rate(
                                *note,
                                pitch.root_note,
                                pitch.fine_tune_cents,
                                440.0,
                            );
                            voice.sample_rate_ratio = rate * (zone.sample_rate() as f64 / self.sample_rate as f64);
                            voice.sample_pos = 0.0;
                            voice.zone_index = Some(zone_idx);
                        }
                    }
                }
            }
            NoteEvent::NoteOff { note, .. } => {
                self.voice_pool.release(*note);
            }
            NoteEvent::MidiPitchBend { value, .. } => {
                self.preset_state.pitch_bend = *value;
            }
            NoteEvent::MidiCC { cc, value, .. } => {
                self.preset_state.handle_cc(*cc, *value);
            }
            _ => {}
        }
    }

    fn handle_runner_midi(&mut self, event: &NoteEvent<()>, transport: &TransportState) {
        match event {
            NoteEvent::NoteOn { note, velocity, .. } => {
                // Spawn a new runner instance transposed by the MIDI note
                self.runner_state
                    .spawn_instance(*note, *velocity, transport);
            }
            NoteEvent::NoteOff { note, .. } => {
                // Release the runner instance for this note
                self.runner_state.release_instance(*note);
                self.voice_pool.release(*note);
            }
            NoteEvent::MidiPitchBend { value, .. } => {
                self.runner_state.pitch_bend = *value;
            }
            _ => {}
        }
    }

    /// Render this slot's audio into the provided stereo buffers.
    pub fn render(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        num_samples: usize,
        sample_rate: f32,
        transport: &TransportState,
    ) {
        if self.has_source {
            self.render_runner(left, right, num_samples, sample_rate, transport);
        } else {
            self.render_preset(left, right, num_samples, sample_rate);
        }

        self.voice_pool.cleanup_finished();
    }

    fn render_preset(&mut self, left: &mut [f32], right: &mut [f32], num_samples: usize, sample_rate: f32) {
        let adsr = self.preset_state.envelope();

        for voice in self.voice_pool.active_voices_mut() {
            for i in 0..num_samples {
                // Advance envelope
                let env = advance_envelope(voice, &adsr, sample_rate);
                if voice.env_stage >= 4 {
                    break;
                }

                // Generate sample from loaded zone (sampler) or fallback to sine
                let (sample_l, sample_r) = match (voice.zone_index, self.preset_state.active_preset.as_ref()) {
                    (Some(zi), Some(preset)) if zi < preset.zones.len() => {
                        let zone = &preset.zones[zi];
                        let pcm = &zone.pcm_data;
                        let channels = zone.channels as usize;
                        let total_frames = pcm.len() / channels;

                        if total_frames == 0 || voice.sample_pos >= total_frames as f64 {
                            // Past end of sample — mark voice finished
                            voice.env_stage = 4;
                            break;
                        }

                        // Linear interpolation between adjacent frames
                        let pos = voice.sample_pos;
                        let idx0 = pos as usize;
                        let frac = (pos - idx0 as f64) as f32;
                        let idx1 = (idx0 + 1).min(total_frames - 1);

                        let (l, r) = if channels >= 2 {
                            let l0 = pcm[idx0 * 2];
                            let l1 = pcm[idx1 * 2];
                            let r0 = pcm[idx0 * 2 + 1];
                            let r1 = pcm[idx1 * 2 + 1];
                            (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac)
                        } else {
                            let s0 = pcm[idx0];
                            let s1 = pcm[idx1];
                            let s = s0 + (s1 - s0) * frac;
                            (s, s)
                        };

                        voice.sample_pos += voice.sample_rate_ratio;
                        (l, r)
                    }
                    _ => {
                        // Pure sine fallback (no preset loaded or no matching zone)
                        let s = (voice.phase * std::f64::consts::TAU).sin() as f32;
                        voice.phase += voice.phase_inc;
                        if voice.phase >= 1.0 {
                            voice.phase -= 1.0;
                        }
                        (s, s)
                    }
                };

                let gain = env * voice.velocity;
                left[i] += sample_l * gain;
                right[i] += sample_r * gain;
            }
        }
    }

    fn render_runner(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        num_samples: usize,
        sample_rate: f32,
        transport: &TransportState,
    ) {
        // Advance runner instances and trigger/release voices
        self.runner_state.advance(
            &mut self.voice_pool,
            num_samples,
            sample_rate,
            transport,
        );

        // Render the triggered voices using sampler or sine fallback
        let adsr = self.runner_state.envelope();
        for voice in self.voice_pool.active_voices_mut() {
            for i in 0..num_samples {
                let env = advance_envelope(voice, &adsr, sample_rate);
                if voice.env_stage >= 4 {
                    break;
                }

                let (sample_l, sample_r) = match (voice.zone_index, self.preset_state.active_preset.as_ref()) {
                    (Some(zi), Some(preset)) if zi < preset.zones.len() => {
                        let zone = &preset.zones[zi];
                        let pcm = &zone.pcm_data;
                        let channels = zone.channels as usize;
                        let total_frames = pcm.len() / channels;

                        if total_frames == 0 || voice.sample_pos >= total_frames as f64 {
                            voice.env_stage = 4;
                            break;
                        }

                        let pos = voice.sample_pos;
                        let idx0 = pos as usize;
                        let frac = (pos - idx0 as f64) as f32;
                        let idx1 = (idx0 + 1).min(total_frames - 1);

                        let (l, r) = if channels >= 2 {
                            let l0 = pcm[idx0 * 2];
                            let l1 = pcm[idx1 * 2];
                            let r0 = pcm[idx0 * 2 + 1];
                            let r1 = pcm[idx1 * 2 + 1];
                            (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac)
                        } else {
                            let s0 = pcm[idx0];
                            let s1 = pcm[idx1];
                            let s = s0 + (s1 - s0) * frac;
                            (s, s)
                        };

                        voice.sample_pos += voice.sample_rate_ratio;
                        (l, r)
                    }
                    _ => {
                        let s = (voice.phase * std::f64::consts::TAU).sin() as f32;
                        voice.phase += voice.phase_inc;
                        if voice.phase >= 1.0 {
                            voice.phase -= 1.0;
                        }
                        (s, s)
                    }
                };

                let gain = env * voice.velocity;
                left[i] += sample_l * gain;
                right[i] += sample_r * gain;
            }
        }
    }
}

/// ADSR envelope parameters.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeParams {
    pub attack_secs: f32,
    pub decay_secs: f32,
    pub sustain_level: f32,
    pub release_secs: f32,
}

impl Default for EnvelopeParams {
    fn default() -> Self {
        Self {
            attack_secs: 0.01,
            decay_secs: 0.1,
            sustain_level: 0.8,
            release_secs: 0.3,
        }
    }
}

/// Advance envelope for a voice by one sample. Returns the envelope gain.
#[inline]
fn advance_envelope(voice: &mut Voice, adsr: &EnvelopeParams, sample_rate: f32) -> f32 {
    let gain = match voice.env_stage {
        0 => {
            // Attack
            let attack_samples = (adsr.attack_secs * sample_rate) as u32;
            if attack_samples == 0 || voice.env_samples >= attack_samples {
                voice.env_stage = 1;
                voice.env_samples = 0;
                voice.env_gain = 1.0;
                1.0
            } else {
                let g = voice.env_samples as f32 / attack_samples as f32;
                voice.env_gain = g;
                voice.env_samples += 1;
                g
            }
        }
        1 => {
            // Decay
            let decay_samples = (adsr.decay_secs * sample_rate) as u32;
            if decay_samples == 0 || voice.env_samples >= decay_samples {
                voice.env_stage = 2;
                voice.env_samples = 0;
                voice.env_gain = adsr.sustain_level;
                adsr.sustain_level
            } else {
                let t = voice.env_samples as f32 / decay_samples as f32;
                let g = 1.0 - t * (1.0 - adsr.sustain_level);
                voice.env_gain = g;
                voice.env_samples += 1;
                g
            }
        }
        2 => {
            // Sustain (hold until release)
            adsr.sustain_level
        }
        3 => {
            // Release
            let release_samples = (adsr.release_secs * sample_rate) as u32;
            if release_samples == 0 || voice.env_samples >= release_samples {
                voice.env_stage = 4; // Done
                voice.env_gain = 0.0;
                0.0
            } else {
                let t = voice.env_samples as f32 / release_samples as f32;
                let g = voice.env_gain * (1.0 - t);
                voice.env_samples += 1;
                g
            }
        }
        _ => 0.0,
    };

    gain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slots::preset_slot::{LoadedZone, PresetInstance};
    use songwalker_core::preset::{
        AudioCodec, AudioReference, KeyRange, PresetCategory, PresetDescriptor, PresetNode,
        SampleZone, SamplerConfig, ZonePitch,
    };
    use std::sync::Arc;

    fn default_transport() -> TransportState {
        TransportState::default()
    }

    /// Helper: create a test SampleZone.
    fn test_sample_zone() -> SampleZone {
        SampleZone {
            key_range: KeyRange { low: 0, high: 127 },
            velocity_range: None,
            pitch: ZonePitch {
                root_note: 69,
                fine_tune_cents: 0.0,
            },
            sample_rate: 44100,
            r#loop: None,
            audio: AudioReference::External {
                url: "test.mp3".into(),
                codec: AudioCodec::Mp3,
                sha256: None,
            },
        }
    }

    /// Helper: create a test PresetDescriptor wrapping a zone.
    fn test_preset_descriptor(zone: SampleZone) -> PresetDescriptor {
        PresetDescriptor {
            id: "test-preset".into(),
            name: "Test Preset".into(),
            category: PresetCategory::Sampler,
            tags: vec![],
            metadata: None,
            tuning: None,
            graph: PresetNode::Sampler {
                config: SamplerConfig {
                    zones: vec![zone],
                    is_drum_kit: false,
                    envelope: None,
                },
            },
        }
    }

    // ── Voice pool ──────────────────────────────────────────────

    #[test]
    fn voice_pool_allocate_and_release() {
        let mut pool = VoicePool::new(4);
        assert_eq!(pool.active_count(), 0);

        pool.allocate(60, 0.8);
        pool.allocate(64, 0.7);
        assert_eq!(pool.active_count(), 2);

        pool.release(60);
        let releasing: Vec<_> = pool.voices.iter().filter(|v| v.releasing).collect();
        assert_eq!(releasing.len(), 1);
        assert_eq!(releasing[0].note, 60);
    }

    #[test]
    fn voice_pool_steal_when_full() {
        let mut pool = VoicePool::new(2);
        pool.allocate(60, 0.8);
        pool.allocate(64, 0.7);
        assert_eq!(pool.active_count(), 2);

        pool.allocate(67, 0.9);
        assert_eq!(pool.active_count(), 2);
        let has_67 = pool.voices.iter().any(|v| v.active && v.note == 67);
        assert!(has_67, "should steal a voice for the new note");
    }

    #[test]
    fn voice_pool_cleanup_finished() {
        let mut pool = VoicePool::new(4);
        pool.allocate(60, 0.8);
        pool.allocate(64, 0.7);
        assert_eq!(pool.active_count(), 2);

        pool.voices[0].env_stage = 4;
        pool.cleanup_finished();
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn voice_pool_release_all() {
        let mut pool = VoicePool::new(4);
        pool.allocate(60, 0.8);
        pool.allocate(64, 0.7);
        pool.allocate(67, 0.5);
        pool.release_all();
        let releasing = pool.voices.iter().filter(|v| v.releasing).count();
        assert_eq!(releasing, 3);
    }

    // ── Slot creation ───────────────────────────────────────────

    #[test]
    fn slot_creation_defaults() {
        let slot = Slot::new(0);
        assert_eq!(slot.index(), 0);
        assert!(!slot.has_source());
        assert!(!slot.is_muted());
        assert!(!slot.is_solo());
        assert_eq!(slot.midi_channel(), 0);
        assert_eq!(slot.active_voice_count(), 0);
        assert_eq!(slot.volume(), 1.0);
        assert_eq!(slot.pan(), 0.0);
    }

    #[test]
    fn slot_source_toggle() {
        let mut slot = Slot::new(0);
        assert!(!slot.has_source());
        slot.set_has_source(true);
        assert!(slot.has_source());
    }

    // ── MIDI handling (preset mode) ─────────────────────────────

    #[test]
    fn preset_note_on_off_triggers_voice() {
        let mut slot = Slot::new(0);
        slot.initialize(44100.0);
        let transport = default_transport();

        let note_on = NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note: 60,
            velocity: 0.8,
        };
        slot.handle_midi_event(&note_on, &transport);
        assert_eq!(slot.active_voice_count(), 1);

        let note_off = NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel: 0,
            note: 60,
            velocity: 0.0,
        };
        slot.handle_midi_event(&note_off, &transport);
        // Voice is still active but in release stage
        assert_eq!(slot.active_voice_count(), 1);
    }

    // ── Envelope ────────────────────────────────────────────────

    #[test]
    fn envelope_attack_ramp() {
        let mut voice = Voice::default();
        voice.active = true;
        voice.env_stage = 0;
        voice.env_samples = 0;

        let adsr = EnvelopeParams {
            attack_secs: 0.01,
            decay_secs: 0.0,
            sustain_level: 1.0,
            release_secs: 0.01,
        };
        let sample_rate = 44100.0;
        let attack_samples = (adsr.attack_secs * sample_rate) as u32;

        let first = advance_envelope(&mut voice, &adsr, sample_rate);
        assert!(first < 0.01, "initial envelope should start near 0, got {first}");

        for _ in 1..attack_samples {
            advance_envelope(&mut voice, &adsr, sample_rate);
        }
        assert!(voice.env_gain >= 0.99, "after attack, gain should be ~1.0, got {}", voice.env_gain);
    }

    #[test]
    fn envelope_release_to_zero() {
        let mut voice = Voice::default();
        voice.active = true;
        voice.env_stage = 2; // sustain
        voice.env_gain = 0.8;

        let adsr = EnvelopeParams {
            attack_secs: 0.0,
            decay_secs: 0.0,
            sustain_level: 0.8,
            release_secs: 0.01,
        };
        let sample_rate = 44100.0;

        voice.releasing = true;
        voice.env_stage = 3;
        voice.env_samples = 0;

        let release_samples = (adsr.release_secs * sample_rate) as u32;
        let mut last_gain = 1.0;
        for _ in 0..release_samples + 10 {
            let g = advance_envelope(&mut voice, &adsr, sample_rate);
            if voice.env_stage >= 4 {
                break;
            }
            assert!(g <= last_gain + 0.001, "release should be monotonically decreasing");
            last_gain = g;
        }
        assert_eq!(voice.env_stage, 4, "voice should be off after release");
    }

    // ── Rendering ───────────────────────────────────────────────

    #[test]
    fn render_sine_fallback_produces_audio() {
        let mut slot = Slot::new(0);
        slot.initialize(44100.0);
        let transport = default_transport();

        let note_on = NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note: 69,
            velocity: 1.0,
        };
        slot.handle_midi_event(&note_on, &transport);

        let num_samples = 256;
        let mut left = vec![0.0f32; num_samples];
        let mut right = vec![0.0f32; num_samples];
        slot.render(&mut left, &mut right, num_samples, 44100.0, &transport);

        let energy: f32 = left.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "sine fallback should produce non-zero audio");
    }

    #[test]
    fn render_sampler_reads_pcm_data() {
        let mut slot = Slot::new(0);
        slot.initialize(44100.0);
        let transport = default_transport();

        // Create a simple mono preset with a 440 Hz sine at 44100 Hz
        let num_pcm_samples = 44100;
        let pcm: Vec<f32> = (0..num_pcm_samples)
            .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin())
            .collect();

        let zone = test_sample_zone();
        let loaded_zone = LoadedZone {
            zone,
            pcm_data: Arc::from(pcm),
            channels: 1,
        };

        let preset_instance = PresetInstance {
            descriptor: test_preset_descriptor(test_sample_zone()),
            zones: vec![loaded_zone],
        };

        slot.preset_state_mut().load_preset("test/preset".into(), preset_instance);

        let note_on = NoteEvent::NoteOn {
            timing: 0, voice_id: None, channel: 0, note: 69, velocity: 1.0,
        };
        slot.handle_midi_event(&note_on, &transport);

        let num_samples = 256;
        let mut left = vec![0.0f32; num_samples];
        let mut right = vec![0.0f32; num_samples];
        slot.render(&mut left, &mut right, num_samples, 44100.0, &transport);

        let energy: f32 = left.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "sampler should produce non-zero audio from PCM data");

        let max_abs = left.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(max_abs > 0.01, "peak amplitude should be audible, got {max_abs}");
    }

    #[test]
    fn render_sampler_stereo() {
        let mut slot = Slot::new(0);
        slot.initialize(44100.0);
        let transport = default_transport();

        let num_frames = 4410;
        let mut pcm = Vec::with_capacity(num_frames * 2);
        for i in 0..num_frames {
            let s = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin();
            pcm.push(s);   // left
            pcm.push(-s);  // right (inverted)
        }

        let zone = test_sample_zone();
        let loaded_zone = LoadedZone {
            zone,
            pcm_data: Arc::from(pcm),
            channels: 2,
        };

        let preset_instance = PresetInstance {
            descriptor: test_preset_descriptor(test_sample_zone()),
            zones: vec![loaded_zone],
        };

        slot.preset_state_mut().load_preset("test/stereo".into(), preset_instance);

        let note_on = NoteEvent::NoteOn {
            timing: 0, voice_id: None, channel: 0, note: 69, velocity: 1.0,
        };
        slot.handle_midi_event(&note_on, &transport);

        let num_samples = 128;
        let mut left = vec![0.0f32; num_samples];
        let mut right = vec![0.0f32; num_samples];
        slot.render(&mut left, &mut right, num_samples, 44100.0, &transport);

        let energy_l: f32 = left.iter().map(|s| s * s).sum();
        let energy_r: f32 = right.iter().map(|s| s * s).sum();
        assert!(energy_l > 0.0, "left channel should have audio");
        assert!(energy_r > 0.0, "right channel should have audio");
    }

    #[test]
    fn sampler_voice_ends_past_sample_length() {
        let mut slot = Slot::new(0);
        slot.initialize(44100.0);
        let transport = default_transport();

        // Very short sample: only 100 frames
        let pcm: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();

        let zone = test_sample_zone();
        let loaded_zone = LoadedZone {
            zone,
            pcm_data: Arc::from(pcm),
            channels: 1,
        };

        let preset_instance = PresetInstance {
            descriptor: test_preset_descriptor(test_sample_zone()),
            zones: vec![loaded_zone],
        };

        slot.preset_state_mut().load_preset("test/short".into(), preset_instance);

        let note_on = NoteEvent::NoteOn {
            timing: 0, voice_id: None, channel: 0, note: 69, velocity: 1.0,
        };
        slot.handle_midi_event(&note_on, &transport);
        assert_eq!(slot.active_voice_count(), 1);

        let mut left = vec![0.0f32; 256];
        let mut right = vec![0.0f32; 256];
        slot.render(&mut left, &mut right, 256, 44100.0, &transport);
        assert_eq!(slot.active_voice_count(), 0, "voice should be cleaned up after sample ends");
    }

    // ── Transport ───────────────────────────────────────────────

    #[test]
    fn transport_beats_to_samples() {
        let t = TransportState {
            bpm: 120.0,
            sample_rate: 44100.0,
            ..Default::default()
        };
        let samples = t.beats_to_samples(1.0);
        assert!((samples - 22050.0).abs() < 1.0, "1 beat at 120 BPM should be ~22050 samples, got {samples}");
    }

    // ── MIDI utility ────────────────────────────────────────────

    #[test]
    fn midi_to_freq_a4() {
        let freq = crate::midi::midi_to_freq(69);
        assert!((freq - 440.0).abs() < 0.01, "MIDI 69 should be 440 Hz, got {freq}");
    }

    #[test]
    fn midi_to_freq_octave() {
        let a3 = crate::midi::midi_to_freq(57);
        let a4 = crate::midi::midi_to_freq(69);
        assert!((a4 / a3 - 2.0).abs() < 0.01, "octave should double frequency");
    }

    // ── Mute / Solo ─────────────────────────────────────────────

    #[test]
    fn slot_mute_solo_setters() {
        let mut slot = Slot::new(0);
        slot.set_muted(true);
        assert!(slot.is_muted());
        slot.set_solo(true);
        assert!(slot.is_solo());
        slot.set_midi_channel(5);
        assert_eq!(slot.midi_channel(), 5);
        slot.set_midi_channel(20);
        assert_eq!(slot.midi_channel(), 16);
        slot.set_midi_channel(-1);
        assert_eq!(slot.midi_channel(), 0);
    }
}
