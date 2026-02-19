//! Preset browser panel — left side of the editor.
//!
//! Shows search, category filter chips, and a collapsible library tree with
//! per-preset "+" buttons to add instruments to slots.

use std::sync::{Arc, Mutex};

use nih_plug_vizia::vizia::prelude::*;

use super::{AppEvent, Data};
use crate::preset::manager::{LibraryStatus, PresetManager};

/// Category definitions matching the web editor.
const CATEGORIES: &[(&str, &str)] = &[
    ("All", ""),
    ("Sampler", "sampler"),
    ("Synth", "synth"),
    ("Composite", "composite"),
    ("Effect", "effect"),
];

/// Build the browser panel.
pub fn build(cx: &mut Context) {
    VStack::new(cx, |cx| {
        // Panel title
        Label::new(cx, "Presets")
            .class("panel-title");

        // Search bar
        Textbox::new(cx, Data::search_text)
            .class("search-box")
            .width(Stretch(1.0))
            .on_edit(|cx, text| {
                cx.emit(AppEvent::SetSearchText(text));
            });

        // Category chips
        HStack::new(cx, |cx| {
            for &(label, value) in CATEGORIES {
                let value_owned = value.to_string();
                let cat_class = match value {
                    "" => "cat-all",
                    "sampler" => "cat-sampler",
                    "synth" => "cat-synth",
                    "composite" => "cat-composite",
                    "effect" => "cat-effect",
                    _ => "cat-all",
                };

                let value_for_check = value_owned.clone();
                Button::new(
                    cx,
                    move |cx| cx.emit(AppEvent::SetCategoryFilter(value_owned.clone())),
                    |cx| Label::new(cx, label),
                )
                .class("category-chip")
                .class(cat_class)
                .checked(Data::category_filter.map(move |f| *f == value_for_check));
            }
        })
        .class("category-row");

        // Separator
        Element::new(cx)
            .height(Pixels(1.0))
            .width(Stretch(1.0))
            .background_color(Color::rgb(49, 50, 68));

        // Library tree (scrollable)
        ScrollView::new(cx, 0.0, 0.0, false, true, |cx| {
            Binding::new(cx, Data::search_text, |cx, search| {
                let search_val = search.get(cx);

                // We need to access preset_manager from cx.
                // Since vizia is retained-mode, we build the tree from
                // a snapshot of the data that we read each time the binding fires.
                Binding::new(cx, Data::preset_manager, move |cx, pm_lens| {
                    let pm_arc = pm_lens.get(cx);
                    build_library_tree(cx, &pm_arc, &search_val);
                });
            });
        })
        .height(Stretch(1.0));
    })
    .id("browser-panel");
}

/// Build the library tree from a PresetManager snapshot.
fn build_library_tree(
    cx: &mut Context,
    pm_arc: &Arc<Mutex<PresetManager>>,
    search_text: &str,
) {
    let Ok(pm) = pm_arc.lock() else { return };
    let search_active = !search_text.is_empty();

    if search_active {
        // Flat search results
        let mut results = Vec::new();
        for lib in &pm.libraries {
            for p in pm.filtered_presets_for_library(&lib.name) {
                results.push((
                    lib.name.clone(),
                    p.name.clone(),
                    p.path.clone(),
                    p.category.clone(),
                ));
            }
        }
        drop(pm);

        if results.is_empty() {
            Label::new(cx, "No matching presets.")
                .class("empty-message");
        } else {
            for (lib_name, preset_name, preset_path, category) in results.iter().take(200) {
                build_preset_row(cx, lib_name, preset_name, preset_path, category);
            }
        }
    } else {
        // Library folders
        let libraries: Vec<(String, usize, LibraryStatus, bool)> = pm
            .libraries
            .iter()
            .map(|l| (l.name.clone(), l.preset_count, l.status.clone(), l.expanded))
            .collect();

        // Build preset lists while lock is held
        let mut presets_by_lib: Vec<Vec<(String, String, String)>> = Vec::new();
        for lib in &pm.libraries {
            if lib.expanded {
                presets_by_lib.push(
                    pm.filtered_presets_for_library(&lib.name)
                        .iter()
                        .take(200)
                        .map(|p| (p.name.clone(), p.path.clone(), p.category.clone()))
                        .collect(),
                );
            } else {
                presets_by_lib.push(Vec::new());
            }
        }
        drop(pm);

        if libraries.is_empty() {
            Label::new(cx, "No presets loaded. Check internet connection.")
                .class("empty-message");
            return;
        }

        for (i, (name, count, status, expanded)) in libraries.iter().enumerate() {
            let name_display = name.clone();
            let name_event = name.clone();
            let chevron = if *expanded { "\u{25BE}" } else { "\u{25B8}" };
            let status_icon = match status {
                LibraryStatus::Loading => " \u{23F3}",
                LibraryStatus::Error(_) => " \u{26A0}",
                _ => "",
            };
            let display_text = format!("{}{}", &name_display, status_icon);
            let count_text = format!("({})", count);

            // Library folder row
            HStack::new(cx, move |cx| {
                Label::new(cx, chevron)
                    .class("chevron");
                Label::new(cx, "\u{1F4C1}");
                Label::new(cx, &display_text)
                    .class("library-name");
                Label::new(cx, &count_text)
                    .class("library-count");
            })
            .class("library-row")
            .on_press(move |cx| {
                cx.emit(AppEvent::ToggleLibrary(name_event.clone()));
            });

            // Presets under this library
            if *expanded {
                let presets = &presets_by_lib[i];
                if presets.is_empty() {
                    let msg = match status {
                        LibraryStatus::Loading => "Loading\u{2026}",
                        LibraryStatus::Error(_) => "\u{26A0} Error loading",
                        _ => "No presets",
                    };
                    Label::new(cx, msg)
                        .class("empty-message")
                        .left(Pixels(24.0));
                } else {
                    let lib_name = name.clone();
                    for (preset_name, preset_path, category) in presets {
                        build_preset_row(cx, &lib_name, preset_name, preset_path, category);
                    }
                }
            }
        }
    }
}

/// Build a single preset row with "+" button, category dot, and name.
fn build_preset_row(
    cx: &mut Context,
    lib_name: &str,
    preset_name: &str,
    preset_path: &str,
    category: &str,
) {
    let lib = lib_name.to_string();
    let name = preset_name.to_string();
    let path = preset_path.to_string();

    let dot_color = match category {
        "sampler" => Color::rgb(166, 227, 161),
        "synth" => Color::rgb(137, 180, 250),
        "composite" => Color::rgb(203, 166, 247),
        "effect" => Color::rgb(250, 179, 135),
        _ => Color::rgb(166, 173, 200),
    };

    let display_name = if name.len() > 35 {
        format!("{}\u{2026}", &name[..34])
    } else {
        name.clone()
    };

    let lib_add = lib.clone();
    let name_add = name.clone();
    let path_add = path.clone();

    HStack::new(cx, move |cx| {
        let lib_btn = lib_add.clone();
        let name_btn = name_add.clone();
        let path_btn = path_add.clone();

        Button::new(
            cx,
            move |cx| {
                cx.emit(AppEvent::AddPresetToSlot(
                    lib_btn.clone(),
                    name_btn.clone(),
                    path_btn.clone(),
                ));
            },
            |cx| Label::new(cx, "+"),
        )
        .class("add-btn");

        Label::new(cx, "\u{25CF}")
            .class("preset-dot")
            .color(dot_color);

        Label::new(cx, &display_name)
            .class("preset-name");
    })
    .class("preset-row");
}
