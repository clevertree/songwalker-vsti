use std::sync::Arc;

use songwalker_core::preset::{PresetDescriptor, SampleZone};

use super::slot::EnvelopeParams;

/// State specific to a Preset-mode slot.
pub struct PresetSlotState {
    /// The currently loaded and active preset (fully decoded, ready for audio thread).
    pub active_preset: Option<PresetInstance>,
    /// Identifier of the loaded preset (library/path).
    pub preset_id: Option<String>,
    /// Current pitch bend value (0.0 = center, -1.0..1.0 range).
    pub pitch_bend: f32,
    /// Modulation wheel (CC1).
    pub mod_wheel: f32,
    /// Expression (CC11).
    pub expression: f32,
    /// Envelope override.
    envelope: EnvelopeParams,
}

impl Default for PresetSlotState {
    fn default() -> Self {
        Self {
            active_preset: None,
            preset_id: None,
            pitch_bend: 0.0,
            mod_wheel: 0.0,
            expression: 1.0,
            envelope: EnvelopeParams::default(),
        }
    }
}

impl PresetSlotState {
    /// Handle a MIDI CC message.
    pub fn handle_cc(&mut self, cc: u8, value: f32) {
        match cc {
            1 => self.mod_wheel = value,
            7 => { /* volume — handled at slot level */ }
            10 => { /* pan — handled at slot level */ }
            11 => self.expression = value,
            64 => { /* sustain pedal — TODO */ }
            _ => {}
        }
    }

    /// Get the ADSR envelope parameters (with any overrides applied).
    pub fn envelope(&self) -> EnvelopeParams {
        self.envelope
    }

    /// Set envelope override from the UI.
    pub fn set_envelope(&mut self, env: EnvelopeParams) {
        self.envelope = env;
    }

    /// Load a new preset (called from the background thread after fetching).
    ///
    /// The `PresetInstance` must be fully prepared (samples decoded to f32 PCM).
    pub fn load_preset(&mut self, id: String, instance: PresetInstance) {
        self.preset_id = Some(id);
        self.active_preset = Some(instance);
    }

    /// Unload the current preset.
    pub fn unload_preset(&mut self) {
        self.preset_id = None;
        self.active_preset = None;
    }
}

/// A fully-loaded preset ready for use on the audio thread.
///
/// All sample data is decoded and stored as `Arc<[f32]>` slices.
/// This struct is built on a background thread and then atomically
/// swapped into the audio thread.
pub struct PresetInstance {
    /// The original preset descriptor (metadata, graph info).
    pub descriptor: PresetDescriptor,
    /// Decoded sample zones with PCM data.
    pub zones: Vec<LoadedZone>,
}

impl PresetInstance {
    /// Find the best matching zone for a given note and velocity.
    pub fn find_zone(&self, note: u8, velocity: f32) -> Option<&LoadedZone> {
        self.find_zone_indexed(note, velocity).map(|(_, z)| z)
    }

    /// Find the best matching zone, returning its index and a reference.
    pub fn find_zone_indexed(&self, note: u8, velocity: f32) -> Option<(usize, &LoadedZone)> {
        let vel_u8 = (velocity * 127.0) as u8;
        self.zones.iter().enumerate().find(|(_, z)| {
            note >= z.zone.key_range.low
                && note <= z.zone.key_range.high
                && z.zone
                    .velocity_range
                    .as_ref()
                    .map_or(true, |vr| vel_u8 >= vr.low && vel_u8 <= vr.high)
        })
    }
}

/// A sample zone with decoded PCM audio data.
pub struct LoadedZone {
    /// The original zone descriptor (key range, pitch, loop points, etc.).
    pub zone: SampleZone,
    /// Decoded audio data (mono or interleaved stereo) at the host sample rate.
    pub pcm_data: Arc<[f32]>,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u32,
}

impl LoadedZone {
    /// Get the native sample rate of this zone's pitch info.
    pub fn pitch(&self) -> &songwalker_core::preset::ZonePitch {
        &self.zone.pitch
    }

    /// Get the sample rate this zone was decoded at.
    pub fn sample_rate(&self) -> u32 {
        self.zone.sample_rate
    }
}
