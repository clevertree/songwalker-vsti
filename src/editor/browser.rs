use nih_plug_egui::egui;

use super::colors;
use super::EditorState;

/// Persistent state for the preset browser.
#[derive(Default)]
pub struct BrowserState {
    pub search_text: String,
    pub selected_category: Option<String>,
    pub selected_preset: Option<(String, String)>, // (library, preset_path)
}

/// Draw the preset browser panel.
pub fn draw(ui: &mut egui::Ui, state: &mut EditorState) {
    ui.vertical(|ui| {
        // --- Search bar ---
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🔍").color(colors::SUBTEXT0));
            let response = ui.text_edit_singleline(&mut state.browser_state.search_text);
            if response.changed() {
                if let Ok(mut pm) = state.preset_manager.lock() {
                    pm.search_query = state.browser_state.search_text.clone();
                }
            }
        });

        ui.separator();

        // --- Libraries section ---
        ui.label(egui::RichText::new("Libraries").color(colors::SUBTEXT0).small());

        let libraries: Vec<(String, usize, bool)> = if let Ok(pm) = state.preset_manager.lock() {
            pm.libraries
                .iter()
                .map(|l| (l.name.clone(), l.entry_count, l.enabled))
                .collect()
        } else {
            Vec::new()
        };

        for (name, count, enabled) in &libraries {
            ui.horizontal(|ui| {
                let mut checked = *enabled;
                if ui.checkbox(&mut checked, "").changed() {
                    if let Ok(mut pm) = state.preset_manager.lock() {
                        if let Some(lib) = pm.libraries.iter_mut().find(|l| &l.name == name) {
                            lib.enabled = checked;
                        }
                    }
                }
                ui.label(
                    egui::RichText::new(format!("{} ({})", name, count))
                        .color(if *enabled {
                            colors::TEXT
                        } else {
                            colors::OVERLAY0
                        })
                        .small(),
                );
            });
        }

        // Download for Offline button
        ui.separator();
        if ui
            .button(
                egui::RichText::new("⬇ Download for Offline")
                    .color(colors::TEAL)
                    .small(),
            )
            .clicked()
        {
            // TODO: trigger background download of enabled libraries
        }

        ui.separator();

        // --- Categories section ---
        ui.label(egui::RichText::new("Categories").color(colors::SUBTEXT0).small());

        let categories: Vec<String> = if let Ok(pm) = state.preset_manager.lock() {
            pm.available_categories()
        } else {
            Vec::new()
        };

        ui.horizontal_wrapped(|ui| {
            // "All" chip
            let all_selected = state.browser_state.selected_category.is_none();
            if ui
                .selectable_label(
                    all_selected,
                    egui::RichText::new("All").color(if all_selected {
                        colors::BLUE
                    } else {
                        colors::SUBTEXT0
                    }),
                )
                .clicked()
            {
                state.browser_state.selected_category = None;
                if let Ok(mut pm) = state.preset_manager.lock() {
                    pm.category_filter = None;
                }
            }

            for cat in &categories {
                let is_sel = state.browser_state.selected_category.as_ref() == Some(cat);
                if ui
                    .selectable_label(
                        is_sel,
                        egui::RichText::new(cat).color(if is_sel {
                            colors::BLUE
                        } else {
                            colors::SUBTEXT0
                        }),
                    )
                    .clicked()
                {
                    state.browser_state.selected_category = Some(cat.clone());
                    if let Ok(mut pm) = state.preset_manager.lock() {
                        pm.category_filter = Some(cat.clone());
                    }
                }
            }
        });

        ui.separator();

        // --- Instrument list ---
        ui.label(egui::RichText::new("Instruments").color(colors::SUBTEXT0).small());

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let entries: Vec<(String, String, String)> =
                    if let Ok(pm) = state.preset_manager.lock() {
                        pm.filtered_entries()
                            .iter()
                            .take(200) // Limit for UI performance
                            .map(|(lib, entry)| {
                                (
                                    lib.to_string(),
                                    entry.path.clone(),
                                    entry.name.clone(),
                                )
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };

                if entries.is_empty() {
                    ui.label(
                        egui::RichText::new("No presets loaded. Check internet connection.")
                            .color(colors::OVERLAY0)
                            .small()
                            .italics(),
                    );
                }

                for (lib, path, name) in &entries {
                    let is_selected = state.browser_state.selected_preset.as_ref()
                        == Some(&(lib.clone(), path.clone()));

                    let response = ui.selectable_label(
                        is_selected,
                        egui::RichText::new(name).color(if is_selected {
                            colors::BLUE
                        } else {
                            colors::TEXT
                        }),
                    );

                    if response.clicked() {
                        state.browser_state.selected_preset =
                            Some((lib.clone(), path.clone()));
                        // TODO: trigger preset loading for the active slot
                    }

                    // Show library name on hover
                    response.on_hover_text(format!("{}/{}", lib, path));
                }
            });
    });
}
