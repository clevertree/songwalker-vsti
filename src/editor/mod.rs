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
use std::sync::atomic::{AtomicU32, Ordering};

use crossbeam_channel::{Receiver, Sender};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};

// ── Parameter abstraction ────────────────────────────────────

/// Trait for accessing global parameters from both plugin and standalone contexts.
///
/// The plugin implementation wraps `ParamSetter` + `SongWalkerParams` from nih-plug.
/// The standalone implementation wraps atomic values shared with the audio thread.
pub trait GlobalParams {
    fn master_volume_gain(&self) -> f32;
    fn set_master_volume_gain(&self, gain: f32);
    fn max_voices(&self) -> i32;
    fn set_max_voices(&self, v: i32);
    fn pitch_bend_range(&self) -> i32;
    fn set_pitch_bend_range(&self, v: i32);
}

/// Plugin-side implementation — wraps nih-plug's ParamSetter for DAW automation.
pub struct PluginGlobalParams<'a> {
    pub setter: &'a ParamSetter<'a>,
    pub params: &'a crate::params::SongWalkerParams,
}

impl GlobalParams for PluginGlobalParams<'_> {
    fn master_volume_gain(&self) -> f32 {
        self.params.master_volume.value()
    }
    fn set_master_volume_gain(&self, gain: f32) {
        self.setter.begin_set_parameter(&self.params.master_volume);
        self.setter.set_parameter(&self.params.master_volume, gain);
        self.setter.end_set_parameter(&self.params.master_volume);
    }
    fn max_voices(&self) -> i32 {
        self.params.max_voices.value()
    }
    fn set_max_voices(&self, v: i32) {
        self.setter.begin_set_parameter(&self.params.max_voices);
        self.setter.set_parameter(&self.params.max_voices, v);
        self.setter.end_set_parameter(&self.params.max_voices);
    }
    fn pitch_bend_range(&self) -> i32 {
        self.params.pitch_bend_range.value()
    }
    fn set_pitch_bend_range(&self, v: i32) {
        self.setter.begin_set_parameter(&self.params.pitch_bend_range);
        self.setter.set_parameter(&self.params.pitch_bend_range, v);
        self.setter.end_set_parameter(&self.params.pitch_bend_range);
    }
}

// ── Standalone device state ──────────────────────────────────

/// Audio and MIDI device info for the standalone Settings panel.
/// Only present when running as standalone (not as a VST3/CLAP plugin).
pub struct DeviceState {
    pub audio_device_names: Vec<String>,
    pub selected_audio_idx: usize,
    pub midi_input_names: Vec<String>,
    pub selected_midi_idx: Option<usize>,
    /// Set by UI — the standalone app checks this after draw and performs the switch.
    pub pending_audio_switch: Option<String>,
    /// Set by UI — the standalone app checks this after draw and performs the switch.
    pub pending_midi_switch: Option<String>,
    /// Set by UI — standalone app refreshes device lists.
    pub needs_refresh: bool,
}

use crate::params::SongWalkerParams;
use crate::preset::manager::PresetManager;
use crate::preset::instance::PresetInstance;
use crate::state::PluginState;

/// Events sent from the editor UI to the audio thread.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// Trigger a note-on on a specific slot.
    NoteOn { slot_index: usize, note: u8, velocity: f32 },
    /// Release a note on a specific slot.
    NoteOff { slot_index: usize, note: u8 },
    /// Stop all preview playback.
    StopPreview,
}

/// Event sent when a preset has been fully loaded (samples decoded) on a
/// background thread.  Delivered to the audio thread via a dedicated channel
/// so it never blocks and the heavy `PresetInstance` is transferred by
/// ownership rather than cloned.
pub struct PresetLoadedEvent {
    /// Which audio slot to load the preset into.
    pub slot_index: usize,
    /// Preset identifier string ("library/path").
    pub preset_id: Arc<String>,
    /// Fully-decoded preset ready for the audio thread.
    pub instance: Arc<PresetInstance>,
    /// If `Some(note)`, trigger a NoteOn at this note immediately after
    /// loading (used by the preview play button).
    pub play_note: Option<u8>,
}

/// The application icon (PNG), embedded at compile time.
pub(crate) const ICON_PNG: &[u8] = include_bytes!("../../media/icon.png");

/// Scale a base size value by the current zoom level.
#[inline]
/// Uniformly scale a value by the editor's zoom level.
/// Deprecated in favor of ctx.set_pixels_per_point(), which does this
/// automatically for all logical units. Returning base value to avoid double-scaling.
pub fn zs(base: f32, _zoom: f32) -> f32 {
    base
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
    audio_preset_loaded_tx: Sender<PresetLoadedEvent>,
    ui_preset_loaded_tx: Sender<PresetLoadedEvent>,
    ui_preset_loaded_rx: Receiver<PresetLoadedEvent>,
    status_text: Arc<Mutex<String>>,
    visualizer_state: Arc<visualizer::VisualizerState>,
    voice_count: Arc<AtomicU32>,
) -> Option<Box<dyn Editor>> {
    let egui_state_for_resize = editor_state.clone();

    create_egui_editor(
        editor_state,
        EditorState {
            egui_state: Some(egui_state_for_resize),
            preset_manager,
            plugin_state,
            current_tab: EditorTab::SlotRack,
            browser_state: browser::BrowserState::default(),
            slot_rack_state: slot_rack::SlotRackState::default(),
            piano_state: piano::PianoState::default(),
            event_tx,
            audio_preset_loaded_tx,
            ui_preset_loaded_tx,
            ui_preset_loaded_rx,
            status_text,
            visualizer_state,
            voice_count,
            zoom_level: 1.0,
            resize_drag_start: None,
            active_presets_ui: std::collections::HashMap::new(),
            device_state: None,
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
            let gp = PluginGlobalParams { setter, params: &params };
            draw_editor(ctx, &gp, state);
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
    /// nih-plug EguiState for plugin window resize. None in standalone mode.
    pub egui_state: Option<Arc<EguiState>>,
    pub preset_manager: Arc<Mutex<PresetManager>>,
    pub plugin_state: Arc<Mutex<PluginState>>,
    pub current_tab: EditorTab,
    pub browser_state: browser::BrowserState,
    pub slot_rack_state: slot_rack::SlotRackState,
    pub piano_state: piano::PianoState,
    /// Channel for sending events (note on/off, preview) to the audio thread.
    pub event_tx: Sender<EditorEvent>,
    /// Channel for sending fully-loaded presets to the audio thread.
    pub audio_preset_loaded_tx: Sender<PresetLoadedEvent>,
    /// Channel for sending presets from background threads to the UI.
    pub ui_preset_loaded_tx: Sender<PresetLoadedEvent>,
    /// Channel for receiving presets from background threads in the UI.
    pub ui_preset_loaded_rx: Receiver<PresetLoadedEvent>,
    /// Shared status text displayed in the footer bar.
    pub status_text: Arc<Mutex<String>>,
    /// Shared visualizer state (lock-free for audio thread).
    pub visualizer_state: Arc<visualizer::VisualizerState>,
    /// Live voice count from the audio thread.
    pub voice_count: Arc<AtomicU32>,
    /// UI zoom level (1.0 = 100%, range 0.5–2.0).
    pub zoom_level: f32,
    /// Tracks the drag anchor for window resize: (start_pointer_pos, start_window_size).
    pub resize_drag_start: Option<(egui::Pos2, egui::Vec2)>,
    /// Tracks which presets are currently active in each slot on the UI side.
    /// This prevents the audio thread from being the last one to drop the Arcs,
    /// avoiding real-time allocation/deallocation panics.
    pub active_presets_ui: std::collections::HashMap<usize, (Arc<String>, Arc<PresetInstance>)>,
    /// Standalone-only: available audio/MIDI devices and switch commands.
    pub device_state: Option<Box<DeviceState>>,
}

/// Apply the Catppuccin Mocha theme to egui, matching the web editor CSS.
pub(crate) fn apply_theme(ctx: &egui::Context) {
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

/// Apply a zoom level change and resize the window proportionally.
fn apply_zoom_change(ctx: &egui::Context, state: &mut EditorState, old_zoom: f32) {
    let new_zoom = state.zoom_level;
    if (new_zoom - old_zoom).abs() > 0.001 {
        let new_w = (EDITOR_WIDTH as f32 * new_zoom).round() as u32;
        let new_h = (EDITOR_HEIGHT as f32 * new_zoom).round() as u32;
        request_resize(ctx, state, new_w.max(MIN_WIDTH as u32), new_h.max(MIN_HEIGHT as u32));
    }
}

/// Request a window resize, abstracting over plugin (EguiState) and standalone (ViewportCommand).
fn request_resize(ctx: &egui::Context, state: &EditorState, width: u32, height: u32) {
    if let Some(ref es) = state.egui_state {
        es.set_requested_size((width, height));
    } else {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
            egui::Vec2::new(width as f32, height as f32),
        ));
    }
}

/// Draw the complete editor UI.
/// Called from both the nih-plug plugin editor and the standalone eframe app.
pub(crate) fn draw_editor(
    ctx: &egui::Context,
    params: &dyn GlobalParams,
    state: &mut EditorState,
) {
    let z = state.zoom_level;
    ctx.set_pixels_per_point(z);

    // Request continuous repaint so the visualizer updates in real-time.
    // Without this, the UI only repaints on user interaction and the
    // peak meters / waveform stay stale.
    ctx.request_repaint();

    // Decay visualizer peaks smoothly over time (60fps assumed, lock-free)
    state.visualizer_state.decay_levels(0.92); // Approx 500ms decay

    // --- Drain loaded presets (background thread → UI → audio thread) ---
    while let Ok(loaded) = state.ui_preset_loaded_rx.try_recv() {
        nih_plug::debug::nih_log!("[UI] Received PresetLoadedEvent for {} into slot {}, play_note={:?}", loaded.preset_id, loaded.slot_index, loaded.play_note);
        // Keep a reference on the UI side to prevent deallocation on the audio thread
        state.active_presets_ui.insert(
            loaded.slot_index,
            (loaded.preset_id.clone(), loaded.instance.clone()),
        );
        // Forward a clone (or the original, since we have clones in the map) to the audio thread
        match state.audio_preset_loaded_tx.try_send(loaded) {
            Ok(()) => nih_plug::debug::nih_log!("[UI] Forwarded preset to audio thread"),
            Err(e) => nih_plug::debug::nih_log!("[UI] FAILED to forward preset to audio thread: {:?}", e),
        }
    }

    let prev_zoom = state.zoom_level;

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

    // Apply zoom by setting pixels per point (handles all scaling automatically)
    // No need for apply_zoom_to_style() — ctx.set_pixels_per_point() does this.

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
            egui::ScrollArea::horizontal()
                .show(ui, |ui| {
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
            egui::ScrollArea::horizontal()
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = zs(12.0, z);

                        // Dynamic status message from loading operations / errors
                        let status_msg = state.status_text.lock().map(|s| s.clone()).unwrap_or_default();
                        if status_msg.is_empty() {
                            ui.label(
                                egui::RichText::new("Ready")
                                    .color(colors::GREEN)
                                    .size(zs(11.0, z))
                                    .family(egui::FontFamily::Monospace),
                            );
                        } else {
                            let is_error = status_msg.starts_with('\u{26a0}') || status_msg.starts_with("Error");
                            let color = if is_error { colors::RED } else { colors::TEAL };
                            ui.label(
                                egui::RichText::new(&status_msg)
                                    .color(color)
                                    .size(zs(11.0, z))
                                    .family(egui::FontFamily::Monospace),
                            );
                        }

                        ui.label(
                            egui::RichText::new(format!("Voices: {}/256", state.voice_count.load(Ordering::Relaxed)))
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

    // --- Output visualizer (right side panel, like web editor) ---
    egui::SidePanel::right("visualizer_panel")
        .default_width(130.0)
        .min_width(80.0)
        .max_width(250.0)
        .resizable(true)
        .frame(
            egui::Frame::NONE
                .fill(colors::CRUST)
                .inner_margin(egui::Margin::symmetric(zs(4.0, z) as i8, zs(4.0, z) as i8)),
        )
        .show(ctx, |ui| {
            // VisualizerState is now lock-free, always accessible
            visualizer::draw(ui, &state.visualizer_state);
        });

    egui::SidePanel::left("browser_panel")
        .default_width(zs(200.0, z))
        .min_width(160.0)
        .max_width(zs(400.0, z))
        .resizable(true)
        .frame(
            egui::Frame::NONE
                .fill(colors::MANTLE)
                .inner_margin(egui::Margin::symmetric(zs(10.0, z) as i8, zs(8.0, z) as i8)),
        )
        .show(ctx, |ui| {
            browser::draw(ui, state, z);
        });

    // --- Central content: Slot rack or settings ---
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                match state.current_tab {
                    EditorTab::SlotRack => {
                        slot_rack::draw(ui, state, z);
                    }
                    EditorTab::Settings => {
                        draw_settings(ui, state, params);
                    }
                }
            });
    });

    // --- Resize corner (bottom-right) ---
    // Uses delta-based tracking to avoid CentralPanel margin coordinate issues.
    // Calls EguiState::set_requested_size() which feeds into nih_plug_egui's
    // internal resize pipeline (queue.resize + ViewportCommand::InnerSize).
    draw_resize_corner(ctx, state);

    // If zoom level changed this frame, resize the window proportionally
    apply_zoom_change(ctx, state, prev_zoom);
}

/// Draw a draggable resize corner in the bottom-right of the window.
/// Uses delta-based calculation: on drag start, records the pointer position
/// and current window size. On drag move, computes new_size = start_size + delta.
fn draw_resize_corner(ctx: &egui::Context, state: &mut EditorState) {
    let screen_rect = ctx.screen_rect();
    let corner_size = egui::Vec2::splat(20.0);
    let corner_rect = egui::Rect::from_min_size(
        screen_rect.max - corner_size,
        corner_size,
    );

    // Paint the resize corner lines
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("resize_corner_painter"),
    ));

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

                    request_resize(ctx, state, new_size.x.round() as u32, new_size.y.round() as u32);
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
    params: &dyn GlobalParams,
) {
    ui.heading(egui::RichText::new("Settings").color(colors::TEXT));
    ui.separator();

    // --- Audio / MIDI device selection (standalone only) ---
    if let Some(ref mut ds) = state.device_state {
        ui.label(egui::RichText::new("Audio Output:").color(colors::SUBTEXT0));
        let current_name = ds.audio_device_names.get(ds.selected_audio_idx)
            .cloned()
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt("audio_device_combo")
            .selected_text(&current_name)
            .show_ui(ui, |ui| {
                for (idx, name) in ds.audio_device_names.iter().enumerate() {
                    if ui.selectable_label(idx == ds.selected_audio_idx, name).clicked() {
                        ds.selected_audio_idx = idx;
                        ds.pending_audio_switch = Some(name.clone());
                    }
                }
            });

        ui.add_space(4.0);

        ui.label(egui::RichText::new("MIDI Input:").color(colors::SUBTEXT0));
        let midi_current = ds.selected_midi_idx
            .and_then(|i| ds.midi_input_names.get(i).cloned())
            .unwrap_or_else(|| "None".into());
        egui::ComboBox::from_id_salt("midi_device_combo")
            .selected_text(&midi_current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(ds.selected_midi_idx.is_none(), "None").clicked() {
                    ds.selected_midi_idx = None;
                    ds.pending_midi_switch = Some(String::new());
                }
                for (idx, name) in ds.midi_input_names.iter().enumerate() {
                    if ui.selectable_label(ds.selected_midi_idx == Some(idx), name).clicked() {
                        ds.selected_midi_idx = Some(idx);
                        ds.pending_midi_switch = Some(name.clone());
                    }
                }
            });

        if ui.button("↻ Refresh Devices").clicked() {
            ds.needs_refresh = true;
        }

        ui.separator();
    }

    ui.label("Library URL:");
    if let Ok(mut pm) = state.preset_manager.lock() {
        let mut url = pm.base_url.clone();
        if ui.text_edit_singleline(&mut url).changed() {
            pm.base_url = url;
        }
    }

    ui.separator();

    // Master Volume slider
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Master Volume:")
                .color(colors::SUBTEXT0),
        );
        let vol_db = nih_plug::util::gain_to_db(params.master_volume_gain());
        let mut vol_db_val = vol_db;
        let slider = egui::Slider::new(&mut vol_db_val, -60.0..=6.0)
            .suffix(" dB")
            .text("");
        if ui.add(slider).changed() {
            params.set_master_volume_gain(nih_plug::util::db_to_gain(vol_db_val));
        }
    });

    ui.separator();

    // Max Voices slider
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Max Voices:")
                .color(colors::SUBTEXT0),
        );
        let mut voices = params.max_voices();
        let slider = egui::Slider::new(&mut voices, 8..=1024)
            .text("");
        if ui.add(slider).changed() {
            params.set_max_voices(voices);
        }
    });

    ui.separator();

    // Pitch Bend Range
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Pitch Bend Range:")
                .color(colors::SUBTEXT0),
        );
        let mut bend = params.pitch_bend_range();
        let slider = egui::Slider::new(&mut bend, 1..=48)
            .suffix(" st")
            .text("");
        if ui.add(slider).changed() {
            params.set_pitch_bend_range(bend);
        }
    });

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
