//! Audio backend using cpal — supports runtime device enumeration and switching.

use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use nih_plug::prelude::NoteEvent;

use crate::audio::{self, AudioEngine};
use crate::editor::visualizer::VisualizerState;
use crate::editor::{EditorEvent, PresetLoadedEvent};
use crate::slots::SlotManager;
use crate::transport::TransportState;

use super::params::StandaloneParams;

/// All mutable state needed by the audio callback.
/// Protected by parking_lot::Mutex for lock-free try_lock in the callback.
pub struct AudioCallbackState {
    pub engine: AudioEngine,
    pub slot_manager: SlotManager,
    pub transport: TransportState,
}

/// Manages the cpal audio output stream and device switching.
pub struct AudioBackend {
    /// Shared audio state — the callback uses try_lock(), device switch takes full lock.
    pub callback_state: Arc<parking_lot::Mutex<AudioCallbackState>>,
    /// Current audio output stream (dropped to stop, recreated to switch devices).
    stream: Option<cpal::Stream>,
    /// Channels drained by the audio callback.
    midi_rx: Receiver<NoteEvent<()>>,
    event_rx: Receiver<EditorEvent>,
    preset_loaded_rx: Receiver<PresetLoadedEvent>,
    /// Parameter atomics read by the audio callback.
    params: StandaloneParams,
    /// Visualizer state (lock-free), fed from the audio callback.
    visualizer_state: Arc<VisualizerState>,
    /// Voice count, updated from the audio callback.
    voice_count: Arc<AtomicU32>,
}

/// Information about an available audio device.
#[derive(Clone, Debug)]
pub struct AudioDeviceInfo {
    pub name: String,
}

impl AudioBackend {
    /// Create a new audio backend (no stream started yet).
    pub fn new(
        sample_rate: f32,
        midi_rx: Receiver<NoteEvent<()>>,
        event_rx: Receiver<EditorEvent>,
        preset_loaded_rx: Receiver<PresetLoadedEvent>,
        params: StandaloneParams,
        visualizer_state: Arc<VisualizerState>,
        voice_count: Arc<AtomicU32>,
    ) -> Self {
        let mut engine = AudioEngine::new();
        engine.initialize(sample_rate, 1024);

        let mut slot_manager = SlotManager::new_empty();
        slot_manager.initialize(sample_rate);
        slot_manager.allocate_all();

        let callback_state = Arc::new(parking_lot::Mutex::new(AudioCallbackState {
            engine,
            slot_manager,
            transport: TransportState::default(),
        }));

        Self {
            callback_state,
            stream: None,
            midi_rx,
            event_rx,
            preset_loaded_rx,
            params,
            visualizer_state,
            voice_count,
        }
    }

    /// Enumerate available output devices.
    pub fn enumerate_devices() -> Vec<AudioDeviceInfo> {
        let host = cpal::default_host();
        let mut devices = Vec::new();
        if let Ok(output_devices) = host.output_devices() {
            for device in output_devices {
                if let Ok(name) = device.name() {
                    devices.push(AudioDeviceInfo { name });
                }
            }
        }
        devices
    }

    /// Start audio output on the default device.
    pub fn start_default(&mut self) -> Result<String, String> {
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| "No default audio output device available".to_string())?;
        let name = device.name().unwrap_or_else(|_| "Unknown".into());
        self.start_device(&device)?;
        Ok(name)
    }

    /// Start audio output on a named device.
    pub fn start_named(&mut self, device_name: &str) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host.output_devices()
            .map_err(|e| format!("Failed to enumerate output devices: {e}"))?
            .find(|d| d.name().as_deref().map(|n| n == device_name).unwrap_or(false))
            .ok_or_else(|| format!("Audio device '{}' not found", device_name))?;
        self.start_device(&device)
    }

    /// Switch to a different audio device at runtime.
    pub fn switch_device(&mut self, device_name: &str) -> Result<(), String> {
        // Drop old stream first (callback stops)
        self.stream = None;
        self.start_named(device_name)
    }

    /// Start the audio stream on a specific cpal device.
    fn start_device(&mut self, device: &cpal::Device) -> Result<(), String> {
        // Query supported config
        let supported = device.default_output_config()
            .map_err(|e| format!("No supported output config: {e}"))?;

        let sample_rate = supported.sample_rate().0;
        let channels = 2u16; // We always want stereo

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Re-initialize engine with the device's sample rate
        {
            let mut state = self.callback_state.lock();
            state.engine.initialize(sample_rate as f32, 2048);
            state.slot_manager.initialize(sample_rate as f32);
            state.transport.sample_rate = sample_rate as f32;
        }

        // Clone everything needed by the callback closure
        let callback_state = self.callback_state.clone();
        let midi_rx = self.midi_rx.clone();
        let event_rx = self.event_rx.clone();
        let preset_loaded_rx = self.preset_loaded_rx.clone();
        let params = self.params.clone();
        let visualizer_state = self.visualizer_state.clone();
        let voice_count = self.voice_count.clone();
        let ch = channels as usize;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                // Try to lock — if UI is switching devices, output silence
                let Some(mut guard) = callback_state.try_lock() else {
                    data.fill(0.0);
                    return;
                };

                let num_frames = data.len() / ch;
                if num_frames == 0 {
                    return;
                }

                // Destructure to get independent borrows of each field
                let AudioCallbackState {
                    ref mut engine,
                    ref mut slot_manager,
                    ref transport,
                } = *guard;

                // Drain loaded presets
                while let Ok(loaded) = preset_loaded_rx.try_recv() {
                    log::info!("[AudioCB] Preset loaded: preset={}, slot={}, play_note={:?}, zones={}",
                        loaded.preset_id, loaded.slot_index, loaded.play_note, loaded.instance.zones.len());
                    if loaded.slot_index < slot_manager.slot_count() {
                        {
                            let slot = &mut slot_manager.slots_mut()[loaded.slot_index];
                            // Kill any voices still playing the old preset on this slot
                            // (their zone_index references would be stale after replacing the preset)
                            slot.voice_pool_mut().kill_all();
                            slot.preset_state_mut()
                                .load_preset(loaded.preset_id.clone(), loaded.instance.clone());
                        }
                        if let Some(note) = loaded.play_note {
                            let note_event = NoteEvent::NoteOn {
                                timing: 0, voice_id: None, channel: 0,
                                note, velocity: 0.8,
                            };
                            slot_manager.slots_mut()[loaded.slot_index]
                                .handle_midi_event(&note_event, transport);
                        }
                    } else {
                        log::warn!("[AudioCB] slot_index {} >= slot_count {}", loaded.slot_index, slot_manager.slot_count());
                    }
                }

                // Drain MIDI events from hardware
                while let Ok(event) = midi_rx.try_recv() {
                    crate::midi::route_event(&event, slot_manager, transport);
                }

                // Drain editor events (piano keys, stop preview)
                while let Ok(event) = event_rx.try_recv() {
                    match event {
                        EditorEvent::NoteOn { slot_index, note, velocity } => {
                            if slot_index < slot_manager.slot_count() {
                                let note_event = NoteEvent::NoteOn {
                                    timing: 0, voice_id: None, channel: 0,
                                    note, velocity,
                                };
                                slot_manager.slots_mut()[slot_index]
                                    .handle_midi_event(&note_event, transport);
                            }
                        }
                        EditorEvent::NoteOff { slot_index, note } => {
                            if let Some(slot) = slot_manager.slots_mut().get_mut(slot_index) {
                                let note_event = NoteEvent::NoteOff {
                                    timing: 0, voice_id: None, channel: 0,
                                    note, velocity: 0.0,
                                };
                                slot.handle_midi_event(&note_event, transport);
                            }
                        }
                        EditorEvent::StopPreview => {
                            for slot in slot_manager.slots_mut() {
                                let all_off = NoteEvent::MidiCC {
                                    timing: 0, channel: 0, cc: 123, value: 0.0,
                                };
                                slot.handle_midi_event(&all_off, transport);
                            }
                        }
                    }
                }

                // Render and mix in chunks (cpal buffer may exceed engine capacity)
                let master_gain = params.master_volume_gain_value();
                let master_pan = params.master_pan_value();
                let max_chunk = engine.max_buffer_size();
                let mut offset = 0;

                while offset < num_frames {
                    let chunk = (num_frames - offset).min(max_chunk);
                    audio::render_and_mix(
                        chunk,
                        engine,
                        slot_manager,
                        transport,
                        master_gain,
                        master_pan,
                        &visualizer_state,
                        &voice_count,
                    );

                    // Interleave this chunk into the cpal output buffer
                    for i in 0..chunk {
                        let out_idx = (offset + i) * ch;
                        data[out_idx] = engine.output_left[i];
                        if ch > 1 {
                            data[out_idx + 1] = engine.output_right[i];
                        }
                    }

                    offset += chunk;
                }
            },
            |err| {
                log::error!("[AudioBackend] Stream error: {err}");
            },
            None, // no timeout
        ).map_err(|e| format!("Failed to build output stream: {e}"))?;

        stream.play().map_err(|e| format!("Failed to start playback: {e}"))?;

        log::info!("[AudioBackend] Stream started: {}Hz, {} channels",
            sample_rate, channels);

        self.stream = Some(stream);
        Ok(())
    }
}
