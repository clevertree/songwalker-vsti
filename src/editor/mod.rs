//! Plugin editor UI using egui.
//!
//! Layout mirrors the SongWalker web editor:
//! - Left panel: Preset browser (search, libraries, categories, instruments)
//! - Right panel: Slot rack (Kontakt-style) with inline editors
//! - Bottom: Visualizer and status bar

pub mod browser;
pub mod code_editor;
pub mod piano;
pub mod slot_rack;
pub mod visualizer;

use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};

use crate::params::SongWalkerParams;
use crate::preset::manager::PresetManager;
use crate::state::PluginState;

/// Events sent from the editor UI to the audio thread.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// Trigger a note-on on a specific slot.
    NoteOn { slot_index: usize, note: u8, velocity: f32 },
    /// Release a note on a specific slot.
    NoteOff { slot_index: usize, note: u8 },
    /// Play a preset preview (C4 E4 G4 C5 arpeggio) on a specific slot.
    PreviewPreset { slot_index: usize },
    /// Stop all preview playback.
    StopPreview,
}

/// The application icon (PNG), embedded at compile time.
const ICON_PNG: &[u8] = include_bytes!("../../media/icon.png");

/// Scale a base size value by the current zoom level.
#[inline]
pub fn zs(base: f32, zoom: f32) -> f32 {
    base * zoom
}

/// Default editor window size.
const EDITOR_WIDTH: u32 = 800;
const EDITOR_HEIGHT: u32 = 600;
/// Minimum resize dimensions.
const MIN_WIDTH: f32 = 400.0;
const MIN_HEIGHT: f32 = 300.0;

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

/// Create a default EguiState for use in params persistence.
pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(EDITOR_WIDTH, EDITOR_HEIGHT)
}

/// Create the plugin editor.
pub fn create(
    preset_manager: Arc<Mutex<PresetManager>>,
    plugin_state: Arc<Mutex<PluginState>>,
    params: Arc<SongWalkerParams>,
    editor_state: Arc<EguiState>,
    event_tx: Sender<EditorEvent>,
) -> Option<Box<dyn Editor>> {
    let egui_state_for_resize = editor_state.clone();

    create_egui_editor(
        editor_state,
        EditorState {
            egui_state: egui_state_for_resize,
            preset_manager,
            plugin_state,
            current_tab: EditorTab::SlotRack,
            browser_state: browser::BrowserState::default(),
            slot_rack_state: slot_rack::SlotRackState::default(),
            piano_state: piano::PianoState::default(),
            event_tx,
            zoom_level: 1.0,
            resize_drag_start: None,
        },
        |ctx, _state| {
            // Apply dark theme on init
            apply_theme(ctx);

            // Set window icon from embedded PNG
            if let Ok(img) = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
            {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let icon_data = egui::IconData {
                    rgba: rgba.into_raw(),
                    width: w,
                    height: h,
                };
                ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(
                    icon_data,
                ))));
            }
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
    pub egui_state: Arc<EguiState>,
    pub preset_manager: Arc<Mutex<PresetManager>>,
    pub plugin_state: Arc<Mutex<PluginState>>,
    pub current_tab: EditorTab,
    pub browser_state: browser::BrowserState,
    pub slot_rack_state: slot_rack::SlotRackState,
    pub piano_state: piano::PianoState,
    /// Channel for sending events (note on/off, preview) to the audio thread.
    pub event_tx: Sender<EditorEvent>,
    /// UI zoom level (1.0 = 100%, range 0.5–2.0).
    pub zoom_level: f32,
    /// Tracks the drag anchor for window resize: (start_pointer_pos, start_window_size).
    pub resize_drag_start: Option<(egui::Pos2, egui::Vec2)>,
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

/// Apply zoom level to the egui style by scaling font sizes and spacing.
/// Called each frame so the UI reflects the current zoom.
fn apply_zoom_to_style(ctx: &egui::Context, zoom: f32) {
    let mut style = (*ctx.style()).clone();

    // Scale all text styles
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Small,     FontId::new(zs(10.0, zoom), FontFamily::Proportional)),
        (TextStyle::Body,      FontId::new(zs(14.0, zoom), FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(zs(13.0, zoom), FontFamily::Monospace)),
        (TextStyle::Button,    FontId::new(zs(14.0, zoom), FontFamily::Proportional)),
        (TextStyle::Heading,   FontId::new(zs(20.0, zoom), FontFamily::Proportional)),
    ]
    .into();

    // Scale spacing
    style.spacing.item_spacing = egui::vec2(zs(6.0, zoom), zs(4.0, zoom));
    style.spacing.button_padding = egui::vec2(zs(6.0, zoom), zs(3.0, zoom));
    style.spacing.window_margin = egui::Margin::same(zs(8.0, zoom) as i8);
    style.spacing.indent = zs(18.0, zoom);

    ctx.set_style(style);
}

/// Draw the complete editor UI.
fn draw_editor(
    ctx: &egui::Context,
    setter: &ParamSetter,
    state: &mut EditorState,
    params: &SongWalkerParams,
) {
    // Handle Ctrl+= / Ctrl+- / Ctrl+0 for zoom
    ctx.input(|i| {
        if i.modifiers.command {
            for event in &i.events {
                if let egui::Event::Key { key, pressed: true, .. } = event {
                    match key {
                        egui::Key::Plus | egui::Key::Equals => {
                            state.zoom_level = (state.zoom_level + 0.1).min(2.0);
                        }
                        egui::Key::Minus => {
                            state.zoom_level = (state.zoom_level - 0.1).max(0.5);
                        }
                        egui::Key::Num0 => {
                            state.zoom_level = 1.0;
                        }
                        _ => {}
                    }
                }
            }
            // Ctrl+scroll wheel for zoom
            if i.smooth_scroll_delta.y != 0.0 {
                let delta = i.smooth_scroll_delta.y * 0.002;
                state.zoom_level = (state.zoom_level + delta).clamp(0.5, 2.0);
            }
        }
    });

    // Apply zoom by scaling the egui style (font sizes + spacing)
    apply_zoom_to_style(ctx, state.zoom_level);

    let z = state.zoom_level;

    // --- Header bar ---
    egui::TopBottomPanel::top("header")
        .frame(
            egui::Frame::NONE
                .fill(colors::BASE)
                .inner_margin(egui::Margin::symmetric(zs(16.0, z) as i8, zs(8.0, z) as i8))
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.set_clip_rect(ui.max_rect());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SongWalker")
                        .color(colors::BLUE)
                        .strong()
                        .size(zs(16.0, z)),
                );
                ui.label(
                    egui::RichText::new("VSTi")
                        .color(colors::SUBTEXT0)
                        .size(zs(12.0, z)),
                );
                ui.add_space(zs(8.0, z));

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

                // Piano keyboard toggle
                let piano_color = if state.piano_state.visible { colors::BLUE } else { colors::SUBTEXT0 };
                if ui
                    .selectable_label(
                        state.piano_state.visible,
                        egui::RichText::new("Piano").color(piano_color).size(zs(14.0, z)),
                    )
                    .clicked()
                {
                    state.piano_state.visible = !state.piano_state.visible;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.hyperlink_to(
                        egui::RichText::new("♥ Donate").color(colors::PINK).size(zs(12.0, z)),
                        "https://github.com/sponsors/clevertree",
                    );

                    ui.add_space(zs(8.0, z));

                    // Zoom controls
                    if ui
                        .button(egui::RichText::new("+").color(colors::SUBTEXT0).size(zs(12.0, z)))
                        .on_hover_text("Zoom in")
                        .clicked()
                    {
                        state.zoom_level = (state.zoom_level + 0.1).min(2.0);
                    }
                    ui.label(
                        egui::RichText::new(format!("{}%", (state.zoom_level * 100.0) as u32))
                            .color(colors::SUBTEXT0)
                            .size(zs(10.0, z)),
                    );
                    if ui
                        .button(egui::RichText::new("−").color(colors::SUBTEXT0).size(zs(12.0, z)))
                        .on_hover_text("Zoom out")
                        .clicked()
                    {
                        state.zoom_level = (state.zoom_level - 0.1).max(0.5);
                    }
                });
            });
            // Bottom border
            let rect = ui.max_rect();
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left(), rect.bottom()),
                    egui::pos2(rect.right(), rect.bottom()),
                ],
                egui::Stroke::new(1.0, colors::SURFACE0),
            );
        });

    // --- Status bar ---
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            egui::Frame::NONE
                .fill(colors::MANTLE)
                .inner_margin(egui::Margin::symmetric(zs(12.0, z) as i8, zs(3.0, z) as i8)),
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
                ui.spacing_mut().item_spacing.x = zs(12.0, z);
                ui.label(
                    egui::RichText::new("Ready")
                        .color(colors::GREEN)
                        .size(zs(11.0, z))
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(
                    egui::RichText::new("Voices: 0/256")
                        .color(colors::SUBTEXT0)
                        .size(zs(11.0, z))
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(
                    egui::RichText::new("CPU: 0.0%")
                        .color(colors::SUBTEXT0)
                        .size(zs(11.0, z))
                        .family(egui::FontFamily::Monospace),
                );
                ui.label(
                    egui::RichText::new("Cache: 0 MB")
                        .color(colors::SUBTEXT0)
                        .size(zs(11.0, z))
                        .family(egui::FontFamily::Monospace),
                );
            });
        });

    // --- Piano keyboard (togglable bottom panel) ---
    if state.piano_state.visible {
        egui::TopBottomPanel::bottom("piano")
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(colors::CRUST)
                    .inner_margin(egui::Margin::symmetric(zs(8.0, z) as i8, zs(4.0, z) as i8))
                    .stroke(egui::Stroke::new(1.0, colors::SURFACE0)),
            )
            .show(ctx, |ui| {
                piano::draw(ui, state, z);
            });
    }

    // --- Left panel: Preset browser ---
    egui::SidePanel::left("browser_panel")
        .default_width(zs(200.0, z))
        .min_width(zs(160.0, z))
        .max_width(zs(300.0, z))
        .resizable(true)
        .frame(
            egui::Frame::NONE
                .fill(colors::MANTLE)
                .inner_margin(egui::Margin::symmetric(zs(10.0, z) as i8, zs(8.0, z) as i8)),
        )
        .show(ctx, |ui| {
            ui.set_clip_rect(ui.max_rect());
            browser::draw(ui, state, z);
        });

    // --- Central content: Slot rack or settings ---
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.set_clip_rect(ui.max_rect());
        match state.current_tab {
            EditorTab::SlotRack => {
                slot_rack::draw(ui, state, z);
            }
            EditorTab::Settings => {
                draw_settings(ui, state, setter, params);
            }
        }
    });

    // --- Resize corner (bottom-right) ---
    // Uses delta-based tracking to avoid CentralPanel margin coordinate issues.
    // Calls EguiState::set_requested_size() which feeds into nih_plug_egui's
    // internal resize pipeline (queue.resize + ViewportCommand::InnerSize).
    draw_resize_corner(ctx, state);
}

/// Draw a draggable resize corner in the bottom-right of the window.
/// Uses delta-based calculation: on drag start, records the pointer position
/// and current window size. On drag move, computes new_size = start_size + delta.
fn draw_resize_corner(ctx: &egui::Context, state: &mut EditorState) {
    let screen_rect = ctx.screen_rect();
    let corner_size = egui::Vec2::splat(16.0);
    let corner_rect = egui::Rect::from_min_size(
        screen_rect.max - corner_size,
        corner_size,
    );

    // Paint the resize corner lines
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("resize_corner_painter"),
    ));
    let _corner_id = egui::Id::new("sw_resize_corner");

    // Use an Area to create an interaction layer on top of everything
    egui::Area::new(egui::Id::new("resize_area"))
        .fixed_pos(corner_rect.min)
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(corner_size, egui::Sense::drag());

            // Paint resize corner diagonal lines
            let stroke = ui.style().interact(&response).fg_stroke;
            let cp = rect.max - egui::Vec2::splat(2.0);
            let mut w = 2.0;
            while w <= rect.width() && w <= rect.height() {
                painter.line_segment(
                    [egui::pos2(cp.x - w, cp.y), egui::pos2(cp.x, cp.y - w)],
                    stroke,
                );
                w += 4.0;
            }

            // Delta-based resize logic
            if response.drag_started() {
                // Record the anchor: where the mouse started and the current window size
                if let Some(pointer_pos) = response.interact_pointer_pos() {
                    state.resize_drag_start = Some((pointer_pos, screen_rect.size()));
                }
            }

            if response.dragged() {
                if let (Some((start_pos, start_size)), Some(pointer_pos)) =
                    (state.resize_drag_start, response.interact_pointer_pos())
                {
                    let delta = pointer_pos - start_pos;
                    let new_size = (start_size + delta)
                        .max(egui::Vec2::new(MIN_WIDTH, MIN_HEIGHT));

                    state.egui_state.set_requested_size((
                        new_size.x.round() as u32,
                        new_size.y.round() as u32,
                    ));
                }
            }

            if response.drag_stopped() {
                state.resize_drag_start = None;
            }

            // Show resize cursor when hovering
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
        });
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
