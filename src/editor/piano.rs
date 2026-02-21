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
        // Use i16 to avoid i8 overflow on extreme octave offsets
        let note = 48_i16 + self.octave_offset as i16 * 12;
        note.clamp(0, 108) as u8
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

        ui.add_space(zs(12.0, z));

        // Display current playing slot
        let slot_index = state.slot_rack_state.selected_slot;
        let slot_name = if let Ok(ps) = state.plugin_state.lock() {
            ps.slot_configs.get(slot_index).map(|c| {
                if let Some(ref pid) = c.preset_id {
                    pid.clone()
                } else if !c.source_code.is_empty() {
                    "Source".to_string()
                } else {
                    "Empty".to_string()
                }
            }).unwrap_or_else(|| "None".to_string())
        } else {
            "???".to_string()
        };

        ui.label(
            egui::RichText::new(format!("Playing Slot {}: {}", slot_index + 1, slot_name))
                .color(colors::TEAL)
                .size(zs(11.0, z)),
        );
    });

    // Piano drawing area — use available_width() to get the actual remaining
    // visible width at the current cursor position (after horizontal controls).
    let desired_height = zs(70.0, z);
    let available_w = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_w, desired_height),
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
            // Black key is placed centered on the boundary after current white_idx-1
            // Positioned at the BOTTOM of the piano area (hanging from the bottom edge)
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
        // Use darker base for black keys to contrast with CRUST panel background
        let fill = if is_active { colors::BLUE } else { colors::BASE };
        painter.rect_filled(key_rect, 0.0, fill);
        painter.rect_stroke(key_rect, 0.0, egui::Stroke::new(1.0, colors::CRUST), egui::StrokeKind::Outside);
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
                nih_plug::debug::nih_log!("[Piano] NoteOn: note={} slot={} vel=0.8", note, slot_index);
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
                        nih_plug::debug::nih_log!("[Piano] NoteOff (gliss): note={} slot={}", old_note, slot_index);
                        piano.active_notes.remove(&old_note);
                        let _ = state.event_tx.try_send(EditorEvent::NoteOff {
                            slot_index,
                            note: old_note,
                        });
                    }
                    // Press new note
                    let slot_index = state.slot_rack_state.selected_slot;
                    nih_plug::debug::nih_log!("[Piano] NoteOn (gliss): note={} slot={} vel=0.8", note, slot_index);
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
        // Release all active notes to avoid stuck keys
        for note in piano.active_notes.drain().collect::<Vec<_>>() {
            let slot_index = state.slot_rack_state.selected_slot;
            nih_plug::debug::nih_log!("[Piano] NoteOff (stop): note={} slot={}", note, slot_index);
            let _ = state.event_tx.try_send(EditorEvent::NoteOff {
                slot_index,
                note,
            });
        }
        piano.last_mouse_note = None;
    }
}

/// Convert a MIDI note number to a name (e.g., 60 → "C4").
pub fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (note as i32 / 12) - 1;
    let name = NAMES[(note % 12) as usize];
    format!("{}{}", name, octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_black_key_sharps() {
        // C#=1, D#=3, F#=6, G#=8, A#=10 are black
        assert!(is_black_key(1));
        assert!(is_black_key(3));
        assert!(is_black_key(6));
        assert!(is_black_key(8));
        assert!(is_black_key(10));
    }

    #[test]
    fn test_is_white_key_naturals() {
        // C=0, D=2, E=4, F=5, G=7, A=9, B=11 are white
        assert!(!is_black_key(0));
        assert!(!is_black_key(2));
        assert!(!is_black_key(4));
        assert!(!is_black_key(5));
        assert!(!is_black_key(7));
        assert!(!is_black_key(9));
        assert!(!is_black_key(11));
    }

    #[test]
    fn test_is_black_key_wraps_octave() {
        // Semitone 13 = C# in second octave
        assert!(is_black_key(13));
        // Semitone 12 = C in second octave
        assert!(!is_black_key(12));
    }

    #[test]
    fn test_note_name_c4() {
        assert_eq!(note_name(60), "C4");
    }

    #[test]
    fn test_note_name_a4() {
        assert_eq!(note_name(69), "A4");
    }

    #[test]
    fn test_note_name_boundaries() {
        assert_eq!(note_name(0), "C-1");
        assert_eq!(note_name(127), "G9");
        assert_eq!(note_name(21), "A0");  // Low A on piano
    }

    #[test]
    fn test_base_note_default() {
        let state = PianoState::default();
        assert_eq!(state.base_note(), 48); // C3
    }

    #[test]
    fn test_base_note_offset() {
        let mut state = PianoState::default();
        state.octave_offset = 1;
        assert_eq!(state.base_note(), 60); // C4
        state.octave_offset = -1;
        assert_eq!(state.base_note(), 36); // C2
    }

    #[test]
    fn test_base_note_clamping() {
        let mut state = PianoState::default();
        state.octave_offset = -10; // Would be 48 - 120 = -72
        assert_eq!(state.base_note(), 0); // Clamps to 0
        state.octave_offset = 10; // Would be 48 + 120 = 168
        assert_eq!(state.base_note(), 108); // Clamps to 108
    }

    #[test]
    fn test_range_label() {
        let state = PianoState::default();
        assert_eq!(state.range_label(), "C3–B4");
    }

    #[test]
    fn test_range_label_shifted() {
        let mut state = PianoState::default();
        state.octave_offset = 1;
        assert_eq!(state.range_label(), "C4–B5");
    }

    #[test]
    fn test_white_key_count() {
        // 2 octaves should have exactly 14 white keys
        let mut white_count = 0;
        for i in 0..NUM_SEMITONES {
            if !is_black_key(i as u8) {
                white_count += 1;
            }
        }
        assert_eq!(white_count, NUM_WHITE_KEYS);
    }

    #[test]
    fn test_black_key_count() {
        // 2 octaves should have exactly 10 black keys
        let mut black_count = 0;
        for i in 0..NUM_SEMITONES {
            if is_black_key(i as u8) {
                black_count += 1;
            }
        }
        assert_eq!(black_count, 10);
    }
}
