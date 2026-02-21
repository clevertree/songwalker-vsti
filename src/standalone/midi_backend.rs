//! MIDI input backend using midir — supports runtime device enumeration and switching.

use crossbeam_channel::Sender;
use midir::{MidiInput, MidiInputConnection};
use nih_plug::prelude::NoteEvent;

/// Manages MIDI input connections.
pub struct MidiBackend {
    /// Active MIDI input connection (dropped to disconnect).
    connection: Option<MidiInputConnection<()>>,
    /// Channel to send parsed NoteEvents to the audio callback.
    midi_tx: Sender<NoteEvent<()>>,
}

impl MidiBackend {
    pub fn new(midi_tx: Sender<NoteEvent<()>>) -> Self {
        Self {
            connection: None,
            midi_tx,
        }
    }

    /// Enumerate available MIDI input ports.
    pub fn enumerate_inputs() -> Vec<String> {
        let Ok(midi_in) = MidiInput::new("SongWalker MIDI Probe") else {
            return Vec::new();
        };
        midi_in.ports().iter()
            .filter_map(|p| midi_in.port_name(p).ok())
            .collect()
    }

    /// Connect to a MIDI input port by name.
    pub fn connect(&mut self, port_name: &str) -> Result<(), String> {
        // Disconnect existing
        self.disconnect();

        let midi_in = MidiInput::new("SongWalker MIDI Input")
            .map_err(|e| format!("Failed to create MIDI input: {e}"))?;

        let port = midi_in.ports().into_iter()
            .find(|p| midi_in.port_name(p).as_deref() == Ok(port_name))
            .ok_or_else(|| format!("MIDI port '{}' not found", port_name))?;

        let tx = self.midi_tx.clone();

        let connection = midi_in.connect(
            &port,
            "SongWalker Input",
            move |_timestamp, data, _| {
                if let Some(event) = parse_midi_bytes(data) {
                    let _ = tx.try_send(event);
                }
            },
            (),
        ).map_err(|e| format!("Failed to connect MIDI: {e}"))?;

        log::info!("[MidiBackend] Connected to: {port_name}");
        self.connection = Some(connection);
        Ok(())
    }

    /// Disconnect the current MIDI input.
    pub fn disconnect(&mut self) {
        if let Some(conn) = self.connection.take() {
            conn.close();
            log::info!("[MidiBackend] Disconnected");
        }
    }
}

/// Parse raw MIDI bytes into a nih-plug NoteEvent.
fn parse_midi_bytes(data: &[u8]) -> Option<NoteEvent<()>> {
    if data.is_empty() {
        return None;
    }

    let status = data[0] & 0xF0;
    let channel = data[0] & 0x0F;

    match status {
        // Note Off
        0x80 if data.len() >= 3 => Some(NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel,
            note: data[1],
            velocity: data[2] as f32 / 127.0,
        }),
        // Note On
        0x90 if data.len() >= 3 => {
            let velocity = data[2] as f32 / 127.0;
            if velocity == 0.0 {
                // Note On with velocity 0 = Note Off
                Some(NoteEvent::NoteOff {
                    timing: 0,
                    voice_id: None,
                    channel,
                    note: data[1],
                    velocity: 0.0,
                })
            } else {
                Some(NoteEvent::NoteOn {
                    timing: 0,
                    voice_id: None,
                    channel,
                    note: data[1],
                    velocity,
                })
            }
        }
        // Polyphonic Aftertouch
        0xA0 if data.len() >= 3 => Some(NoteEvent::PolyPressure {
            timing: 0,
            voice_id: None,
            channel,
            note: data[1],
            pressure: data[2] as f32 / 127.0,
        }),
        // Control Change
        0xB0 if data.len() >= 3 => Some(NoteEvent::MidiCC {
            timing: 0,
            channel,
            cc: data[1],
            value: data[2] as f32 / 127.0,
        }),
        // Pitch Bend
        0xE0 if data.len() >= 3 => {
            let lsb = data[1] as u16;
            let msb = data[2] as u16;
            let value = ((msb << 7) | lsb) as f32 / 16383.0;
            Some(NoteEvent::MidiPitchBend {
                timing: 0,
                channel,
                value,
            })
        }
        // Channel Pressure
        0xD0 if data.len() >= 2 => Some(NoteEvent::MidiChannelPressure {
            timing: 0,
            channel,
            pressure: data[1] as f32 / 127.0,
        }),
        _ => None,
    }
}
