use nih_plug::prelude::*;

use crate::slots::SlotManager;
use crate::transport::TransportState;

/// Route a MIDI event from the host to the appropriate slot(s).
///
/// Events are routed based on each slot's MIDI channel setting:
/// - Channel 0 = receive all channels
/// - Channel 1–16 = receive only that channel
pub fn route_event(
    event: &NoteEvent<()>,
    slot_manager: &mut SlotManager,
    transport: &TransportState,
) {
    let channel = event_channel(event);

    for slot in slot_manager.slots_mut().iter_mut() {
        let slot_ch = slot.midi_channel();
        // Channel 0 means "all", otherwise must match
        if slot_ch == 0 || slot_ch == (channel as i32 + 1) {
            slot.handle_midi_event(event, transport);
        }
    }
}

/// Extract the MIDI channel (0–15) from a NoteEvent.
fn event_channel(event: &NoteEvent<()>) -> u8 {
    match event {
        NoteEvent::NoteOn { channel, .. } => *channel,
        NoteEvent::NoteOff { channel, .. } => *channel,
        NoteEvent::PolyPressure { channel, .. } => *channel,
        NoteEvent::MidiCC { channel, .. } => *channel,
        NoteEvent::MidiPitchBend { channel, .. } => *channel,
        NoteEvent::MidiChannelPressure { channel, .. } => *channel,
        _ => 0,
    }
}

/// Convert a MIDI note number (0–127) to frequency in Hz (A4 = 440 Hz).
#[inline]
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

/// Convert a MIDI velocity (0–127) to a normalized float (0.0–1.0).
#[inline]
pub fn velocity_to_float(velocity: f32) -> f32 {
    velocity.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_freq_a4() {
        let freq = midi_to_freq(69);
        assert!((freq - 440.0).abs() < 0.01, "A4 should be 440 Hz, got {}", freq);
    }

    #[test]
    fn test_midi_to_freq_c4() {
        let freq = midi_to_freq(60);
        assert!((freq - 261.63).abs() < 0.1, "C4 should be ~261.63 Hz, got {}", freq);
    }

    #[test]
    fn test_midi_to_freq_boundaries() {
        assert!(midi_to_freq(0) > 0.0, "Note 0 should have positive frequency");
        assert!(midi_to_freq(127) < 20000.0, "Note 127 should be < 20kHz");
        // Octave relationship: note+12 should be double the frequency
        let f60 = midi_to_freq(60);
        let f72 = midi_to_freq(72);
        assert!((f72 / f60 - 2.0).abs() < 0.01, "Octave should double frequency");
    }

    #[test]
    fn test_velocity_to_float_normal() {
        assert_eq!(velocity_to_float(0.0), 0.0);
        assert_eq!(velocity_to_float(0.5), 0.5);
        assert_eq!(velocity_to_float(1.0), 1.0);
    }

    #[test]
    fn test_velocity_to_float_clamp() {
        assert_eq!(velocity_to_float(-0.5), 0.0);
        assert_eq!(velocity_to_float(1.5), 1.0);
    }

    #[test]
    fn test_event_channel_note_on() {
        let event = NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 5,
            note: 60,
            velocity: 0.8,
        };
        assert_eq!(event_channel(&event), 5);
    }

    #[test]
    fn test_event_channel_note_off() {
        let event = NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel: 10,
            note: 60,
            velocity: 0.0,
        };
        assert_eq!(event_channel(&event), 10);
    }

    #[test]
    fn test_event_channel_cc() {
        let event = NoteEvent::MidiCC {
            timing: 0,
            channel: 3,
            cc: 1,
            value: 0.5,
        };
        assert_eq!(event_channel(&event), 3);
    }
}
