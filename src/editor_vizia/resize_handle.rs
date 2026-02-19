//! A resize handle that changes window dimensions without scaling content.
//!
//! Unlike the built-in [`ResizeHandle`](nih_plug_vizia::widgets::ResizeHandle)
//! which uniformly scales the UI via `user_scale_factor`, this widget changes
//! the logical window size directly so content stays the same size and the
//! window simply gets larger or smaller.

use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::GuiContextEvent;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Shared mutable window size used by `ViziaState::size_fn` and this handle.
#[derive(Debug)]
pub struct SharedWindowSize {
    pub width: AtomicU32,
    pub height: AtomicU32,
}

impl SharedWindowSize {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            width: AtomicU32::new(w),
            height: AtomicU32::new(h),
        }
    }

    pub fn load(&self) -> (u32, u32) {
        (self.width.load(Ordering::Relaxed), self.height.load(Ordering::Relaxed))
    }

    pub fn store(&self, w: u32, h: u32) {
        self.width.store(w, Ordering::Relaxed);
        self.height.store(h, Ordering::Relaxed);
    }
}

/// A resize handle placed at the bottom-right corner of the window.
///
/// Dragging it changes window dimensions (more/less visible area) without
/// scaling the UI content.
pub struct WindowResizeHandle {
    shared_size: Arc<SharedWindowSize>,
    min_size: (u32, u32),

    drag_active: bool,
    /// Logical start position of the pointer when drag began.
    start_pos: (f32, f32),
    /// Window size when drag began.
    start_size: (u32, u32),
}

impl WindowResizeHandle {
    pub fn new(
        cx: &mut Context,
        shared_size: Arc<SharedWindowSize>,
        min_size: (u32, u32),
    ) -> Handle<'_, Self> {
        Self {
            shared_size,
            min_size,
            drag_active: false,
            start_pos: (0.0, 0.0),
            start_size: (0, 0),
        }
        .build(cx, |_| {})
    }
}

impl View for WindowResizeHandle {
    fn element(&self) -> Option<&'static str> {
        // Use the same CSS element name so the existing theme.css styles apply
        Some("resize-handle")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match *window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                if intersects_triangle(
                    cx.cache.get_bounds(cx.current()),
                    (cx.mouse().cursorx, cx.mouse().cursory),
                ) {
                    cx.capture();
                    cx.set_active(true);

                    self.drag_active = true;
                    self.start_pos = (cx.mouse().cursorx, cx.mouse().cursory);
                    self.start_size = self.shared_size.load();

                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.drag_active {
                    cx.release();
                    cx.set_active(false);
                    self.drag_active = false;
                }
            }
            WindowEvent::MouseMove(x, y) => {
                cx.set_hover(intersects_triangle(
                    cx.cache.get_bounds(cx.current()),
                    (x, y),
                ));

                if self.drag_active {
                    let dx = x - self.start_pos.0;
                    let dy = y - self.start_pos.1;

                    let new_w = ((self.start_size.0 as f32) + dx)
                        .round()
                        .max(self.min_size.0 as f32) as u32;
                    let new_h = ((self.start_size.1 as f32) + dy)
                        .round()
                        .max(self.min_size.1 as f32) as u32;

                    let current = self.shared_size.load();
                    if current != (new_w, new_h) {
                        self.shared_size.store(new_w, new_h);
                        // Tell nih_plug_vizia to re-query ViziaState::inner_logical_size()
                        // and resize the baseview window accordingly.
                        cx.emit(GuiContextEvent::Resize);
                    }
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if bounds.w == 0.0 || bounds.h == 0.0 {
            return;
        }

        let opacity = cx.opacity();
        let border_width = cx.border_width();

        // Background fill
        let background_color = cx.background_color();
        let mut bg: vg::Color = background_color.into();
        bg.set_alphaf(bg.a * opacity);
        let x = bounds.x + border_width / 2.0;
        let y = bounds.y + border_width / 2.0;
        let w = bounds.w - border_width;
        let h = bounds.h - border_width;

        let mut path = vg::Path::new();
        path.move_to(x, y);
        path.line_to(x, y + h);
        path.line_to(x + w, y + h);
        path.line_to(x + w, y);
        path.close();
        canvas.fill_path(&path, &vg::Paint::color(bg));

        // Triangle indicator
        let mut path = vg::Path::new();
        path.move_to(x, y + h);
        path.line_to(x + w, y + h);
        path.line_to(x + w, y);
        path.move_to(x, y + h);
        path.close();

        let mut color: vg::Color = cx.font_color().into();
        color.set_alphaf(color.a * opacity);
        canvas.fill_path(&path, &vg::Paint::color(color));
    }
}

/// Test whether a point intersects with the triangle of the resize handle.
fn intersects_triangle(bounds: BoundingBox, (x, y): (f32, f32)) -> bool {
    let (p1x, p1y) = bounds.bottom_left();
    let (p2x, p2y) = bounds.top_right();
    let (v1x, v1y) = (p2x - p1x, p2y - p1y);
    ((x - p1x) * v1y) - ((y - p1y) * v1x) >= 0.0
}
