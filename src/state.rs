use serde::{Deserialize, Serialize};

/// Serialized plugin state – saved/restored by the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginState {
    /// Library URLs that have been added.
    pub library_urls: Vec<String>,
    /// Per-slot configuration.
    pub slot_configs: Vec<SlotConfig>,
}

impl Default for PluginState {
    fn default() -> Self {
        Self {
            library_urls: vec![
                "https://clevertree.github.io/songwalker-library".to_string(),
            ],
            slot_configs: Vec::new(),
        }
    }
}

impl PluginState {
    /// Add a new slot configuration and return its index.
    pub fn add_slot_config(&mut self, config: SlotConfig) -> usize {
        let idx = self.slot_configs.len();
        self.slot_configs.push(config);
        idx
    }

    /// Remove a slot by index.
    pub fn remove_slot_config(&mut self, index: usize) {
        if index < self.slot_configs.len() {
            self.slot_configs.remove(index);
        }
    }

    /// Serialize the state to JSON bytes for host persistence.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Deserialize from JSON bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }
}

/// Configuration for a single slot, persisted in the project.
///
/// Each slot is a unified instrument that can load a preset and/or
/// contain `.sw` source code (like the web editor). There is no
/// separate "preset" vs "runner" mode — presets are loaded via
/// `loadPreset()` calls in the source code, or assigned directly
/// through the browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotConfig {
    /// Display name (typically the preset or file name).
    pub name: String,
    /// Preset identifier (library_key / instrument_key), if any.
    pub preset_id: Option<String>,
    /// MIDI channel this slot responds to (0 = omni, 1-16 = specific).
    pub midi_channel: i32,
    /// Volume 0.0–1.0.
    pub volume: f32,
    /// Pan −1.0 (L) .. +1.0 (R).
    pub pan: f32,
    /// Muted flag.
    pub muted: bool,
    /// Solo flag.
    pub solo: bool,
    /// Root MIDI note for triggering (default 60 = C4).
    pub root_note: u8,
    /// Song Walker source code (optional inline editor).
    pub source_code: String,
    /// Last compilation error, not persisted.
    #[serde(skip)]
    pub compile_error: Option<String>,
}

impl Default for SlotConfig {
    fn default() -> Self {
        Self {
            name: "New Slot".to_string(),
            preset_id: None,
            midi_channel: 0,
            volume: 0.8,
            pan: 0.0,
            muted: false,
            solo: false,
            root_note: 60,
            source_code: String::new(),
            compile_error: None,
        }
    }
}

impl SlotConfig {
    /// Create a new slot with a preset assigned.
    pub fn new_preset(name: &str, preset_id: &str) -> Self {
        Self {
            name: name.to_string(),
            preset_id: Some(preset_id.to_string()),
            ..Self::default()
        }
    }

    /// Create a new slot with source code.
    pub fn new_with_source(name: &str, source: &str) -> Self {
        Self {
            name: name.to_string(),
            source_code: source.to_string(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_state_default() {
        let state = PluginState::default();
        assert_eq!(state.library_urls.len(), 1);
        assert!(state.library_urls[0].contains("songwalker-library"));
        assert!(state.slot_configs.is_empty());
    }

    #[test]
    fn test_plugin_state_serialize_roundtrip() {
        let mut state = PluginState::default();
        state.add_slot_config(SlotConfig::new_preset("Piano", "lib/piano"));
        state.add_slot_config(SlotConfig::new_with_source("Custom", "loadPreset('test')"));

        let bytes = state.to_bytes();
        let restored = PluginState::from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(restored.library_urls, state.library_urls);
        assert_eq!(restored.slot_configs.len(), 2);
        assert_eq!(restored.slot_configs[0].name, "Piano");
        assert_eq!(restored.slot_configs[0].preset_id.as_deref(), Some("lib/piano"));
        assert_eq!(restored.slot_configs[1].name, "Custom");
        assert_eq!(restored.slot_configs[1].source_code, "loadPreset('test')");
    }

    #[test]
    fn test_plugin_state_from_invalid_bytes() {
        let result = PluginState::from_bytes(b"not valid json");
        assert!(result.is_none());
    }

    #[test]
    fn test_slot_config_default() {
        let config = SlotConfig::default();
        assert_eq!(config.name, "New Slot");
        assert!(config.preset_id.is_none());
        assert_eq!(config.midi_channel, 0); // Omni
        assert_eq!(config.volume, 0.8);
        assert_eq!(config.pan, 0.0);
        assert!(!config.muted);
        assert!(!config.solo);
        assert_eq!(config.root_note, 60);
        assert!(config.source_code.is_empty());
        assert!(config.compile_error.is_none());
    }

    #[test]
    fn test_add_remove_slot_config() {
        let mut state = PluginState::default();
        let idx = state.add_slot_config(SlotConfig::default());
        assert_eq!(idx, 0);
        assert_eq!(state.slot_configs.len(), 1);

        let idx2 = state.add_slot_config(SlotConfig::new_preset("Bass", "lib/bass"));
        assert_eq!(idx2, 1);
        assert_eq!(state.slot_configs.len(), 2);

        state.remove_slot_config(0);
        assert_eq!(state.slot_configs.len(), 1);
        assert_eq!(state.slot_configs[0].name, "Bass");
    }

    #[test]
    fn test_remove_slot_config_out_of_bounds() {
        let mut state = PluginState::default();
        state.add_slot_config(SlotConfig::default());
        state.remove_slot_config(5); // Out of bounds — should not panic
        assert_eq!(state.slot_configs.len(), 1);
    }

    #[test]
    fn test_slot_config_new_preset() {
        let config = SlotConfig::new_preset("Grand Piano", "FluidR3_GM/acoustic_grand_piano");
        assert_eq!(config.name, "Grand Piano");
        assert_eq!(config.preset_id.as_deref(), Some("FluidR3_GM/acoustic_grand_piano"));
        assert_eq!(config.volume, 0.8); // Inherits default
    }

    #[test]
    fn test_slot_config_new_with_source() {
        let config = SlotConfig::new_with_source("Track 1", "C D E F");
        assert_eq!(config.name, "Track 1");
        assert_eq!(config.source_code, "C D E F");
        assert!(config.preset_id.is_none());
    }
}
