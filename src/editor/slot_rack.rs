use nih_plug_egui::egui;

use super::colors;
use super::EditorState;
use crate::state::{SlotConfig, SlotMode};

/// Persistent state for the slot rack UI.
#[derive(Default)]
pub struct SlotRackState {
    /// Currently selected/focused slot index.
    pub selected_slot: usize,
    /// Whether the code editor is expanded for a runner slot.
    pub editor_expanded: bool,
}

/// Draw the Kontakt-style slot rack.
pub fn draw(ui: &mut egui::Ui, state: &mut EditorState) {
    ui.vertical(|ui| {
        // Header with "Add Slot" button
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Slot Rack").color(colors::TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(egui::RichText::new("+ Add Preset Slot").color(colors::GREEN))
                    .clicked()
                {
                    if let Ok(mut ps) = state.plugin_state.lock() {
                        ps.add_slot_config(SlotConfig {
                            mode: SlotMode::Preset,
                            ..SlotConfig::default()
                        });
                    }
                }
                if ui
                    .button(egui::RichText::new("+ Add Runner Slot").color(colors::TEAL))
                    .clicked()
                {
                    if let Ok(mut ps) = state.plugin_state.lock() {
                        ps.add_slot_config(SlotConfig {
                            mode: SlotMode::Runner,
                            ..SlotConfig::default()
                        });
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
                            colors::SURFACE0
                        } else {
                            colors::MANTLE
                        })
                        .inner_margin(8.0)
                        .outer_margin(egui::Margin::symmetric(0, 2))
                        .corner_radius(4.0)
                        .stroke(egui::Stroke::new(
                            1.0,
                            if is_selected {
                                colors::BLUE
                            } else {
                                colors::SURFACE1
                            },
                        ))
                        .show(ui, |ui| {
                            draw_slot_strip(ui, state, idx);
                        });
                }

                if slot_count == 0 {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(
                                "No slots. Click '+ Add Preset Slot' to get started.",
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
fn draw_slot_strip(ui: &mut egui::Ui, state: &mut EditorState, idx: usize) {
    let slot_config = if let Ok(ps) = state.plugin_state.lock() {
        ps.slot_configs.get(idx).cloned()
    } else {
        return;
    };

    let Some(config) = slot_config else { return };

    // Click to select
    let response = ui
        .horizontal(|ui| {
            // Slot number
            ui.label(
                egui::RichText::new(format!("{}.", idx + 1))
                    .color(colors::OVERLAY0)
                    .strong(),
            );

            // Slot name / preset name
            let name = if config.preset_id.is_some() {
                config.preset_id.as_ref().unwrap().clone()
            } else if config.mode == SlotMode::Runner {
                "Runner".to_string()
            } else {
                "Empty".to_string()
            };

            ui.label(egui::RichText::new(&name).color(colors::TEXT).strong());

            // Mode badge
            let (mode_text, mode_color) = match config.mode {
                SlotMode::Preset => ("Preset", colors::BLUE),
                SlotMode::Runner => ("Runner", colors::TEAL),
            };
            ui.label(
                egui::RichText::new(format!("[{}]", mode_text))
                    .color(mode_color)
                    .small(),
            );

            // MIDI channel
            let ch_text = if config.midi_channel == 0 {
                "All".to_string()
            } else {
                format!("Ch:{}", config.midi_channel)
            };
            ui.label(egui::RichText::new(ch_text).color(colors::SUBTEXT0).small());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Remove button
                if ui
                    .button(egui::RichText::new("✕").color(colors::RED).small())
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
                    .button(egui::RichText::new("S").color(solo_color).small())
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
                    .button(egui::RichText::new("M").color(mute_color).small())
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
            ui.label(egui::RichText::new("Vol:").color(colors::SUBTEXT0).small());
            // Volume slider
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

            ui.label(egui::RichText::new("Pan:").color(colors::SUBTEXT0).small());
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

        // Runner mode: show inline code editor
        if config.mode == SlotMode::Runner {
            ui.separator();

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Root Note:").color(colors::SUBTEXT0).small());
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
                        .small(),
                );
            });

            // Code editor
            let mut source = config.source_code.clone();
            let response = ui.add(
                egui::TextEdit::multiline(&mut source)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
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
                ui.label(egui::RichText::new(err).color(colors::RED).small());
            }
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
