use songwalker_core::compiler::{EventKind, EventList};

use super::slot::{EnvelopeParams, VoicePool};
use crate::transport::TransportState;

/// Default root note for runner instances (C4 = MIDI 60).
const DEFAULT_ROOT_NOTE: u8 = 60;

/// Maximum simultaneous runner instances (one per held MIDI note).
const MAX_RUNNER_INSTANCES: usize = 16;

/// State specific to a Runner-mode slot.
pub struct RunnerSlotState {
    /// The compiled `.sw` event list (None if no `.sw` is loaded or compilation failed).
    pub event_list: Option<EventList>,
    /// The `.sw` source code currently loaded in the editor.
    pub source_code: String,
    /// The root note for transposition (default C4).
    pub root_note: u8,
    /// Currently active runner instances.
    instances: Vec<RunnerInstance>,
    /// Compilation error message (if any).
    pub compile_error: Option<String>,
    /// Pitch bend from MIDI input.
    pub pitch_bend: f32,
    /// Envelope parameters for runner-triggered voices.
    envelope: EnvelopeParams,
}

impl Default for RunnerSlotState {
    fn default() -> Self {
        Self {
            event_list: None,
            source_code: String::new(),
            root_note: DEFAULT_ROOT_NOTE,
            instances: Vec::with_capacity(MAX_RUNNER_INSTANCES),
            compile_error: None,
            pitch_bend: 0.0,
            envelope: EnvelopeParams::default(),
        }
    }
}

impl RunnerSlotState {
    pub fn reset(&mut self) {
        self.instances.clear();
    }

    pub fn envelope(&self) -> EnvelopeParams {
        self.envelope
    }

    pub fn set_envelope(&mut self, env: EnvelopeParams) {
        self.envelope = env;
    }

    /// Compile `.sw` source code into an event list.
    pub fn compile(&mut self, source: &str) {
        self.source_code = source.to_string();
        match songwalker_core::parse(source) {
            Ok(program) => match songwalker_core::compiler::compile(&program) {
                Ok(event_list) => {
                    self.event_list = Some(event_list);
                    self.compile_error = None;
                }
                Err(e) => {
                    self.compile_error = Some(e);
                    self.event_list = None;
                }
            },
            Err(e) => {
                self.compile_error = Some(e.to_string());
                self.event_list = None;
            }
        }
    }

    /// Spawn a new runner instance triggered by a MIDI Note On.
    ///
    /// The instance is transposed by `(note - root_note)` semitones.
    /// Host transport values (BPM, time sig) are injected.
    pub fn spawn_instance(&mut self, note: u8, velocity: f32, transport: &TransportState) {
        if self.event_list.is_none() {
            return;
        }
        // Don't exceed max instances
        if self.instances.len() >= MAX_RUNNER_INSTANCES {
            return;
        }

        let transpose = note as i32 - self.root_note as i32;

        let instance = RunnerInstance {
            trigger_note: note,
            transpose,
            velocity,
            cursor: 0,
            position_beats: 0.0,
            _bpm: transport.bpm,
            active: true,
            releasing: false,
        };

        self.instances.push(instance);
    }

    /// Release the runner instance triggered by the given MIDI note.
    pub fn release_instance(&mut self, note: u8) {
        for instance in &mut self.instances {
            if instance.trigger_note == note && instance.active && !instance.releasing {
                instance.releasing = true;
            }
        }
    }

    /// Advance all active runner instances by the given number of samples.
    ///
    /// This fires events from the event list whose beat position falls within
    /// the current buffer window, allocating voices with the appropriate transpose.
    pub fn advance(
        &mut self,
        voice_pool: &mut VoicePool,
        num_samples: usize,
        sample_rate: f32,
        transport: &TransportState,
    ) {
        let event_list = match &self.event_list {
            Some(el) => el,
            None => return,
        };

        let events = &event_list.events;
        let beats_per_second = transport.bpm / 60.0;
        let beats_per_sample = beats_per_second / sample_rate as f64;
        let beat_advance = beats_per_sample * num_samples as f64;

        // Process each active instance
        let mut i = 0;
        while i < self.instances.len() {
            let instance = &mut self.instances[i];
            if !instance.active {
                self.instances.swap_remove(i);
                continue;
            }

            if instance.releasing {
                // When releasing, don't schedule new events; just let existing voices
                // finish their release. Mark instance inactive once cursor is past end.
                instance.active = false;
                self.instances.swap_remove(i);
                continue;
            }

            let start_beat = instance.position_beats;
            let end_beat = start_beat + beat_advance;

            // Fire events in the [start_beat, end_beat) window
            while instance.cursor < events.len() {
                let event = &events[instance.cursor];
                if event.time >= end_beat {
                    break;
                }
                if event.time >= start_beat {
                    match &event.kind {
                        EventKind::Note {
                            pitch,
                            velocity: note_vel,
                            gate: _,
                            ..
                        } => {
                            // Parse pitch string to MIDI note, apply transpose
                            if let Some(base_pitch) = parse_pitch(pitch) {
                                let transposed_pitch = (base_pitch as i32 + instance.transpose)
                                    .clamp(0, 127) as u8;
                                let vel = (*note_vel as f32) * instance.velocity;

                                if let Some(voice) = voice_pool.allocate(transposed_pitch, vel) {
                                    let freq = crate::midi::midi_to_freq(transposed_pitch);
                                    voice.phase_inc = freq as f64 / sample_rate as f64;
                                    voice.transpose = instance.transpose;
                                }
                            }
                        }
                        _ => {
                            // TrackStart, SetProperty, PresetRef handled at compile time
                        }
                    }
                }
                instance.cursor += 1;
            }

            instance.position_beats = end_beat;

            // If we've passed the end of the event list, loop or stop
            if instance.cursor >= events.len()
                && instance.position_beats >= event_list.total_beats
            {
                // Restart from beginning (loop the pattern)
                instance.cursor = 0;
                instance.position_beats = 0.0;
            }

            i += 1;
        }
    }
}

/// A single running instance of a `.sw` track, triggered by one MIDI note.
struct RunnerInstance {
    /// The MIDI note that triggered this instance.
    trigger_note: u8,
    /// Transpose offset in semitones (trigger_note - root_note).
    transpose: i32,
    /// Velocity of the triggering note (0.0–1.0).
    velocity: f32,
    /// Current position in the event list.
    cursor: usize,
    /// Current position in beats.
    position_beats: f64,
    /// BPM at the time of instance creation.
    _bpm: f64,
    /// Whether this instance is still active.
    active: bool,
    /// Whether this instance is releasing (Note Off received).
    releasing: bool,
}

/// Parse a pitch string like "C4", "D#5", "Eb3" to a MIDI note number.
fn parse_pitch(pitch: &str) -> Option<u8> {
    let chars: Vec<char> = pitch.chars().collect();
    if chars.is_empty() {
        return None;
    }

    let base = match chars[0] {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    let (accidental, rest_start) = if chars.len() > 1 {
        match chars[1] {
            '#' | 's' => (1i32, 2),
            'b' => (-1i32, 2),
            _ => (0i32, 1),
        }
    } else {
        (0i32, 1)
    };

    let octave_str: String = chars[rest_start..].iter().collect();
    let octave: i32 = octave_str.parse().ok()?;

    let midi = (octave + 1) * 12 + base + accidental;
    if midi >= 0 && midi <= 127 {
        Some(midi as u8)
    } else {
        None
    }
}
