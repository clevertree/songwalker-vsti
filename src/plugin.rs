use nih_plug::prelude::*;
use std::sync::{Arc, Mutex};

use crate::audio::AudioEngine;
use crate::editor;
use crate::params::SongWalkerParams;
use crate::preset::manager::PresetManager;
use crate::slots::SlotManager;
use crate::state::PluginState;
use crate::transport::TransportState;

/// The main SongWalker VSTi plugin.
pub struct SongWalkerPlugin {
    params: Arc<SongWalkerParams>,
    /// Shared audio engine holding pre-allocated voice pools and mix buffers.
    audio_engine: AudioEngine,
    /// Multi-slot instrument rack.
    slot_manager: SlotManager,
    /// Background preset manager (async fetch + cache).
    preset_manager: Arc<Mutex<PresetManager>>,
    /// Current host transport state (updated each process block).
    transport: TransportState,
    /// Serializable plugin state for DAW save/restore.
    plugin_state: Arc<Mutex<PluginState>>,
    /// Sample rate provided by the host.
    sample_rate: f32,
}

impl Default for SongWalkerPlugin {
    fn default() -> Self {
        let params = Arc::new(SongWalkerParams::default());
        Self {
            params,
            audio_engine: AudioEngine::new(),
            slot_manager: SlotManager::new(),
            preset_manager: Arc::new(Mutex::new(PresetManager::new())),
            transport: TransportState::default(),
            plugin_state: Arc::new(Mutex::new(PluginState::default())),
            sample_rate: 44100.0,
        }
    }
}

impl Plugin for SongWalkerPlugin {
    const NAME: &'static str = "SongWalker";
    const VENDOR: &'static str = "SongWalker Contributors";
    const URL: &'static str = "https://songwalker.org";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let preset_manager = self.preset_manager.clone();
        let plugin_state = self.plugin_state.clone();
        let params = self.params.clone();
        let editor_state = self.params.editor_state.clone();
        editor::create(preset_manager, plugin_state, params, editor_state)
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.audio_engine
            .initialize(buffer_config.sample_rate, buffer_config.max_buffer_size as usize);
        self.slot_manager.initialize(buffer_config.sample_rate);

        // Start background preset manager (fetches library indexes)
        let pm = self.preset_manager.clone();
        PresetManager::start_background_refresh(pm);

        true
    }

    fn reset(&mut self) {
        self.audio_engine.reset();
        self.slot_manager.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Update transport from host
        self.transport.update(context.transport());

        // Process all MIDI events and route to slots
        crate::audio::process_block(
            buffer,
            context,
            &mut self.audio_engine,
            &mut self.slot_manager,
            &self.transport,
            &self.params,
        );

        ProcessStatus::Normal
    }
}

impl ClapPlugin for SongWalkerPlugin {
    const CLAP_ID: &'static str = "org.songwalker.vsti";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Multi-timbral instrument with remote preset loading and .sw track runner");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://songwalker.org");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for SongWalkerPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"SongWalkerVSTi__";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Sampler,
        Vst3SubCategory::Stereo,
    ];
}
