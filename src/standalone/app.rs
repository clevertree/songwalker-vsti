//! Standalone eframe application — the main entry point for the standalone binary.
//!
//! Creates the audio backend (cpal), MIDI backend (midir), and eframe window,
//! wiring them together with the shared EditorState and channels.

use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU32;

use eframe::egui;
use nih_plug::prelude::NoteEvent;

use crate::editor;
use crate::editor::visualizer::VisualizerState;
use crate::editor::{DeviceState, EditorEvent, EditorState, EditorTab, PresetLoadedEvent};
use crate::preset::manager::PresetManager;
use crate::state::PluginState;

use super::audio_backend::AudioBackend;
use super::midi_backend::MidiBackend;
use super::params::{StandaloneGlobalParams, StandaloneParams};

/// Run the standalone application.
pub fn run() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([400.0, 300.0])
            .with_title("SongWalker"),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "SongWalker",
        options,
        Box::new(|cc| {
            // Apply theme on creation
            editor::apply_theme(&cc.egui_ctx);

            // Set window icon
            if let Ok(img) = image::load_from_memory_with_format(
                editor::ICON_PNG, image::ImageFormat::Png,
            ) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let icon_data = egui::IconData {
                    rgba: rgba.into_raw(),
                    width: w,
                    height: h,
                };
                cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Icon(
                    Some(Arc::new(icon_data)),
                ));
            }

            Ok(Box::new(StandaloneApp::new()))
        }),
    );
}

/// The standalone eframe application.
struct StandaloneApp {
    editor_state: EditorState,
    params: StandaloneParams,
    audio_backend: AudioBackend,
    midi_backend: MidiBackend,
    /// Whether the app has been initialized (first frame).
    initialized: bool,
}

impl StandaloneApp {
    fn new() -> Self {
        let params = StandaloneParams::default();

        // Create channels
        let (event_tx, event_rx) = crossbeam_channel::bounded::<EditorEvent>(64);
        let (audio_preset_loaded_tx, audio_preset_loaded_rx) =
            crossbeam_channel::bounded::<PresetLoadedEvent>(16);
        let (ui_preset_loaded_tx, ui_preset_loaded_rx) =
            crossbeam_channel::unbounded::<PresetLoadedEvent>();
        let (midi_tx, midi_rx) = crossbeam_channel::bounded::<NoteEvent<()>>(256);

        let visualizer_state = Arc::new(VisualizerState::new(512));
        let voice_count = Arc::new(AtomicU32::new(0));
        let preset_manager = Arc::new(Mutex::new(PresetManager::new()));
        let plugin_state = Arc::new(Mutex::new(PluginState::default()));
        let status_text = Arc::new(Mutex::new(String::new()));

        // Create audio backend
        let audio_backend = AudioBackend::new(
            48000.0,
            midi_rx,
            event_rx,
            audio_preset_loaded_rx,
            params.clone(),
            visualizer_state.clone(),
            voice_count.clone(),
        );

        // Create MIDI backend
        let midi_backend = MidiBackend::new(midi_tx);

        // Enumerate devices for the Settings UI
        let audio_devices = AudioBackend::enumerate_devices();
        let midi_devices = MidiBackend::enumerate_inputs();
        let audio_device_names: Vec<String> = audio_devices.iter().map(|d| d.name.clone()).collect();

        let device_state = DeviceState {
            audio_device_names,
            selected_audio_idx: 0,
            midi_input_names: midi_devices,
            selected_midi_idx: None,
            pending_audio_switch: None,
            pending_midi_switch: None,
            needs_refresh: false,
        };

        let editor_state = EditorState {
            egui_state: None, // standalone — no nih-plug EguiState
            preset_manager: preset_manager.clone(),
            plugin_state,
            current_tab: EditorTab::SlotRack,
            browser_state: editor::browser::BrowserState::default(),
            slot_rack_state: editor::slot_rack::SlotRackState::default(),
            piano_state: editor::piano::PianoState::default(),
            event_tx,
            audio_preset_loaded_tx: audio_preset_loaded_tx.clone(),
            ui_preset_loaded_tx,
            ui_preset_loaded_rx,
            status_text,
            visualizer_state,
            voice_count,
            zoom_level: 1.0,
            resize_drag_start: None,
            active_presets_ui: std::collections::HashMap::new(),
            device_state: Some(Box::new(device_state)),
        };

        // Start background preset refresh
        PresetManager::start_background_refresh(preset_manager);

        Self {
            editor_state,
            params,
            audio_backend,
            midi_backend,
            initialized: false,
        }
    }

    /// Start audio on the default device (called on first frame).
    fn initialize_audio(&mut self) {
        match self.audio_backend.start_default() {
            Ok(name) => {
                log::info!("[Standalone] Audio started on: {name}");
                // Update selected device in UI
                if let Some(ref mut ds) = self.editor_state.device_state {
                    if let Some(idx) = ds.audio_device_names.iter().position(|n| n == &name) {
                        ds.selected_audio_idx = idx;
                    }
                }
            }
            Err(e) => {
                log::error!("[Standalone] Failed to start audio: {e}");
                if let Ok(mut s) = self.editor_state.status_text.lock() {
                    *s = format!("⚠ Audio error: {e}");
                }
            }
        }
    }

    /// Handle pending device switch commands from the Settings UI.
    fn handle_device_commands(&mut self) {
        let (audio_switch, midi_switch, needs_refresh) = {
            let Some(ref mut ds) = self.editor_state.device_state else { return };
            (
                ds.pending_audio_switch.take(),
                ds.pending_midi_switch.take(),
                std::mem::replace(&mut ds.needs_refresh, false),
            )
        };

        if let Some(device_name) = audio_switch {
            match self.audio_backend.switch_device(&device_name) {
                Ok(()) => {
                    log::info!("[Standalone] Switched audio to: {device_name}");
                    if let Ok(mut s) = self.editor_state.status_text.lock() {
                        *s = format!("Audio: {device_name}");
                    }
                }
                Err(e) => {
                    log::error!("[Standalone] Audio switch failed: {e}");
                    if let Ok(mut s) = self.editor_state.status_text.lock() {
                        *s = format!("⚠ {e}");
                    }
                }
            }
        }

        if let Some(ref device_name) = midi_switch {
            if device_name.is_empty() {
                self.midi_backend.disconnect();
            } else {
                match self.midi_backend.connect(device_name) {
                    Ok(()) => {
                        log::info!("[Standalone] MIDI connected: {device_name}");
                    }
                    Err(e) => {
                        log::error!("[Standalone] MIDI connect failed: {e}");
                        if let Ok(mut s) = self.editor_state.status_text.lock() {
                            *s = format!("⚠ MIDI: {e}");
                        }
                    }
                }
            }
        }

        if needs_refresh {
            let audio_devices = AudioBackend::enumerate_devices();
            let midi_devices = MidiBackend::enumerate_inputs();
            if let Some(ref mut ds) = self.editor_state.device_state {
                ds.audio_device_names = audio_devices.iter().map(|d| d.name.clone()).collect();
                ds.midi_input_names = midi_devices;
            }
        }
    }
}

impl eframe::App for StandaloneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // First-frame initialization
        if !self.initialized {
            self.initialized = true;
            self.initialize_audio();
        }

        // Drain UI preset loaded events is done inside draw_editor()
        // (it stores in active_presets_ui and forwards to audio thread)

        // Draw the shared editor UI
        let gp = StandaloneGlobalParams { params: &self.params };
        editor::draw_editor(ctx, &gp, &mut self.editor_state);

        // Handle device switch commands after drawing
        self.handle_device_commands();
    }
}
