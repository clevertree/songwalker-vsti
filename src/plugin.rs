use nih_plug::prelude::*;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;

use crossbeam_channel::{Receiver, Sender};

use crate::audio::AudioEngine;
use crate::editor;
use crate::editor::{EditorEvent, PresetLoadedEvent};
use crate::editor::visualizer::VisualizerState;
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
    /// Channel sender for editor events (note on/off).
    event_tx: Sender<EditorEvent>,
    /// Channel receiver drained on the audio thread each process block.
    event_rx: Receiver<EditorEvent>,
    /// Channel sender for loaded presets (editor/background → audio thread).
    preset_loaded_tx: Sender<PresetLoadedEvent>,
    /// Channel receiver for loaded presets (drained on audio thread).
    preset_loaded_rx: Receiver<PresetLoadedEvent>,
    /// Shared status text displayed in the editor footer.
    status_text: Arc<Mutex<String>>,
    /// Shared visualizer state (lock-free, fed from audio thread).
    visualizer_state: Arc<VisualizerState>,
    /// Live voice count (updated per process block, read by editor).
    voice_count: Arc<AtomicU32>,
    /// Sample rate provided by the host.
    sample_rate: f32,
}

impl Default for SongWalkerPlugin {
    fn default() -> Self {
        let params = Arc::new(SongWalkerParams::default());
        let (event_tx, event_rx) = crossbeam_channel::bounded(64);
        let (preset_loaded_tx, preset_loaded_rx) = crossbeam_channel::bounded(16);
        Self {
            params,
            audio_engine: AudioEngine::new(),
            slot_manager: SlotManager::new_empty(),
            preset_manager: Arc::new(Mutex::new(PresetManager::new())),
            transport: TransportState::default(),
            plugin_state: Arc::new(Mutex::new(PluginState::default())),
            event_tx,
            event_rx,
            preset_loaded_tx,
            preset_loaded_rx,
            status_text: Arc::new(Mutex::new(String::new())),
            visualizer_state: Arc::new(VisualizerState::new(512)),
            voice_count: Arc::new(AtomicU32::new(0)),
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
        let event_tx = self.event_tx.clone();
        let audio_preset_loaded_tx = self.preset_loaded_tx.clone();
        let (ui_preset_loaded_tx, ui_preset_loaded_rx) = crossbeam_channel::unbounded();
        let status_text = self.status_text.clone();
        let visualizer_state = self.visualizer_state.clone();
        let voice_count = self.voice_count.clone();
        editor::create(
            preset_manager,
            plugin_state,
            params,
            editor_state,
            event_tx,
            audio_preset_loaded_tx,
            ui_preset_loaded_tx,
            ui_preset_loaded_rx,
            status_text,
            visualizer_state,
            voice_count,
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        log::info!("SongWalkerPlugin::initialize() sample_rate={}", buffer_config.sample_rate);
        self.sample_rate = buffer_config.sample_rate;
        self.audio_engine
            .initialize(buffer_config.sample_rate, buffer_config.max_buffer_size as usize);
        self.slot_manager.initialize(buffer_config.sample_rate);
        
        // Ensure all slots are allocated now (not in process() which would crash)
        log::info!("SongWalkerPlugin::initialize() allocate_all");
        self.slot_manager.allocate_all();

        // Start background preset manager (fetches library indexes)
        log::info!("SongWalkerPlugin::initialize() background refresh start");
        let pm = self.preset_manager.clone();
        PresetManager::start_background_refresh(pm);

        log::info!("SongWalkerPlugin::initialize() success");
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

        // --- Drain loaded presets (background thread → audio thread) ---
        while let Ok(loaded) = self.preset_loaded_rx.try_recv() {
            // Index must be within pre-allocated bounds
            if loaded.slot_index < self.slot_manager.slot_count() {
                let slot = &mut self.slot_manager.slots_mut()[loaded.slot_index];
                slot.preset_state_mut()
                    .load_preset(loaded.preset_id, loaded.instance);

                // Optionally trigger a note-on immediately after loading (preview)
                if let Some(note) = loaded.play_note {
                    let note_event = NoteEvent::NoteOn {
                        timing: 0,
                        voice_id: None,
                        channel: 0,
                        note,
                        velocity: 0.8,
                    };
                    self.slot_manager.slots_mut()[loaded.slot_index]
                        .handle_midi_event(&note_event, &self.transport);
                }
            }
        }

        // --- Drain editor events (piano keys, stop-preview) ---
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                EditorEvent::NoteOn { slot_index, note, velocity } => {
                    if slot_index < self.slot_manager.slot_count() {
                        nih_plug::debug::nih_log!("[AudioThread] Received EditorEvent::NoteOn: note={} slot={}", note, slot_index);
                        let note_event = NoteEvent::NoteOn {
                            timing: 0,
                            voice_id: None,
                            channel: 0,
                            note,
                            velocity,
                        };
                        self.slot_manager.slots_mut()[slot_index]
                            .handle_midi_event(&note_event, &self.transport);
                    }
                }
                EditorEvent::NoteOff { slot_index, note } => {
                    if let Some(slot) = self.slot_manager.slots_mut().get_mut(slot_index) {
                        nih_plug::debug::nih_log!("[AudioThread] Received EditorEvent::NoteOff: note={} slot={}", note, slot_index);
                        let note_event = NoteEvent::NoteOff {
                            timing: 0,
                            voice_id: None,
                            channel: 0,
                            note,
                            velocity: 0.0,
                        };
                        slot.handle_midi_event(&note_event, &self.transport);
                    }
                }
                EditorEvent::StopPreview => {
                    nih_plug::debug::nih_log!("[AudioThread] Received EditorEvent::StopPreview");
                    // All-notes-off on all slots
                    for slot in self.slot_manager.slots_mut() {
                        let all_off = NoteEvent::MidiCC {
                            timing: 0,
                            channel: 0,
                            cc: 123,
                            value: 0.0,
                        };
                        slot.handle_midi_event(&all_off, &self.transport);
                    }
                }
            }
        }

        // Process all MIDI events and route to slots
        crate::audio::process_block(
            buffer,
            context,
            &mut self.audio_engine,
            &mut self.slot_manager,
            &self.transport,
            &self.params,
            &self.visualizer_state,
            &self.voice_count,
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
