use std::sync::atomic::{AtomicU32, Ordering};

use nih_plug_egui::egui;
use parking_lot::Mutex;

use super::colors;

/// Atomic f32 helper — load f32 from AtomicU32.
#[inline]
fn load_f32(atom: &AtomicU32) -> f32 {
    f32::from_bits(atom.load(Ordering::Relaxed))
}

/// Atomic f32 helper — store f32 into AtomicU32.
#[inline]
fn store_f32(atom: &AtomicU32, val: f32) {
    atom.store(val.to_bits(), Ordering::Relaxed);
}

/// Atomic f32 helper — fetch-max for peak tracking.
#[inline]
fn fetch_max_f32(atom: &AtomicU32, val: f32) {
    let mut current = atom.load(Ordering::Relaxed);
    loop {
        let current_f = f32::from_bits(current);
        if val <= current_f {
            return; // No update needed
        }
        match atom.compare_exchange_weak(current, val.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(x) => current = x,
        }
    }
}

/// Lock-free audio visualizer state.
///
/// Peak/RMS levels use atomics so the audio thread always succeeds.
/// Waveform buffers use parking_lot::Mutex with try_lock on both sides.
pub struct VisualizerState {
    /// Ring buffer for waveform display (locked).
    waveform: Mutex<WaveformBuffer>,
    /// Display width (number of samples).
    width: usize,
    /// Current peak level for Left channel (atomic f32 bits).
    peak_left: AtomicU32,
    /// Current peak level for Right channel (atomic f32 bits).
    peak_right: AtomicU32,
    /// RMS level for Left channel (atomic f32 bits).
    rms_left: AtomicU32,
    /// RMS level for Right channel (atomic f32 bits).
    rms_right: AtomicU32,
}

/// Inner waveform ring buffer (protected by Mutex).
struct WaveformBuffer {
    left: Vec<f32>,
    right: Vec<f32>,
    cursor: usize,
}

impl VisualizerState {
    pub fn new(width: usize) -> Self {
        Self {
            waveform: Mutex::new(WaveformBuffer {
                left: vec![0.0; width],
                right: vec![0.0; width],
                cursor: 0,
            }),
            width,
            peak_left: AtomicU32::new(0),
            peak_right: AtomicU32::new(0),
            rms_left: AtomicU32::new(0),
            rms_right: AtomicU32::new(0),
        }
    }

    /// Push a stereo sample pair into the ring buffer (non-blocking).
    /// Returns true if the push succeeded, false if lock was contended.
    pub fn try_push(&self, left: f32, right: f32) -> bool {
        if let Some(mut wf) = self.waveform.try_lock() {
            let cursor = wf.cursor;
            wf.left[cursor] = left;
            wf.right[cursor] = right;
            wf.cursor = (cursor + 1) % self.width;
            true
        } else {
            false
        }
    }

    /// Update peak and RMS levels (lock-free, always succeeds).
    pub fn update_levels(&self, peak_l: f32, peak_r: f32, rms_l: f32, rms_r: f32) {
        fetch_max_f32(&self.peak_left, peak_l);
        fetch_max_f32(&self.peak_right, peak_r);
        store_f32(&self.rms_left, rms_l);
        store_f32(&self.rms_right, rms_r);
    }

    /// Decay peak levels (call periodically from UI thread).
    pub fn decay_levels(&self, amount: f32) {
        let pl = load_f32(&self.peak_left) * amount;
        let pr = load_f32(&self.peak_right) * amount;
        store_f32(&self.peak_left, if pl < 0.001 { 0.0 } else { pl });
        store_f32(&self.peak_right, if pr < 0.001 { 0.0 } else { pr });
    }

    /// Read current peak levels (lock-free).
    pub fn peak_levels(&self) -> (f32, f32) {
        (load_f32(&self.peak_left), load_f32(&self.peak_right))
    }

    /// Read current RMS levels (lock-free).
    pub fn rms_levels(&self) -> (f32, f32) {
        (load_f32(&self.rms_left), load_f32(&self.rms_right))
    }

    /// Get waveform width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Access waveform data for drawing (may fail if audio thread holds lock).
    pub fn with_waveform<R>(&self, f: impl FnOnce(&[f32], &[f32], usize) -> R) -> Option<R> {
        self.waveform.try_lock().map(|wf| f(&wf.left, &wf.right, wf.cursor))
    }

    /// Clear the waveform buffers and levels.
    pub fn clear(&self) {
        store_f32(&self.peak_left, 0.0);
        store_f32(&self.peak_right, 0.0);
        store_f32(&self.rms_left, 0.0);
        store_f32(&self.rms_right, 0.0);
        if let Some(mut wf) = self.waveform.try_lock() {
            wf.left.fill(0.0);
            wf.right.fill(0.0);
            wf.cursor = 0;
        }
    }
}

/// Draw the output visualizer in a vertical panel (right side, like the web editor).
/// Layout: Peak label → meters → dB text → separator → Output label → waveform.
pub fn draw(ui: &mut egui::Ui, state: &VisualizerState) {
    let panel_width = ui.available_width();
    let (peak_left, peak_right) = state.peak_levels();
    let (rms_left, rms_right) = state.rms_levels();

    // --- Peak section ---
    ui.label(
        egui::RichText::new("Peak")
            .color(colors::SUBTEXT0)
            .size(11.0)
            .strong(),
    );
    ui.add_space(2.0);

    // Two vertical bars side by side
    let meter_height = 80.0_f32.min(ui.available_height() * 0.25);
    let meter_size = egui::vec2(panel_width, meter_height);
    let (meter_rect, _) = ui.allocate_exact_size(meter_size, egui::Sense::hover());

    if ui.is_rect_visible(meter_rect) {
        let painter = ui.painter_at(meter_rect);
        painter.rect_filled(meter_rect, 2.0, colors::SURFACE0);

        let spacing = 4.0;
        let bar_w = ((meter_rect.width() - spacing * 3.0) / 2.0).max(4.0);

        let rect_l = egui::Rect::from_min_size(
            egui::pos2(meter_rect.left() + spacing, meter_rect.top() + spacing),
            egui::vec2(bar_w, meter_rect.height() - spacing * 2.0),
        );
        let rect_r = egui::Rect::from_min_size(
            egui::pos2(rect_l.right() + spacing, meter_rect.top() + spacing),
            egui::vec2(bar_w, meter_rect.height() - spacing * 2.0),
        );

        draw_meter(&painter, rect_l, peak_left, rms_left);
        draw_meter(&painter, rect_r, peak_right, rms_right);
    }

    // Peak dB text
    let max_peak = peak_left.max(peak_right);
    let db_text = if max_peak < 0.0001 {
        "\u{2212}\u{221e} dB".to_string()
    } else {
        format!("{:.1} dB", 20.0 * max_peak.log10())
    };
    let db_color = if max_peak > 1.0 {
        colors::RED
    } else if max_peak > 0.707 {
        colors::YELLOW
    } else {
        colors::GREEN
    };
    ui.label(
        egui::RichText::new(db_text)
            .color(db_color)
            .size(10.0)
            .family(egui::FontFamily::Monospace),
    );

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    // --- Output / Waveform section ---
    ui.label(
        egui::RichText::new("Output")
            .color(colors::SUBTEXT0)
            .size(11.0)
            .strong(),
    );
    ui.add_space(2.0);

    // Waveform takes remaining height
    let waveform_height = ui.available_height().max(40.0);
    let waveform_size = egui::vec2(panel_width, waveform_height);
    let (wf_rect, _) = ui.allocate_exact_size(waveform_size, egui::Sense::hover());

    if ui.is_rect_visible(wf_rect) {
        let painter = ui.painter_at(wf_rect);
        painter.rect_filled(wf_rect, 2.0, colors::CRUST);

        // Center line
        let center_y = wf_rect.center().y;
        painter.line_segment(
            [
                egui::pos2(wf_rect.left(), center_y),
                egui::pos2(wf_rect.right(), center_y),
            ],
            egui::Stroke::new(0.5, colors::SURFACE1),
        );

        let width = state.width() as f32;
        let half_height = wf_rect.height() / 2.0;

        // Try to access waveform data (may fail if audio thread holds lock)
        if let Some(()) = state.with_waveform(|left, right, cursor| {
            // Left channel (teal)
            draw_channel(
                &painter,
                left,
                cursor,
                wf_rect,
                half_height,
                center_y,
                width,
                colors::TEAL.gamma_multiply(0.7),
            );

            // Right channel (mauve)
            draw_channel(
                &painter,
                right,
                cursor,
                wf_rect,
                half_height,
                center_y,
                width,
                colors::MAUVE.gamma_multiply(0.7),
            );
        }) {
            // Waveform drawn successfully
        }

        // Clipping indicator (red border if over 0dB)
        if peak_left > 1.0 || peak_right > 1.0 {
            painter.rect_stroke(wf_rect, 2.0, egui::Stroke::new(1.0, colors::RED), egui::StrokeKind::Outside);
        }
    }
}

fn draw_meter(painter: &egui::Painter, rect: egui::Rect, peak: f32, rms: f32) {
    painter.rect_filled(rect, 1.0, colors::SURFACE0);

    // Draw peak bar first (background, dimmer)
    let peak_h = peak.min(1.0) * rect.height();
    let peak_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - peak_h),
        rect.max
    );

    let peak_color = if peak > 1.0 { colors::RED }
                     else if peak > 0.707 { colors::YELLOW }
                     else { colors::GREEN };

    painter.rect_filled(peak_rect, 1.0, peak_color.gamma_multiply(0.4));

    // Draw RMS bar on top (brighter, always visible since RMS ≤ peak)
    let rms_h = rms.min(1.0) * rect.height();
    let rms_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - rms_h),
        rect.max
    );
    painter.rect_filled(rms_rect, 1.0, peak_color.gamma_multiply(0.8));
}

fn draw_channel(
    painter: &egui::Painter,
    buffer: &[f32],
    cursor: usize,
    rect: egui::Rect,
    half_height: f32,
    center_y: f32,
    width: f32,
    color: egui::Color32,
) {
    let len = buffer.len();
    if len < 2 {
        return;
    }

    let points: Vec<egui::Pos2> = (0..len)
        .map(|i| {
            let idx = (cursor + i) % len;
            let x = rect.left() + (i as f32 / width) * rect.width();
            let y = center_y - buffer[idx].clamp(-1.0, 1.0) * half_height;
            egui::pos2(x, y)
        })
        .collect();

    // Draw as a polyline
    let stroke = egui::Stroke::new(1.0, color);
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let vis = VisualizerState::new(64);
        assert_eq!(vis.width(), 64);
        let (pl, pr) = vis.peak_levels();
        assert_eq!(pl, 0.0);
        assert_eq!(pr, 0.0);
        let (rl, rr) = vis.rms_levels();
        assert_eq!(rl, 0.0);
        assert_eq!(rr, 0.0);
        vis.with_waveform(|left, right, cursor| {
            assert_eq!(left.len(), 64);
            assert_eq!(right.len(), 64);
            assert_eq!(cursor, 0);
        });
    }

    #[test]
    fn test_push_advances_cursor() {
        let vis = VisualizerState::new(4);
        vis.try_push(0.5, -0.5);
        vis.with_waveform(|left, right, cursor| {
            assert_eq!(cursor, 1);
            assert_eq!(left[0], 0.5);
            assert_eq!(right[0], -0.5);
        });
    }

    #[test]
    fn test_push_wraps_cursor() {
        let vis = VisualizerState::new(4);
        for i in 0..5 {
            vis.try_push(i as f32, 0.0);
        }
        // After 5 pushes into a buffer of 4, cursor should be at 1
        vis.with_waveform(|left, _right, cursor| {
            assert_eq!(cursor, 1);
            // The 5th push (value 4.0) overwrote index 0
            assert_eq!(left[0], 4.0);
        });
    }

    #[test]
    fn test_update_levels_peak_accumulates() {
        let vis = VisualizerState::new(4);
        vis.update_levels(0.5, 0.3, 0.2, 0.1);
        let (pl, pr) = vis.peak_levels();
        assert_eq!(pl, 0.5);
        assert_eq!(pr, 0.3);

        // Second call with lower peak — should keep the max
        vis.update_levels(0.3, 0.1, 0.15, 0.05);
        let (pl, pr) = vis.peak_levels();
        assert_eq!(pl, 0.5); // Kept max
        assert_eq!(pr, 0.3); // Kept max

        // Third call with higher peak — should update
        vis.update_levels(0.8, 0.9, 0.4, 0.3);
        let (pl, pr) = vis.peak_levels();
        assert_eq!(pl, 0.8);
        assert_eq!(pr, 0.9);
    }

    #[test]
    fn test_decay_levels() {
        let vis = VisualizerState::new(4);
        vis.update_levels(1.0, 0.5, 0.0, 0.0);
        vis.decay_levels(0.5);
        let (pl, pr) = vis.peak_levels();
        assert_eq!(pl, 0.5);
        assert_eq!(pr, 0.25);
    }

    #[test]
    fn test_decay_levels_floor() {
        let vis = VisualizerState::new(4);
        vis.update_levels(0.0005, 0.002, 0.0, 0.0);
        vis.decay_levels(0.5);
        let (pl, pr) = vis.peak_levels();
        assert_eq!(pl, 0.0); // Snapped to zero (0.0005 * 0.5 = 0.00025 < 0.001)
        assert_eq!(pr, 0.001); // 0.002 * 0.5 = 0.001, at threshold
    }

    #[test]
    fn test_clear() {
        let vis = VisualizerState::new(4);
        vis.try_push(1.0, 1.0);
        vis.try_push(0.5, 0.5);
        vis.update_levels(0.9, 0.8, 0.5, 0.4);

        vis.clear();

        let (pl, pr) = vis.peak_levels();
        assert_eq!(pl, 0.0);
        assert_eq!(pr, 0.0);
        let (rl, rr) = vis.rms_levels();
        assert_eq!(rl, 0.0);
        assert_eq!(rr, 0.0);
        vis.with_waveform(|left, right, cursor| {
            assert_eq!(cursor, 0);
            assert!(left.iter().all(|&v| v == 0.0));
            assert!(right.iter().all(|&v| v == 0.0));
        });
    }
}
