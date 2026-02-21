use std::sync::Arc;
use songwalker_core::preset::instance::PresetInstance;

use super::slot::EnvelopeParams;

/// State specific to a Preset-mode slot.
pub struct PresetSlotState {
    /// The currently loaded and active preset (fully decoded, ready for audio thread).
    pub active_preset: Option<Arc<PresetInstance>>,
    /// Identifier of the loaded preset (library/path).
    pub preset_id: Option<Arc<String>>,
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
    pub fn load_preset(&mut self, id: Arc<String>, instance: Arc<PresetInstance>) {
        self.preset_id = Some(id);
        self.active_preset = Some(instance);
    }

    /// Unload the current preset.
    pub fn unload_preset(&mut self) {
        self.preset_id = None;
        self.active_preset = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let state = PresetSlotState::default();
        assert!(state.active_preset.is_none());
        assert!(state.preset_id.is_none());
        assert_eq!(state.pitch_bend, 0.0);
        assert_eq!(state.mod_wheel, 0.0);
        assert_eq!(state.expression, 1.0);
    }

    #[test]
    fn test_handle_cc1_mod_wheel() {
        let mut state = PresetSlotState::default();
        state.handle_cc(1, 0.75);
        assert_eq!(state.mod_wheel, 0.75);
    }

    #[test]
    fn test_handle_cc11_expression() {
        let mut state = PresetSlotState::default();
        state.handle_cc(11, 0.5);
        assert_eq!(state.expression, 0.5);
    }

    #[test]
    fn test_handle_cc_volume_pan_at_slot_level() {
        let mut state = PresetSlotState::default();
        let orig_mod = state.mod_wheel;
        let orig_expr = state.expression;
        // CC7 (volume) and CC10 (pan) are handled at slot level, not here
        state.handle_cc(7, 0.9);
        state.handle_cc(10, 0.3);
        assert_eq!(state.mod_wheel, orig_mod);
        assert_eq!(state.expression, orig_expr);
    }

    #[test]
    fn test_envelope_default() {
        let state = PresetSlotState::default();
        let env = state.envelope();
        // EnvelopeParams::default() should have reasonable values
        // Just verify it doesn't panic and returns something
        let _ = env;
    }

    #[test]
    fn test_set_envelope() {
        let mut state = PresetSlotState::default();
        let mut env = state.envelope();
        env.attack_secs = 0.1;
        state.set_envelope(env);
        assert_eq!(state.envelope().attack_secs, 0.1);
    }

    #[test]
    fn test_unload_preset() {
        let mut state = PresetSlotState::default();
        // Load something
        state.preset_id = Some(Arc::new("test/preset".to_string()));
        // Unload
        state.unload_preset();
        assert!(state.preset_id.is_none());
        assert!(state.active_preset.is_none());
    }
}
