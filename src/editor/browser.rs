use nih_plug_egui::egui;

use super::colors;
use super::zs;
use super::EditorEvent;
use super::EditorState;
use crate::preset::manager::{LibraryStatus, PresetManager};
use crate::state::SlotConfig;

/// Persistent state for the preset browser.
#[derive(Default)]
pub struct BrowserState {
    pub search_text: String,
    pub selected_category: Option<String>,
    pub selected_preset: Option<(String, String)>, // (library, preset_path)
}

/// Category chip definitions matching the JS version.
const CATEGORIES: &[(&str, &str)] = &[
    ("All", ""),
    ("Sampler", "sampler"),
    ("Synth", "synth"),
    ("Composite", "composite"),
    ("Effect", "effect"),
];

/// Draw the preset browser panel (matches JS PresetBrowser layout).
pub fn draw(ui: &mut egui::Ui, state: &mut EditorState, z: f32) {
    ui.set_clip_rect(ui.max_rect());
    ui.vertical(|ui| {
        ui.set_max_width(ui.available_width());
        ui.spacing_mut().item_spacing = egui::vec2(zs(6.0, z), zs(3.0, z));

        // --- Header ---
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Presets")
                    .color(colors::TEXT)
                    .strong()
                    .size(zs(14.0, z)),
            );
        });

        ui.add_space(zs(4.0, z));

        // --- Search bar ---
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.browser_state.search_text)
                    .hint_text("Search presets…")
                    .desired_width(ui.available_width()),
            );
            if response.changed() {
                if let Ok(mut pm) = state.preset_manager.lock() {
                    pm.search_query = state.browser_state.search_text.clone();
                }
            }
        });

        ui.add_space(zs(4.0, z));

        // --- Category filter chips ---
        ui.horizontal_wrapped(|ui| {
            for &(label, value) in CATEGORIES {
                let is_all = value.is_empty();
                let is_selected = if is_all {
                    state.browser_state.selected_category.is_none()
                } else {
                    state.browser_state.selected_category.as_deref() == Some(value)
                };

                let color = if is_selected {
                    match value {
                        "sampler" => colors::GREEN,
                        "synth" => colors::BLUE,
                        "composite" => colors::MAUVE,
                        "effect" => colors::PEACH,
                        _ => colors::BLUE,
                    }
                } else {
                    colors::SUBTEXT0
                };

                if ui
                    .selectable_label(
                        is_selected,
                        egui::RichText::new(label).color(color).size(zs(11.0, z)),
                    )
                    .clicked()
                {
                    if is_all {
                        state.browser_state.selected_category = None;
                    } else {
                        state.browser_state.selected_category = Some(value.to_string());
                    }
                    if let Ok(mut pm) = state.preset_manager.lock() {
                        pm.category_filter = state.browser_state.selected_category.clone();
                    }
                }
            }
        });

        ui.separator();

        // --- Library tree ---
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let search_active = !state.browser_state.search_text.is_empty();

                if search_active {
                    draw_search_results(ui, state, z);
                } else {
                    draw_library_tree(ui, state, z);
                }
            });

        // --- Status bar ---
        ui.add_space(zs(4.0, z));
        if let Ok(pm) = state.preset_manager.lock() {
            if !pm.status_message.is_empty() {
                ui.label(
                    egui::RichText::new(&pm.status_message)
                        .color(colors::OVERLAY0)
                        .size(zs(10.0, z))
                        .italics(),
                );
            }
        }
    });
}

/// Draw the collapsible library tree (no search active).
fn draw_library_tree(ui: &mut egui::Ui, state: &mut EditorState, z: f32) {
    // Collect library info outside the lock
    let libraries: Vec<(String, String, usize, LibraryStatus, bool)> = if let Ok(pm) = state.preset_manager.lock() {
        pm.libraries
            .iter()
            .map(|l| {
                (
                    l.name.clone(),
                    l.description.clone(),
                    l.preset_count,
                    l.status.clone(),
                    l.expanded,
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    if libraries.is_empty() {
        ui.label(
            egui::RichText::new("No presets loaded. Check internet connection.")
                .color(colors::OVERLAY0)
                .size(zs(11.0, z))
                .italics(),
        );
        return;
    }

    for (name, _desc, count, status, expanded) in &libraries {
        // Library folder row
        let chevron = if *expanded { "\u{25BE}" } else { "\u{25B8}" };
        let status_indicator = match status {
            LibraryStatus::Loading => " \u{23F3}",
            LibraryStatus::Error(_) => " \u{26A0}",
            _ => "",
        };

        let response = ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(chevron)
                    .color(colors::SUBTEXT0)
                    .size(zs(12.0, z))
                    .family(egui::FontFamily::Monospace),
            );
            ui.label(
                egui::RichText::new("\u{1F4C1}")
                    .size(zs(12.0, z)),
            );
            ui.label(
                egui::RichText::new(&format!("{}{}", name, status_indicator))
                    .color(colors::TEXT)
                    .size(zs(12.0, z)),
            );
            ui.label(
                egui::RichText::new(&format!("({})", count))
                    .color(colors::OVERLAY0)
                    .size(zs(11.0, z)),
            );
        });

        // Handle click on library folder row
        if response.response.interact(egui::Sense::click()).clicked() {
            let lib_name = name.clone();
            if let Ok(mut pm) = state.preset_manager.lock() {
                if let Some(lib) = pm.libraries.iter_mut().find(|l| l.name == lib_name) {
                    lib.expanded = !lib.expanded;
                    let should_fetch = lib.expanded && lib.status == LibraryStatus::NotLoaded;
                    let is_expanded = lib.expanded;
                    drop(pm);

                    if should_fetch {
                        // Trigger background fetch
                        PresetManager::fetch_library_index(
                            state.preset_manager.clone(),
                            lib_name,
                        );
                    }
                    let _ = is_expanded; // suppress warning
                }
            }
        }

        // Show presets if library is expanded
        if *expanded {
            let presets: Vec<(String, String, String)> = if let Ok(pm) = state.preset_manager.lock() {
                pm.filtered_presets_for_library(name)
                    .iter()
                    .take(200) // Limit for UI performance
                    .map(|p| (p.name.clone(), p.path.clone(), p.category.clone()))
                    .collect()
            } else {
                Vec::new()
            };

            if presets.is_empty() {
                match status {
                    LibraryStatus::Loading => {
                        ui.horizontal(|ui| {
                            ui.add_space(zs(24.0, z));
                            ui.label(
                                egui::RichText::new("Loading…")
                                    .color(colors::OVERLAY0)
                                    .size(zs(11.0, z))
                                    .italics(),
                            );
                        });
                    }
                    LibraryStatus::Error(e) => {
                        ui.horizontal(|ui| {
                            ui.add_space(zs(24.0, z));
                            ui.label(
                                egui::RichText::new(&format!("⚠ {}", e))
                                    .color(colors::RED)
                                    .size(zs(11.0, z)),
                            );
                        });
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            ui.add_space(zs(24.0, z));
                            ui.label(
                                egui::RichText::new("No presets")
                                    .color(colors::OVERLAY0)
                                    .size(zs(11.0, z))
                                    .italics(),
                            );
                        });
                    }
                }
            }

            for (preset_name, preset_path, category) in &presets {
                let is_selected = state.browser_state.selected_preset.as_ref()
                    == Some(&(name.clone(), preset_path.clone()));

                let cat_color = match category.as_str() {
                    "sampler" => colors::GREEN,
                    "synth" => colors::BLUE,
                    "composite" => colors::MAUVE,
                    "effect" => colors::PEACH,
                    _ => colors::SUBTEXT0,
                };

                ui.horizontal(|ui| {
                    ui.add_space(zs(24.0, z));

                    // Play button (painted triangle)
                    if play_triangle_button(ui, z).clicked() {
                        // Add to slot first (ensures a slot has this preset), then preview
                        add_preset_to_slot(state, name, preset_name, preset_path);
                        let slot_idx = state.slot_rack_state.selected_slot;
                        let _ = state.event_tx.try_send(EditorEvent::NoteOn {
                            slot_index: slot_idx,
                            note: 60, // C4
                            velocity: 0.8,
                        });
                    }

                    // "+" add-to-slot button
                    if ui
                        .small_button(egui::RichText::new("+").color(colors::GREEN).size(zs(10.0, z)))
                        .on_hover_text("Add to next available slot")
                        .clicked()
                    {
                        add_preset_to_slot(state, name, preset_name, preset_path);
                    }

                    let dot = egui::RichText::new("●")
                        .color(cat_color)
                        .size(zs(8.0, z));
                    ui.label(dot);

                    let display_name = if preset_name.len() > 35 {
                        format!("{}…", &preset_name[..34])
                    } else {
                        preset_name.clone()
                    };

                    let response = ui.selectable_label(
                        is_selected,
                        egui::RichText::new(&display_name).color(if is_selected {
                            colors::BLUE
                        } else {
                            colors::TEXT
                        }).size(zs(11.0, z)),
                    );

                    if response.clicked() {
                        state.browser_state.selected_preset =
                            Some((name.clone(), preset_path.clone()));
                    }

                    response.on_hover_text(format!("{}/{}", name, preset_path));
                });
            }
        }
    }
}

/// Draw flat search results across all loaded presets.
fn draw_search_results(ui: &mut egui::Ui, state: &mut EditorState, z: f32) {
    let results: Vec<(String, String, String, String)> = if let Ok(pm) = state.preset_manager.lock() {
        let mut all = Vec::new();
        for lib in &pm.libraries {
            for p in pm.filtered_presets_for_library(&lib.name) {
                all.push((
                    lib.name.clone(),
                    p.name.clone(),
                    p.path.clone(),
                    p.category.clone(),
                ));
            }
        }
        all
    } else {
        Vec::new()
    };

    if results.is_empty() {
        ui.label(
            egui::RichText::new("No matching presets. Expand folders to load more.")
                .color(colors::OVERLAY0)
                .size(zs(11.0, z))
                .italics(),
        );
        return;
    }

    for (lib_name, preset_name, preset_path, category) in results.iter().take(200) {
        let is_selected = state.browser_state.selected_preset.as_ref()
            == Some(&(lib_name.clone(), preset_path.clone()));

        let cat_color = match category.as_str() {
            "sampler" => colors::GREEN,
            "synth" => colors::BLUE,
            "composite" => colors::MAUVE,
            "effect" => colors::PEACH,
            _ => colors::SUBTEXT0,
        };

        ui.horizontal(|ui| {
            // Play button (painted triangle)
            if play_triangle_button(ui, z).clicked() {
                add_preset_to_slot(state, lib_name, preset_name, preset_path);
                let slot_idx = state.slot_rack_state.selected_slot;
                let _ = state.event_tx.try_send(EditorEvent::NoteOn {
                    slot_index: slot_idx,
                    note: 60,
                    velocity: 0.8,
                });
            }

            // "+" add-to-slot button
            if ui
                .small_button(egui::RichText::new("+").color(colors::GREEN).size(zs(10.0, z)))
                .on_hover_text("Add to next available slot")
                .clicked()
            {
                add_preset_to_slot(state, lib_name, preset_name, preset_path);
            }

            let dot = egui::RichText::new("●").color(cat_color).size(zs(8.0, z));
            ui.label(dot);

            let response = ui.selectable_label(
                is_selected,
                egui::RichText::new(preset_name).color(if is_selected {
                    colors::BLUE
                } else {
                    colors::TEXT
                }).size(zs(11.0, z)),
            );

            if response.clicked() {
                state.browser_state.selected_preset =
                    Some((lib_name.clone(), preset_path.clone()));
            }

            response.on_hover_text(format!("{}/{}", lib_name, preset_path));
        });
    }
}

/// Add a preset to the next available (empty) slot, or create a new one.
fn add_preset_to_slot(
    state: &mut EditorState,
    library_name: &str,
    preset_name: &str,
    preset_path: &str,
) {
    let preset_id = format!("{}/{}", library_name, preset_path);

    if let Ok(mut ps) = state.plugin_state.lock() {
        // Find the first empty slot (no preset assigned)
        let empty_idx = ps
            .slot_configs
            .iter()
            .position(|c| c.preset_id.is_none() && c.source_code.is_empty());

        if let Some(idx) = empty_idx {
            // Assign to existing empty slot
            ps.slot_configs[idx].name = preset_name.to_string();
            ps.slot_configs[idx].preset_id = Some(preset_id);
            state.slot_rack_state.selected_slot = idx;
        } else {
            // Create a new slot with this preset
            let config = SlotConfig::new_preset(preset_name, &preset_id);
            let idx = ps.add_slot_config(config);
            state.slot_rack_state.selected_slot = idx;
        }
    }
}

/// Draw a small play triangle button (▶) using the egui painter.
/// Returns the Response so the caller can check `.clicked()`.
fn play_triangle_button(ui: &mut egui::Ui, z: f32) -> egui::Response {
    let size = zs(14.0, z);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(size, size),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let color = if response.hovered() {
            colors::GREEN
        } else {
            colors::TEAL
        };

        // Draw a right-pointing triangle centered in the rect
        let center = rect.center();
        let half = size * 0.4;
        let points = vec![
            egui::pos2(center.x - half * 0.6, center.y - half),
            egui::pos2(center.x + half * 0.8, center.y),
            egui::pos2(center.x - half * 0.6, center.y + half),
        ];
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::NONE,
        ));
    }

    response.on_hover_text("Preview preset (C4)")
}
