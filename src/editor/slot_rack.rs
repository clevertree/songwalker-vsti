use nih_plug_egui::egui;

use super::colors;
use super::zs;
use super::EditorState;
use crate::state::SlotConfig;

/// Persistent state for the slot rack UI.
#[derive(Default)]
pub struct SlotRackState {
    /// Currently selected/focused slot index.
    pub selected_slot: usize,
    /// Whether the code editor is expanded for the selected slot.
    pub editor_expanded: bool,
}

/// Draw the Kontakt-style slot rack.
pub fn draw(ui: &mut egui::Ui, state: &mut EditorState, z: f32) {
    ui.set_clip_rect(ui.max_rect());
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(zs(6.0, z), zs(4.0, z));

        // Header with "Add Slot" button
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Slot Rack")
                    .color(colors::TEXT)
                    .strong()
                    .size(zs(14.0, z)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(egui::RichText::new("+ Add Slot").color(colors::GREEN).size(zs(12.0, z)))
                    .clicked()
                {
                    if let Ok(mut ps) = state.plugin_state.lock() {
                        ps.add_slot_config(SlotConfig::default());
                    }
                }
            });
        });

        ui.separator();

        // Slot list
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let slot_count = if let Ok(ps) = state.plugin_state.lock() {
                    ps.slot_configs.len()
                } else {
                    0
                };

                for idx in 0..slot_count {
                    let is_selected = state.slot_rack_state.selected_slot == idx;

                    egui::Frame::NONE
                        .fill(if is_selected {
                            colors::MANTLE
                        } else {
                            colors::CRUST
                        })
                        .inner_margin(egui::Margin::symmetric(zs(10.0, z) as i8, zs(6.0, z) as i8))
                        .outer_margin(egui::Margin::symmetric(0, 1))
                        .corner_radius(zs(4.0, z))
                        .stroke(egui::Stroke::new(
                            1.0,
                            if is_selected {
                                colors::BLUE
                            } else {
                                colors::SURFACE0
                            },
                        ))
                        .show(ui, |ui| {
                            draw_slot_strip(ui, state, idx, z);
                        });
                }

                if slot_count == 0 {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(
                                "No slots. Click '+ Add Slot' to get started.",
                            )
                            .color(colors::OVERLAY0)
                            .italics(),
                        );
                    });
                }
            });
    });
}

/// Draw a single slot strip (one row in the rack).
fn draw_slot_strip(ui: &mut egui::Ui, state: &mut EditorState, idx: usize, z: f32) {
    let slot_config = if let Ok(ps) = state.plugin_state.lock() {
        ps.slot_configs.get(idx).cloned()
    } else {
        return;
    };

    let Some(config) = slot_config else { return };

    // Constrain to available width
    let max_width = ui.available_width();
    ui.set_max_width(max_width);

    // Click to select
    let response = ui
        .horizontal(|ui| {
            // Slot number
            ui.label(
                egui::RichText::new(format!("{}.", idx + 1))
                    .color(colors::OVERLAY0)
                    .strong()
                    .size(zs(12.0, z)),
            );

            // Slot name / preset name
            let name = if let Some(ref preset_id) = config.preset_id {
                preset_id.clone()
            } else if !config.source_code.is_empty() {
                "Source".to_string()
            } else {
                "Empty".to_string()
            };

            ui.label(egui::RichText::new(&name).color(colors::TEXT).strong().size(zs(13.0, z)));

            // MIDI channel
            let ch_text = if config.midi_channel == 0 {
                "All".to_string()
            } else {
                format!("Ch:{}", config.midi_channel)
            };
            ui.label(egui::RichText::new(ch_text).color(colors::SUBTEXT0).size(zs(10.0, z)));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Remove button
                if ui
                    .button(egui::RichText::new("\u{2715}").color(colors::RED).size(zs(11.0, z)))
                    .clicked()
                {
                    if let Ok(mut ps) = state.plugin_state.lock() {
                        ps.remove_slot_config(idx);
                    }
                }

                // Solo button
                let solo_color = if config.solo {
                    colors::YELLOW
                } else {
                    colors::OVERLAY0
                };
                if ui
                    .button(egui::RichText::new("S").color(solo_color).size(zs(11.0, z)))
                    .clicked()
                {
                    if let Ok(mut ps) = state.plugin_state.lock() {
                        if let Some(cfg) = ps.slot_configs.get_mut(idx) {
                            cfg.solo = !cfg.solo;
                        }
                    }
                }

                // Mute button
                let mute_color = if config.muted {
                    colors::RED
                } else {
                    colors::OVERLAY0
                };
                if ui
                    .button(egui::RichText::new("M").color(mute_color).size(zs(11.0, z)))
                    .clicked()
                {
                    if let Ok(mut ps) = state.plugin_state.lock() {
                        if let Some(cfg) = ps.slot_configs.get_mut(idx) {
                            cfg.muted = !cfg.muted;
                        }
                    }
                }
            });
        })
        .response;

    if response.clicked() {
        state.slot_rack_state.selected_slot = idx;
    }

    // --- Expanded controls for selected slot ---
    if state.slot_rack_state.selected_slot == idx {
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Vol:").color(colors::SUBTEXT0).size(zs(11.0, z)));
            let mut vol = config.volume;
            if ui
                .add(egui::Slider::new(&mut vol, 0.0..=1.5).show_value(false))
                .changed()
            {
                if let Ok(mut ps) = state.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(idx) {
                        cfg.volume = vol;
                    }
                }
            }

            ui.label(egui::RichText::new("Pan:").color(colors::SUBTEXT0).size(zs(11.0, z)));
            let mut pan = config.pan;
            if ui
                .add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false))
                .changed()
            {
                if let Ok(mut ps) = state.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(idx) {
                        cfg.pan = pan;
                    }
                }
            }
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Root Note:").color(colors::SUBTEXT0).size(11.0));
            let mut root = config.root_note as i32;
            if ui
                .add(egui::Slider::new(&mut root, 0..=127))
                .changed()
            {
                if let Ok(mut ps) = state.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(idx) {
                        cfg.root_note = root as u8;
                    }
                }
            }
            ui.label(
                egui::RichText::new(note_name(config.root_note))
                    .color(colors::TEAL)
                    .size(zs(11.0, z)),
            );
        });

        // Code editor (always available, like the web editor)
        let mut source = config.source_code.clone();
        let response = ui.add(
            egui::TextEdit::multiline(&mut source)
                .font(egui::TextStyle::Monospace)
                .desired_rows(6)
                .desired_width(ui.available_width())
                .code_editor(),
        );

        if response.changed() {
            if let Ok(mut ps) = state.plugin_state.lock() {
                if let Some(cfg) = ps.slot_configs.get_mut(idx) {
                    cfg.source_code = source;
                }
            }
        }

        // Show compile error if any
        if let Some(ref err) = config.compile_error {
            ui.label(egui::RichText::new(err).color(colors::RED).size(zs(11.0, z)));
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
