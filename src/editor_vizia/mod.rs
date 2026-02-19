//! Plugin editor UI using VIZIA.
//!
//! Layout mirrors the SongWalker web editor:
//! - Left panel: Preset browser (search, libraries, categories, instruments)
//! - Right panel: Slot rack (Kontakt-style) with inline editors
//! - Bottom: Status bar

pub mod browser;
pub mod resize_handle;
pub mod slot_rack;

use std::sync::{Arc, Mutex};

use nih_plug::prelude::*;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};

use crate::params::SongWalkerParams;
use crate::preset::manager::PresetManager;
use crate::state::PluginState;

use self::resize_handle::{SharedWindowSize, WindowResizeHandle};

/// Default editor window size.
pub const EDITOR_WIDTH: u32 = 800;
pub const EDITOR_HEIGHT: u32 = 600;
/// Minimum window size.
const MIN_WIDTH: u32 = 400;
const MIN_HEIGHT: u32 = 300;

/// Shared mutable window dimensions read by ViziaState's size_fn.
/// Stored globally so it survives editor close/re-open cycles.
static WINDOW_SIZE: std::sync::LazyLock<Arc<SharedWindowSize>> =
    std::sync::LazyLock::new(|| Arc::new(SharedWindowSize::new(EDITOR_WIDTH, EDITOR_HEIGHT)));

/// Create the default vizia state (window dimensions).
/// The size_fn reads from the shared mutable WINDOW_SIZE so that the custom
/// resize handle can change dimensions at runtime.
pub fn default_state() -> Arc<ViziaState> {
    let size = WINDOW_SIZE.clone();
    ViziaState::new(move || size.load())
}

/// Data model exposed to the VIZIA view tree via lenses.
#[derive(Lens, Clone)]
pub struct Data {
    pub params: Arc<SongWalkerParams>,
    pub preset_manager: Arc<Mutex<PresetManager>>,
    pub plugin_state: Arc<Mutex<PluginState>>,
    /// Active tab: 0 = Slot Rack, 1 = Settings
    pub active_tab: u32,
    /// Browser search text
    pub search_text: String,
    /// Selected category filter (empty = all)
    pub category_filter: String,
}

/// Events emitted by UI widgets and handled by the Data model.
pub enum AppEvent {
    SetTab(u32),
    SetSearchText(String),
    SetCategoryFilter(String),
    ToggleLibrary(String),
    SelectPreset(String, String),
    AddPresetToSlot(String, String, String),
    AddEmptySlot,
    RemoveSlot(usize),
    ToggleSolo(usize),
    ToggleMute(usize),
    SetSlotVolume(usize, f32),
    SetSlotPan(usize, f32),
    SetSlotRootNote(usize, u8),
    SetSlotSource(usize, String),
}

impl Model for Data {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _meta| match app_event {
            AppEvent::SetTab(tab) => {
                self.active_tab = *tab;
            }
            AppEvent::SetSearchText(text) => {
                self.search_text = text.clone();
                if let Ok(mut pm) = self.preset_manager.lock() {
                    pm.search_query = text.clone();
                }
            }
            AppEvent::SetCategoryFilter(cat) => {
                self.category_filter = cat.clone();
                if let Ok(mut pm) = self.preset_manager.lock() {
                    pm.category_filter = if cat.is_empty() {
                        None
                    } else {
                        Some(cat.clone())
                    };
                }
            }
            AppEvent::ToggleLibrary(name) => {
                let pm_arc = self.preset_manager.clone();
                let pm_arc2 = pm_arc.clone();
                let mut should_fetch = false;
                if let Ok(mut pm) = pm_arc.lock() {
                    if let Some(lib) = pm.libraries.iter_mut().find(|l| l.name == *name) {
                        lib.expanded = !lib.expanded;
                        should_fetch =
                            lib.expanded && lib.status == crate::preset::manager::LibraryStatus::NotLoaded;
                    }
                }
                if should_fetch {
                    PresetManager::fetch_library_index(pm_arc2, name.clone());
                }
            }
            AppEvent::SelectPreset(_lib, _path) => {
                // Selection tracking — future use
            }
            AppEvent::AddPresetToSlot(lib_name, preset_name, preset_path) => {
                let preset_id = format!("{}/{}", lib_name, preset_path);
                if let Ok(mut ps) = self.plugin_state.lock() {
                    let empty_idx = ps
                        .slot_configs
                        .iter()
                        .position(|c| c.preset_id.is_none() && c.source_code.is_empty());
                    if let Some(idx) = empty_idx {
                        ps.slot_configs[idx].name = preset_name.clone();
                        ps.slot_configs[idx].preset_id = Some(preset_id);
                    } else {
                        let config =
                            crate::state::SlotConfig::new_preset(preset_name, &preset_id);
                        ps.add_slot_config(config);
                    }
                }
            }
            AppEvent::AddEmptySlot => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    ps.add_slot_config(crate::state::SlotConfig::default());
                }
            }
            AppEvent::RemoveSlot(idx) => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    ps.remove_slot_config(*idx);
                }
            }
            AppEvent::ToggleSolo(idx) => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(*idx) {
                        cfg.solo = !cfg.solo;
                    }
                }
            }
            AppEvent::ToggleMute(idx) => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(*idx) {
                        cfg.muted = !cfg.muted;
                    }
                }
            }
            AppEvent::SetSlotVolume(idx, vol) => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(*idx) {
                        cfg.volume = *vol;
                    }
                }
            }
            AppEvent::SetSlotPan(idx, pan) => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(*idx) {
                        cfg.pan = *pan;
                    }
                }
            }
            AppEvent::SetSlotRootNote(idx, note) => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(*idx) {
                        cfg.root_note = *note;
                    }
                }
            }
            AppEvent::SetSlotSource(idx, src) => {
                if let Ok(mut ps) = self.plugin_state.lock() {
                    if let Some(cfg) = ps.slot_configs.get_mut(*idx) {
                        cfg.source_code = src.clone();
                    }
                }
            }
        });
    }
}

/// Create the plugin editor.
pub fn create(
    preset_manager: Arc<Mutex<PresetManager>>,
    plugin_state: Arc<Mutex<PluginState>>,
    params: Arc<SongWalkerParams>,
    editor_state: Arc<ViziaState>,
) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        assets::register_noto_sans_light(cx);
        assets::register_noto_sans_thin(cx);

        // Load our Catppuccin Mocha theme
        if let Err(err) = cx.add_stylesheet(include_style!("src/editor/theme.css")) {
            nih_plug::debug::nih_log!("Failed to load theme CSS: {:?}", err);
        }

        // Build the data model
        Data {
            params: params.clone(),
            preset_manager: preset_manager.clone(),
            plugin_state: plugin_state.clone(),
            active_tab: 0,
            search_text: String::new(),
            category_filter: String::new(),
        }
        .build(cx);

        // Main layout: header, content (browser + rack/settings), status bar
        VStack::new(cx, |cx| {
            // ── Header ──
            build_header(cx);

            // ── Body: browser panel + main content ──
            HStack::new(cx, |cx| {
                // Left: browser
                browser::build(cx);

                // Right: slot rack or settings
                VStack::new(cx, |cx| {
                    // Tab content based on active_tab
                    Binding::new(cx, Data::active_tab, |cx, tab| {
                        let tab_val = tab.get(cx);
                        if tab_val == 0 {
                            slot_rack::build(cx);
                        } else {
                            build_settings(cx);
                        }
                    });
                })
                .width(Stretch(1.0))
                .height(Stretch(1.0));
            })
            .height(Stretch(1.0));

            // ── Status bar ──
            build_status_bar(cx);
        })
        .width(Stretch(1.0))
        .height(Stretch(1.0));

        // Resize handle (must be last — changes window size, not scale)
        WindowResizeHandle::new(cx, WINDOW_SIZE.clone(), (MIN_WIDTH, MIN_HEIGHT));
    })
}

/// Build the header bar.
fn build_header(cx: &mut Context) {
    HStack::new(cx, |cx| {
        // Logo / title
        Label::new(cx, "SongWalker")
            .class("title");
        Label::new(cx, "VSTi")
            .class("subtitle");

        // Tab buttons
        Button::new(
            cx,
            |cx| cx.emit(AppEvent::SetTab(0)),
            |cx| Label::new(cx, "Slot Rack"),
        )
        .class("tab-button")
        .checked(Data::active_tab.map(|t| *t == 0));

        Button::new(
            cx,
            |cx| cx.emit(AppEvent::SetTab(1)),
            |cx| Label::new(cx, "\u{2699} Settings"),
        )
        .class("tab-button")
        .checked(Data::active_tab.map(|t| *t == 1));

        // Right-aligned items
        HStack::new(cx, |cx| {
            Label::new(cx, "\u{2665} Donate")
                .class("donate-link");
        })
        .class("header-right");
    })
    .id("header-inner")
    .width(Stretch(1.0));
}

/// Build the bottom status bar.
fn build_status_bar(cx: &mut Context) {
    HStack::new(cx, |cx| {
        Label::new(cx, "Ready")
            .class("status-text")
            .class("ready");
        Label::new(cx, "Voices: 0/256")
            .class("status-text");
        Label::new(cx, "CPU: 0.0%")
            .class("status-text");
        Label::new(cx, "Cache: 0 MB")
            .class("status-text");
    })
    .id("status-bar");
}

/// Build the settings panel.
fn build_settings(cx: &mut Context) {
    VStack::new(cx, |cx| {
        Label::new(cx, "Settings")
            .class("heading");

        Element::new(cx)
            .height(Pixels(1.0))
            .width(Stretch(1.0))
            .background_color(Color::rgb(49, 50, 68));

        Label::new(cx, "Master Volume:");
        // TODO: ParamSlider for master_volume
        // ParamSlider::new(cx, Data::params, |p| &p.master_volume);

        Label::new(cx, "Master Pan:");
        // TODO: ParamSlider for master_pan

        Label::new(cx, "Max Voices:");
        // TODO: ParamSlider for max_voices

        Element::new(cx)
            .height(Pixels(1.0))
            .width(Stretch(1.0))
            .background_color(Color::rgb(49, 50, 68));

        HStack::new(cx, |cx| {
            Label::new(cx, "License:");
            Label::new(cx, "GPL-3.0 — Free & Open Source")
                .color(Color::rgb(166, 227, 161));
        })
        .class("settings-row");

        HStack::new(cx, |cx| {
            Label::new(cx, "Version:");
            Label::new(cx, env!("CARGO_PKG_VERSION"));
        })
        .class("settings-row");
    })
    .id("settings-panel");
}
