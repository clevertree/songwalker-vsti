//! Plugin editor UI using egui.
//!
//! Layout mirrors the SongWalker web editor:
//! - Left panel: Preset browser (search, libraries, categories, instruments)
//! - Right panel: Slot rack (Kontakt-style) with inline editors
//! - Bottom: Visualizer and status bar

pub mod browser;
pub mod code_editor;
pub mod slot_rack;
pub mod visualizer;

use std::sync::{Arc, Mutex};

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};

use crate::params::SongWalkerParams;
use crate::preset::manager::PresetManager;
use crate::state::PluginState;

/// Default editor window size.
const EDITOR_WIDTH: u32 = 800;
const EDITOR_HEIGHT: u32 = 600;

/// Catppuccin Mocha color palette (matches the web editor).
pub mod colors {
    use nih_plug_egui::egui::Color32;

    pub const BASE: Color32 = Color32::from_rgb(30, 30, 46);
    pub const MANTLE: Color32 = Color32::from_rgb(24, 24, 37);
    pub const CRUST: Color32 = Color32::from_rgb(17, 17, 27);
    pub const SURFACE0: Color32 = Color32::from_rgb(49, 50, 68);
    pub const SURFACE1: Color32 = Color32::from_rgb(69, 71, 90);
    pub const SURFACE2: Color32 = Color32::from_rgb(88, 91, 112);
    pub const TEXT: Color32 = Color32::from_rgb(205, 214, 244);
    pub const SUBTEXT0: Color32 = Color32::from_rgb(166, 173, 200);
    pub const SUBTEXT1: Color32 = Color32::from_rgb(186, 194, 222);
    pub const BLUE: Color32 = Color32::from_rgb(137, 180, 250);
    pub const GREEN: Color32 = Color32::from_rgb(166, 227, 161);
    pub const PEACH: Color32 = Color32::from_rgb(250, 179, 135);
    pub const RED: Color32 = Color32::from_rgb(243, 139, 168);
    pub const MAUVE: Color32 = Color32::from_rgb(203, 166, 247);
    pub const YELLOW: Color32 = Color32::from_rgb(249, 226, 175);
    pub const TEAL: Color32 = Color32::from_rgb(148, 226, 213);
    pub const LAVENDER: Color32 = Color32::from_rgb(180, 190, 254);
    pub const PINK: Color32 = Color32::from_rgb(245, 194, 231);
    pub const OVERLAY0: Color32 = Color32::from_rgb(108, 112, 134);
}

/// Create the plugin editor.
pub fn create(
    preset_manager: Arc<Mutex<PresetManager>>,
    plugin_state: Arc<Mutex<PluginState>>,
    params: Arc<SongWalkerParams>,
) -> Option<Box<dyn Editor>> {
    let egui_state = EguiState::from_size(EDITOR_WIDTH, EDITOR_HEIGHT);

    create_egui_editor(
        egui_state,
        EditorState {
            preset_manager,
            plugin_state,
            current_tab: EditorTab::SlotRack,
            browser_state: browser::BrowserState::default(),
            slot_rack_state: slot_rack::SlotRackState::default(),
        },
        |ctx, _state| {
            // Apply dark theme on init
            apply_theme(ctx);
        },
        move |ctx, setter, state| {
            draw_editor(ctx, setter, state, &params);
        },
    )
}

/// Which tab/panel is active in the main area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    SlotRack,
    Settings,
}

/// Persistent state for the editor (not the audio state).
pub struct EditorState {
    pub preset_manager: Arc<Mutex<PresetManager>>,
    pub plugin_state: Arc<Mutex<PluginState>>,
    pub current_tab: EditorTab,
    pub browser_state: browser::BrowserState,
    pub slot_rack_state: slot_rack::SlotRackState,
}

/// Apply the Catppuccin Mocha theme to egui, matching the web editor CSS.
fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Dark background — matches --bg / --surface
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = colors::BASE;
    style.visuals.window_fill = colors::MANTLE;

    // Widget colors — buttons use --surface bg + --border border (4px radius)
    style.visuals.widgets.noninteractive.bg_fill = colors::SURFACE0;
    style.visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0, colors::TEXT);
    style.visuals.widgets.noninteractive.weak_bg_fill = colors::MANTLE;
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.inactive.bg_fill = colors::MANTLE;
    style.visuals.widgets.inactive.weak_bg_fill = colors::MANTLE;
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0, colors::SURFACE0);
    style.visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0, colors::SUBTEXT0);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.hovered.bg_fill = colors::SURFACE0;
    style.visuals.widgets.hovered.weak_bg_fill = colors::SURFACE0;
    style.visuals.widgets.hovered.fg_stroke =
        egui::Stroke::new(1.0, colors::TEXT);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.active.bg_fill = colors::SURFACE1;
    style.visuals.widgets.active.weak_bg_fill = colors::SURFACE1;
    style.visuals.widgets.active.fg_stroke =
        egui::Stroke::new(1.0, colors::TEXT);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);

    // Selection
    style.visuals.selection.bg_fill = colors::BLUE.linear_multiply(0.3);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, colors::BLUE);

    // Window / panel borders — matching --border (#313244 = SURFACE0)
    style.visuals.window_stroke = egui::Stroke::new(1.0, colors::SURFACE0);
    style.visuals.window_corner_radius = egui::CornerRadius::same(4);

    // Spacing — tighter to match web CSS
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(6.0, 3.0);
    style.spacing.window_margin = egui::Margin::same(8);

    ctx.set_style(style);
}

/// Draw the complete editor UI.
fn draw_editor(
    ctx: &egui::Context,
    setter: &ParamSetter,
    state: &mut EditorState,
    params: &SongWalkerParams,
) {
    // --- Header bar --- (web: padding 1rem 2rem, border-bottom: --border)
    egui::TopBottomPanel::top("header")
        .frame(
            egui::Frame::NONE
                .fill(colors::BASE)
                .inner_margin(egui::Margin::symmetric(16, 8))
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.set_clip_rect(ui.max_rect());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SongWalker")
                        .color(colors::BLUE)
                        .strong()
                        .size(16.0),
                );
                ui.label(
                    egui::RichText::new("VSTi")
                        .color(colors::SUBTEXT0)
                        .size(12.0),
                );
                ui.add_space(8.0);

                if ui
                    .selectable_label(state.current_tab == EditorTab::SlotRack, "Slot Rack")
                    .clicked()
                {
                    state.current_tab = EditorTab::SlotRack;
                }
                if ui
                    .selectable_label(state.current_tab == EditorTab::Settings, "⚙ Settings")
                    .clicked()
                {
                    state.current_tab = EditorTab::Settings;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.hyperlink_to(
                        egui::RichText::new("♥ Donate").color(colors::PINK).size(12.0),
                        "https://github.com/sponsors/clevertree",
                    );
                });
            });
            // Bottom border matching --border
            let rect = ui.max_rect();
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left(), rect.bottom()),
                    egui::pos2(rect.right(), rect.bottom()),
                ],
                egui::Stroke::new(1.0, colors::SURFACE0),
            );
        });

    // --- Status bar --- (web: padding 0.2rem 0.75rem, bg --surface, border-top --border)
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            egui::Frame::NONE
                .fill(colors::MANTLE)
                .inner_margin(egui::Margin::symmetric(12, 3)),
        )
        .show(ctx, |ui| {
            // Top border
            let rect = ui.max_rect();
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left(), rect.top()),
                    egui::pos2(rect.right(), rect.top()),
                ],
                egui::Stroke::new(1.0, colors::SURFACE0),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.label(
                    egui::RichText::new("Ready")
                        .color(colors::GREEN)
                        .size(11.0)
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(
                    egui::RichText::new("Voices: 0/256")
                        .color(colors::SUBTEXT0)
                        .size(11.0)
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(
                    egui::RichText::new("CPU: 0.0%")
                        .color(colors::SUBTEXT0)
                        .size(11.0)
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(
                    egui::RichText::new("Cache: 0 MB")
                        .color(colors::SUBTEXT0)
                        .size(11.0)
                        .family(egui::FontFamily::Monospace),
                );
            });
        });

    // --- Left panel: Preset browser --- (web: bg --surface, border-left --border)
    egui::SidePanel::left("browser_panel")
        .default_width(200.0)
        .min_width(160.0)
        .max_width(260.0)
        .resizable(true)
        .frame(
            egui::Frame::NONE
                .fill(colors::MANTLE)
                .inner_margin(egui::Margin::symmetric(10, 8)),
        )
        .show(ctx, |ui| {
            ui.set_clip_rect(ui.max_rect());
            browser::draw(ui, state);
        });

    // --- Central panel: Slot rack or settings ---
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.set_clip_rect(ui.max_rect());
        match state.current_tab {
            EditorTab::SlotRack => {
                slot_rack::draw(ui, state);
            }
            EditorTab::Settings => {
                draw_settings(ui, state, setter, params);
            }
        }
    });

    // Enable window dragging from any non-interactive area (empty panel backgrounds, etc.)
    // Works in standalone mode; silently ignored when hosted in a DAW that manages the window.
    if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        && !ctx.is_using_pointer()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// Draw the settings panel.
fn draw_settings(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    _setter: &ParamSetter,
    _params: &SongWalkerParams,
) {
    ui.heading(egui::RichText::new("Settings").color(colors::TEXT));
    ui.separator();

    ui.label("Library URL:");
    if let Ok(mut pm) = state.preset_manager.lock() {
        let mut url = pm.base_url.clone();
        if ui.text_edit_singleline(&mut url).changed() {
            pm.base_url = url;
        }
    }

    ui.separator();
    ui.label("Master Volume:");
    // TODO: Use nih-plug setter for master volume knob

    ui.separator();
    ui.label("Max Voices:");
    // TODO: Use nih-plug setter for max voices

    ui.separator();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("License:").color(colors::SUBTEXT0));
        ui.label(egui::RichText::new("GPL-3.0 — Free & Open Source").color(colors::GREEN));
    });

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Version:").color(colors::SUBTEXT0));
        ui.label(egui::RichText::new(env!("CARGO_PKG_VERSION")).color(colors::TEXT));
    });

    ui.separator();
    ui.hyperlink_to("GitHub", "https://github.com/clevertree/songwalker-vsti");
    ui.hyperlink_to("Website", "https://songwalker.org");
}
