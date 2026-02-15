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
