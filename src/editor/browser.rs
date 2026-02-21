use nih_plug_egui::egui;
use std::sync::Arc;

use super::colors;
use super::zs;
use super::EditorState;
use super::PresetLoadedEvent;
use crate::preset::loader::PresetLoader;
use crate::preset::manager::{LibraryStatus, PresetManager};
use crate::state::SlotConfig;

/// Number of presets to show per page in the browser.
const PAGE_SIZE: usize = 100;

/// Number of slots reserved for preview round-robin playback.
/// This allows clicking multiple presets rapidly without cutting off previous ones.
const PREVIEW_SLOTS: usize = 8;

/// Persistent state for the preset browser.
#[derive(Default)]
pub struct BrowserState {
    pub search_text: String,
    pub selected_category: Option<String>,
    pub selected_preset: Option<(String, String)>, // (library, preset_path)
    /// Per-context page offset: key is a library name, sub-index key, or "search".
    pub page_offsets: std::collections::HashMap<String, usize>,
    /// Round-robin counter for preview slot allocation.
    /// Each preview click uses the next slot so multiple presets can play simultaneously.
    next_preview_slot: usize,
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
                // Reset all pagination when search changes
                state.browser_state.page_offsets.clear();
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
                    // Reset pagination on category change
                    state.browser_state.page_offsets.clear();
                }
            }
        });

        ui.separator();

        // --- Library tree ---
        egui::ScrollArea::both()
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

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), zs(20.0, z)),
            egui::Sense::click(),
        );

        if response.hovered() {
            ui.painter()
                .rect_filled(rect, zs(4.0, z), colors::SURFACE0.gamma_multiply(0.5));
        }

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.horizontal(|ui| {
                ui.add_space(zs(4.0, z));
                ui.label(
                    egui::RichText::new(chevron)
                        .color(colors::SUBTEXT0)
                        .size(zs(12.0, z))
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(egui::RichText::new("\u{1F4C1}").size(zs(12.0, z)));
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
        });

        // Handle click on library folder row
        if response.clicked() {
            let lib_name = name.clone();
            if let Ok(mut pm) = state.preset_manager.lock() {
                if let Some(lib) = pm.libraries.iter_mut().find(|l| l.name == lib_name) {
                    lib.expanded = !lib.expanded;
                    let should_fetch = lib.expanded && lib.status == LibraryStatus::NotLoaded;
                    let is_expanded = lib.expanded;
                    drop(pm);

                    if should_fetch {
                        // Trigger background fetch
                        PresetManager::fetch_library_index(state.preset_manager.clone(), lib_name);
                    }
                    let _ = is_expanded; // suppress warning
                }
            }
        }

        // Show presets if library is expanded
        if *expanded {
            // Check if this library uses sub-indexes (hierarchical) or flat presets
            let has_sub_indexes = if let Ok(pm) = state.preset_manager.lock() {
                pm.library_has_sub_indexes(name)
            } else {
                false
            };

            if has_sub_indexes {
                // 3-level hierarchy: library → sub-index → presets
                draw_sub_indexes(ui, state, name, status, z);
            } else {
                // Flat: library → presets
                draw_preset_list(ui, state, name, name, status, zs(24.0, z), z);
            }
        }
    }
}

/// Draw sub-index folders within a library (e.g., games in Auto-Ripped).
fn draw_sub_indexes(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    lib_name: &str,
    lib_status: &LibraryStatus,
    z: f32,
) {
    // Collect sub-index info outside the lock
    let sub_idxs: Vec<(String, String, usize, bool)> = if let Ok(pm) = state.preset_manager.lock()
    {
        pm.sub_indexes
            .get(lib_name)
            .map(|subs| {
                let query = pm.search_query.to_lowercase();
                subs.iter()
                    .filter(|s| {
                        // If there's a search query, filter sub-indexes by name
                        if query.is_empty() {
                            true
                        } else {
                            s.name.to_lowercase().contains(&query)
                        }
                    })
                    .map(|s| {
                        (
                            s.name.clone(),
                            s.path.clone(),
                            s.instrument_count,
                            s.expanded,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    if sub_idxs.is_empty() {
        match lib_status {
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
            _ => {
                ui.horizontal(|ui| {
                    ui.add_space(zs(24.0, z));
                    ui.label(
                        egui::RichText::new("No sub-indexes")
                            .color(colors::OVERLAY0)
                            .size(zs(11.0, z))
                            .italics(),
                    );
                });
            }
        }
        return;
    }

    for (sub_name, sub_path, inst_count, sub_expanded) in &sub_idxs {
        let chevron = if *sub_expanded { "\u{25BE}" } else { "\u{25B8}" };

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), zs(18.0, z)),
            egui::Sense::click(),
        );

        if response.hovered() {
            ui.painter()
                .rect_filled(rect, zs(4.0, z), colors::SURFACE0.gamma_multiply(0.5));
        }

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.horizontal(|ui| {
                ui.add_space(zs(24.0, z)); // Indent
                ui.label(
                    egui::RichText::new(chevron)
                        .color(colors::SUBTEXT0)
                        .size(zs(11.0, z))
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(egui::RichText::new("\u{1F3B5}").size(zs(11.0, z)));
                ui.label(
                    egui::RichText::new(sub_name)
                        .color(colors::SUBTEXT1)
                        .size(zs(11.0, z)),
                );
                ui.label(
                    egui::RichText::new(&format!("({})", inst_count))
                        .color(colors::OVERLAY0)
                        .size(zs(10.0, z)),
                );
            });
        });

        if response.clicked() {
            let lib = lib_name.to_string();
            let sn = sub_name.clone();
            let sp = sub_path.clone();
            let should_fetch;

            if let Ok(mut pm) = state.preset_manager.lock() {
                if let Some(subs) = pm.sub_indexes.get_mut(&lib) {
                    if let Some(sub) = subs.iter_mut().find(|s| s.name == sn) {
                        sub.expanded = !sub.expanded;
                        should_fetch = sub.expanded
                            && !pm.sub_index_presets.contains_key(&format!("{}/{}", lib, sn));
                    } else {
                        should_fetch = false;
                    }
                } else {
                    should_fetch = false;
                }
            } else {
                should_fetch = false;
            }

            if should_fetch {
                PresetManager::fetch_sub_index(state.preset_manager.clone(), lib, sn, sp);
            }
        }

        // Show sub-index presets if expanded
        if *sub_expanded {
            let key = format!("{}/{}", lib_name, sub_name);
            draw_sub_index_presets(ui, state, lib_name, &key, z);
        }
    }
}

/// Draw presets belonging to a sub-index.
fn draw_sub_index_presets(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    lib_name: &str,
    sub_key: &str,
    z: f32,
) {
    let all_presets: Vec<(String, String, String)> = if let Ok(pm) = state.preset_manager.lock() {
        pm.filtered_presets_for_sub_index(sub_key)
            .iter()
            .map(|p| (p.name.clone(), p.path.clone(), p.category.clone()))
            .collect()
    } else {
        Vec::new()
    };

    if all_presets.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(zs(44.0, z));
            ui.label(
                egui::RichText::new("Loading…")
                    .color(colors::OVERLAY0)
                    .size(zs(11.0, z))
                    .italics(),
            );
        });
        return;
    }

    let offset = *state.browser_state.page_offsets.get(sub_key).unwrap_or(&0);
    let page = &all_presets[offset.min(all_presets.len())..];
    let showing = page.len().min(PAGE_SIZE);

    for (preset_name, preset_path, category) in &page[..showing] {
        draw_preset_row(ui, state, lib_name, preset_name, preset_path, category, zs(44.0, z), z);
    }

    draw_pagination_controls(ui, state, sub_key, offset, all_presets.len(), zs(44.0, z), z);
}

/// Draw a flat list of presets for a library (no sub-indexes).
fn draw_preset_list(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    lib_name: &str,
    filter_lib: &str,
    status: &LibraryStatus,
    indent: f32,
    z: f32,
) {
    let all_presets: Vec<(String, String, String)> = if let Ok(pm) = state.preset_manager.lock() {
        pm.filtered_presets_for_library(filter_lib)
            .iter()
            .map(|p| (p.name.clone(), p.path.clone(), p.category.clone()))
            .collect()
    } else {
        Vec::new()
    };

    if all_presets.is_empty() {
        match status {
            LibraryStatus::Loading => {
                ui.horizontal(|ui| {
                    ui.add_space(indent);
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
                    ui.add_space(indent);
                    ui.label(
                        egui::RichText::new(&format!("⚠ {}", e))
                            .color(colors::RED)
                            .size(zs(11.0, z)),
                    );
                });
            }
            _ => {
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    ui.label(
                        egui::RichText::new("No presets")
                            .color(colors::OVERLAY0)
                            .size(zs(11.0, z))
                            .italics(),
                    );
                });
            }
        }
        return;
    }

    let page_key = filter_lib.to_string();
    let offset = *state.browser_state.page_offsets.get(&page_key).unwrap_or(&0);
    let page = &all_presets[offset.min(all_presets.len())..];
    let showing = page.len().min(PAGE_SIZE);

    for (preset_name, preset_path, category) in &page[..showing] {
        draw_preset_row(ui, state, lib_name, preset_name, preset_path, category, indent, z);
    }

    draw_pagination_controls(ui, state, &page_key, offset, all_presets.len(), indent, z);
}

/// Draw a single preset row with play/add buttons and category indicator.
fn draw_preset_row(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    lib_name: &str,
    preset_name: &str,
    preset_path: &str,
    category: &str,
    indent: f32,
    z: f32,
) {
    let is_selected = state.browser_state.selected_preset.as_ref()
        == Some(&(lib_name.to_string(), preset_path.to_string()));

    let cat_color = match category {
        "sampler" => colors::GREEN,
        "synth" => colors::BLUE,
        "composite" => colors::MAUVE,
        "effect" => colors::PEACH,
        _ => colors::SUBTEXT0,
    };

    ui.horizontal(|ui| {
        ui.add_space(indent);

        // Play button (painted triangle)
        if play_triangle_button(ui, z).clicked() {
            let preview_slot = state.browser_state.next_preview_slot;
            state.browser_state.next_preview_slot = (preview_slot + 1) % PREVIEW_SLOTS;
            spawn_preset_load(state, lib_name, preset_path, preview_slot, Some(60));
        }

        // "+" add-to-slot button
        if ui
            .small_button(
                egui::RichText::new("+")
                    .color(colors::GREEN)
                    .size(zs(10.0, z)),
            )
            .on_hover_text("Add to next available slot")
            .clicked()
        {
            let slot_idx = add_preset_to_slot(state, lib_name, preset_name, preset_path);
            spawn_preset_load(state, lib_name, preset_path, slot_idx, None);
        }

        let dot = egui::RichText::new("●")
            .color(cat_color)
            .size(zs(8.0, z));
        ui.label(dot);

        let display_name = if preset_name.len() > 35 {
            format!("{}…", &preset_name[..34])
        } else {
            preset_name.to_string()
        };

        let response = ui.selectable_label(
            is_selected,
            egui::RichText::new(&display_name)
                .color(if is_selected { colors::BLUE } else { colors::TEXT })
                .size(zs(11.0, z)),
        );

        if response.clicked() {
            state.browser_state.selected_preset =
                Some((lib_name.to_string(), preset_path.to_string()));
            // Also trigger preview load/play on click
            let preview_slot = state.browser_state.next_preview_slot;
            state.browser_state.next_preview_slot = (preview_slot + 1) % PREVIEW_SLOTS;
            spawn_preset_load(state, lib_name, preset_path, preview_slot, Some(60));
        }

        response.on_hover_text(format!("{}/{}", lib_name, preset_path));
    });
}

/// Draw flat search results across all loaded presets.
fn draw_search_results(ui: &mut egui::Ui, state: &mut EditorState, z: f32) {
    let results: Vec<(String, String, String, String)> = if let Ok(pm) = state.preset_manager.lock() {
        let mut all = Vec::new();
        // Flat library presets
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
        // Sub-index presets (from hierarchical libraries)
        for (key, _presets) in &pm.sub_index_presets {
            let lib_name = key.split('/').next().unwrap_or(key);
            for p in pm.filtered_presets_for_sub_index(key) {
                all.push((
                    lib_name.to_string(),
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

    let page_key = "search".to_string();
    let offset = *state.browser_state.page_offsets.get(&page_key).unwrap_or(&0);
    let page = &results[offset.min(results.len())..];
    let showing = page.len().min(PAGE_SIZE);

    for (lib_name, preset_name, preset_path, category) in &page[..showing] {
        draw_preset_row(ui, state, lib_name, preset_name, preset_path, category, 0.0, z);
    }

    draw_pagination_controls(ui, state, &page_key, offset, results.len(), 0.0, z);
}

/// Draw "Show previous" / "Show more" pagination controls.
fn draw_pagination_controls(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    page_key: &str,
    offset: usize,
    total: usize,
    indent: f32,
    z: f32,
) {
    if total <= PAGE_SIZE && offset == 0 {
        return; // Everything fits on one page
    }

    ui.horizontal(|ui| {
        ui.add_space(indent);

        let end = (offset + PAGE_SIZE).min(total);
        ui.label(
            egui::RichText::new(&format!("{}-{} of {}", offset + 1, end, total))
                .color(colors::OVERLAY0)
                .size(zs(10.0, z)),
        );

        if offset > 0 {
            if ui
                .small_button(
                    egui::RichText::new("◀ Previous")
                        .color(colors::BLUE)
                        .size(zs(10.0, z)),
                )
                .clicked()
            {
                let new_offset = offset.saturating_sub(PAGE_SIZE);
                state
                    .browser_state
                    .page_offsets
                    .insert(page_key.to_string(), new_offset);
            }
        }

        if offset + PAGE_SIZE < total {
            if ui
                .small_button(
                    egui::RichText::new("Next ▶")
                        .color(colors::BLUE)
                        .size(zs(10.0, z)),
                )
                .clicked()
            {
                state
                    .browser_state
                    .page_offsets
                    .insert(page_key.to_string(), offset + PAGE_SIZE);
            }
        }
    });
}

/// Add a preset to the next available (empty) slot, or create a new one.
/// Returns the slot index that was used.
fn add_preset_to_slot(
    state: &mut EditorState,
    library_name: &str,
    preset_name: &str,
    preset_path: &str,
) -> usize {
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
            idx
        } else {
            // Create a new slot with this preset
            let config = SlotConfig::new_preset(preset_name, &preset_id);
            let idx = ps.add_slot_config(config);
            state.slot_rack_state.selected_slot = idx;
            idx
        }
    } else {
        0
    }
}

/// Spawn a background thread that loads a preset (fetches JSON descriptor
/// and decodes all sample data) then delivers the result to the audio thread
/// via the `preset_loaded_tx` channel.
///
/// If `play_note` is `Some(midi_note)`, the audio thread will also trigger a
/// NoteOn immediately after loading (used for the preview play button).
fn spawn_preset_load(
    state: &EditorState,
    library_name: &str,
    preset_path: &str,
    slot_index: usize,
    play_note: Option<u8>,
) {
    let preset_manager = state.preset_manager.clone();
    let ui_preset_loaded_tx = state.ui_preset_loaded_tx.clone();
    let status_text = state.status_text.clone();
    let library = library_name.to_string();
    let path = preset_path.to_string();

    nih_plug::debug::nih_log!("[Browser] Spawning load for preset: {}/{} into slot {}", library_name, preset_path, slot_index);

    // Display the short name in the status bar
    let display_name = path.rsplit('/').next().unwrap_or(&path).to_string();
    if let Ok(mut st) = status_text.lock() {
        *st = format!("Loading {}\u{2026}", display_name);
    }

    std::thread::spawn(move || {
        nih_plug::debug::nih_log!("[LoaderThread] Background thread started for {}/{}", library, path);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();

        let Ok(rt) = rt else {
            nih_plug::debug::nih_log!("[LoaderThread] Error: Failed to create async runtime");
            if let Ok(mut st) = status_text.lock() {
                *st = "\u{26a0} Failed to create async runtime".to_string();
            }
            return;
        };

        let (base_url, slug) = {
            let pm = preset_manager.lock().unwrap();
            let slug = pm
                .libraries
                .iter()
                .find(|l| l.name == library)
                .map(|l| l.slug.clone())
                .unwrap_or_else(|| library.clone());
            (pm.base_url.clone(), slug)
        };
        let loader = PresetLoader::new().with_base_url(base_url);

        nih_plug::debug::nih_log!("[LoaderThread] Fetching preset: slug={} path={}", slug, path);

        match rt.block_on(loader.load_preset(&slug, &path, 44100.0)) {
            Ok(instance) => {
                let preset_id = Arc::new(format!("{}/{}", library, path));
                let zone_count = instance.zones.len();
                nih_plug::debug::nih_log!("[LoaderThread] Successfully loaded preset {}: zones={}", preset_id, zone_count);
                let _ = ui_preset_loaded_tx.try_send(PresetLoadedEvent {
                    slot_index,
                    preset_id,
                    instance: Arc::new(instance),
                    play_note,
                });
                if let Ok(mut st) = status_text.lock() {
                    *st = format!("Loaded {} ({} zones)", display_name, zone_count);
                }
            }
            Err(e) => {                nih_plug::debug::nih_log!("[LoaderThread] Error loading preset: {:?}", e);                if let Ok(mut st) = status_text.lock() {
                    *st = format!("\u{26a0} Error: {}", e);
                }
            }
        }
    });
}

/// Draw a small play triangle button (▶) using the egui painter.
/// Returns the Response so the caller can check `.clicked()`.
fn play_triangle_button(ui: &mut egui::Ui, z: f32) -> egui::Response {
    let size = zs(18.0, z);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());

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
