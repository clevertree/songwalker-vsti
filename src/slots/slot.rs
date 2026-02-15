use nih_plug::prelude::*;

use super::preset_slot::PresetSlotState;
use super::runner_slot::RunnerSlotState;
use crate::transport::TransportState;

/// The mode of operation for a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotMode {
    /// Direct MIDI → preset playback.
    Preset,
    /// `.sw` track execution triggered by MIDI.
    Runner,
}

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
pub struct Slot {
    /// Slot index in the rack.
    index: usize,
    /// Current mode.
    mode: SlotMode,
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
    /// Preset-specific state.
    preset_state: PresetSlotState,
    /// Runner-specific state.
    runner_state: RunnerSlotState,
    /// Display name for the slot.
    pub name: String,
}

impl Slot {
    pub fn new(index: usize, mode: SlotMode) -> Self {
        Self {
            index,
            mode,
            voice_pool: VoicePool::new(64),
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            midi_channel: 0,
            sample_rate: 44100.0,
            preset_state: PresetSlotState::default(),
            runner_state: RunnerSlotState::default(),
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

    pub fn mode(&self) -> SlotMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: SlotMode) {
        self.reset();
        self.mode = mode;
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
    pub fn handle_midi_event(&mut self, event: &NoteEvent<()>, transport: &TransportState) {
        match self.mode {
            SlotMode::Preset => self.handle_preset_midi(event),
            SlotMode::Runner => self.handle_runner_midi(event, transport),
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
                        if let Some(zone) = preset_instance.find_zone(*note, *velocity) {
                            let pitch = zone.pitch();
                            let rate = songwalker_core::preset::sample_playback_rate(
                                *note,
                                pitch.root_note,
                                pitch.fine_tune_cents,
                                440.0,
                            );
                            voice.sample_rate_ratio = rate * (zone.sample_rate() as f64 / self.sample_rate as f64);
                            voice.sample_pos = 0.0;
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
        match self.mode {
            SlotMode::Preset => {
                self.render_preset(left, right, num_samples, sample_rate);
            }
            SlotMode::Runner => {
                self.render_runner(left, right, num_samples, sample_rate, transport);
            }
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

                // Generate sample (sine oscillator as default / fallback)
                let sample = if self.preset_state.active_preset.is_some() {
                    // TODO: Use actual preset DSP graph (sampler/oscillator/composite)
                    // For now, fall back to sine
                    let s = (voice.phase * std::f64::consts::TAU).sin() as f32;
                    voice.phase += voice.phase_inc;
                    if voice.phase >= 1.0 {
                        voice.phase -= 1.0;
                    }
                    s
                } else {
                    // Pure sine fallback
                    let s = (voice.phase * std::f64::consts::TAU).sin() as f32;
                    voice.phase += voice.phase_inc;
                    if voice.phase >= 1.0 {
                        voice.phase -= 1.0;
                    }
                    s
                };

                let out = sample * env * voice.velocity;
                left[i] += out;
                right[i] += out;
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

        // Render the triggered voices (same as preset rendering)
        let adsr = self.runner_state.envelope();
        for voice in self.voice_pool.active_voices_mut() {
            for i in 0..num_samples {
                let env = advance_envelope(voice, &adsr, sample_rate);
                if voice.env_stage >= 4 {
                    break;
                }
                let s = (voice.phase * std::f64::consts::TAU).sin() as f32;
                voice.phase += voice.phase_inc;
                if voice.phase >= 1.0 {
                    voice.phase -= 1.0;
                }
                let out = s * env * voice.velocity;
                left[i] += out;
                right[i] += out;
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
