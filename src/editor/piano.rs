//! Interactive piano keyboard widget for the VSTi editor.
//!
//! Renders a 2-octave piano (C3–B4) at the bottom of the editor.
//! When a key is pressed, a NoteOn event is sent via crossbeam channel
//! to the audio thread, targeting the currently selected slot.

use nih_plug_egui::egui;
use std::collections::HashSet;

use super::colors;
use super::zs;
use super::EditorState;
use super::EditorEvent;

/// Persistent state for the piano keyboard.
pub struct PianoState {
    /// Whether the piano is visible (toggled via header button).
    pub visible: bool,
    /// Octave offset from default range (0 = C3–B4).
    pub octave_offset: i8,
    /// Currently depressed/active notes (for visual feedback).
    pub active_notes: HashSet<u8>,
    /// The last note triggered by mouse (for drag-across-keys).
    last_mouse_note: Option<u8>,
}

impl Default for PianoState {
    fn default() -> Self {
        Self {
            visible: false,
            octave_offset: 0,
            active_notes: HashSet::new(),
            last_mouse_note: None,
        }
    }
}

impl PianoState {
    /// Base MIDI note for the leftmost key.
    pub fn base_note(&self) -> u8 {
        (48_i8 + self.octave_offset * 12).clamp(0, 108) as u8
    }

    /// Range label (e.g., "C3–B4").
    pub fn range_label(&self) -> String {
        let base = self.base_note();
        let top = base + 23;
        format!("{}–{}", note_name(base), note_name(top))
    }
}

/// Number of white keys in 2 octaves (C to B × 2 = 14 white keys).
const NUM_WHITE_KEYS: usize = 14;
/// Total semitones in 2 octaves.
const NUM_SEMITONES: usize = 24;

/// Whether a semitone offset (0–11) within an octave is a black key.
const fn is_black_key(semitone: u8) -> bool {
    matches!(semitone % 12, 1 | 3 | 6 | 8 | 10)
}

/// Draw the piano keyboard panel.
pub fn draw(ui: &mut egui::Ui, state: &mut EditorState, z: f32) {
    let piano = &mut state.piano_state;
    let base_note = piano.base_note();

    ui.horizontal(|ui| {
        // Octave shift controls
        if ui
            .button(egui::RichText::new("◀").size(zs(12.0, z)))
            .on_hover_text("Shift down one octave")
            .clicked()
        {
            piano.octave_offset = (piano.octave_offset - 1).max(-4);
        }
        ui.label(
            egui::RichText::new(piano.range_label())
                .color(colors::SUBTEXT0)
                .size(zs(11.0, z))
                .family(egui::FontFamily::Monospace),
        );
        if ui
            .button(egui::RichText::new("▶").size(zs(12.0, z)))
            .on_hover_text("Shift up one octave")
            .clicked()
        {
            piano.octave_offset = (piano.octave_offset + 1).min(4);
        }
    });

    // Piano drawing area
    let desired_height = zs(70.0, z);
    let available_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, desired_height),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(rect);
    let white_key_width = rect.width() / NUM_WHITE_KEYS as f32;
    let black_key_width = white_key_width * 0.6;
    let black_key_height = rect.height() * 0.6;

    // Build layout: map each white key index to its screen rect
    let mut white_rects: Vec<(u8, egui::Rect)> = Vec::with_capacity(NUM_WHITE_KEYS);
    let mut black_rects: Vec<(u8, egui::Rect)> = Vec::with_capacity(10);

    let mut white_idx = 0;
    for i in 0..NUM_SEMITONES {
        let semitone = i as u8;
        let midi_note = base_note + semitone;
        if is_black_key(semitone) {
            // Black key is placed to the left of the current white key position
            let x = rect.left() + white_idx as f32 * white_key_width - black_key_width * 0.5;
            let key_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - black_key_height),
                egui::vec2(black_key_width, black_key_height),
            );
            black_rects.push((midi_note, key_rect));
        } else {
            let x = rect.left() + white_idx as f32 * white_key_width;
            let key_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(white_key_width, rect.height()),
            );
            white_rects.push((midi_note, key_rect));
            white_idx += 1;
        }
    }

    // Draw white keys
    for &(midi_note, key_rect) in &white_rects {
        let is_active = piano.active_notes.contains(&midi_note);
        let fill = if is_active { colors::BLUE } else { colors::TEXT };
        painter.rect_filled(key_rect, 0.0, fill);
        painter.rect_stroke(key_rect, 0.0, egui::Stroke::new(1.0, colors::SURFACE1), egui::StrokeKind::Outside);
    }

    // Draw black keys (on top of white)
    for &(midi_note, key_rect) in &black_rects {
        let is_active = piano.active_notes.contains(&midi_note);
        let fill = if is_active { colors::BLUE } else { colors::CRUST };
        painter.rect_filled(key_rect, 0.0, fill);
        painter.rect_stroke(key_rect, 0.0, egui::Stroke::new(1.0, colors::SURFACE0), egui::StrokeKind::Outside);
    }

    // --- Mouse interaction ---
    let pointer_pos = response.interact_pointer_pos();

    if let Some(pos) = pointer_pos {
        // Check black keys first (they overlap white keys)
        let hit_note = black_rects
            .iter()
            .find(|(_, r)| r.contains(pos))
            .or_else(|| white_rects.iter().find(|(_, r)| r.contains(pos)))
            .map(|&(note, _)| note);

        if response.drag_started() || response.clicked() {
            // New press
            if let Some(note) = hit_note {
                let slot_index = state.slot_rack_state.selected_slot;
                piano.active_notes.insert(note);
                piano.last_mouse_note = Some(note);
                let _ = state.event_tx.try_send(EditorEvent::NoteOn {
                    slot_index,
                    note,
                    velocity: 0.8,
                });
            }
        } else if response.dragged() {
            // Drag across keys
            if let Some(note) = hit_note {
                if piano.last_mouse_note != Some(note) {
                    // Release old note
                    if let Some(old_note) = piano.last_mouse_note {
                        let slot_index = state.slot_rack_state.selected_slot;
                        piano.active_notes.remove(&old_note);
                        let _ = state.event_tx.try_send(EditorEvent::NoteOff {
                            slot_index,
                            note: old_note,
                        });
                    }
                    // Press new note
                    let slot_index = state.slot_rack_state.selected_slot;
                    piano.active_notes.insert(note);
                    piano.last_mouse_note = Some(note);
                    let _ = state.event_tx.try_send(EditorEvent::NoteOn {
                        slot_index,
                        note,
                        velocity: 0.8,
                    });
                }
            }
        }
    }

    if response.drag_stopped() || (!response.dragged() && !response.is_pointer_button_down_on()) {
        // Release all active notes
        if let Some(note) = piano.last_mouse_note.take() {
            let slot_index = state.slot_rack_state.selected_slot;
            piano.active_notes.remove(&note);
            let _ = state.event_tx.try_send(EditorEvent::NoteOff {
                slot_index,
                note,
            });
        }
    }
}

/// Convert a MIDI note number to a name (e.g., 60 → "C4").
fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (note as i32 / 12) - 1;
    let name = NAMES[(note % 12) as usize];
    format!("{}{}", name, octave)
}
