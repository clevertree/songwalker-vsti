use nih_plug_egui::egui;

use super::colors;

/// Audio visualizer state.
pub struct VisualizerState {
    /// Ring buffer for left channel waveform display.
    pub waveform_left: Vec<f32>,
    /// Ring buffer for right channel waveform display.
    pub waveform_right: Vec<f32>,
    /// Write cursor into the ring buffers.
    pub cursor: usize,
    /// Display width (number of samples shown).
    pub width: usize,
}

impl VisualizerState {
    pub fn new(width: usize) -> Self {
        Self {
            waveform_left: vec![0.0; width],
            waveform_right: vec![0.0; width],
            cursor: 0,
            width,
        }
    }

    /// Push a stereo sample pair into the ring buffer.
    pub fn push(&mut self, left: f32, right: f32) {
        self.waveform_left[self.cursor] = left;
        self.waveform_right[self.cursor] = right;
        self.cursor = (self.cursor + 1) % self.width;
    }

    /// Clear the waveform buffers.
    pub fn clear(&mut self) {
        self.waveform_left.fill(0.0);
        self.waveform_right.fill(0.0);
        self.cursor = 0;
    }
}

/// Draw the output waveform visualizer.
pub fn draw(ui: &mut egui::Ui, state: &VisualizerState) {
    let desired_size = egui::vec2(ui.available_width(), 64.0);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // Dark background
        painter.rect_filled(rect, 2.0, colors::CRUST);

        // Center line
        let center_y = rect.center().y;
        painter.line_segment(
            [
                egui::pos2(rect.left(), center_y),
                egui::pos2(rect.right(), center_y),
            ],
            egui::Stroke::new(0.5, colors::SURFACE1),
        );

        let width = state.width as f32;
        let half_height = rect.height() / 2.0;

        // Draw left channel (teal)
        draw_channel(
            &painter,
            &state.waveform_left,
            state.cursor,
            rect,
            half_height,
            center_y,
            width,
            colors::TEAL.gamma_multiply(0.7),
        );

        // Draw right channel (mauve)
        draw_channel(
            &painter,
            &state.waveform_right,
            state.cursor,
            rect,
            half_height,
            center_y,
            width,
            colors::MAUVE.gamma_multiply(0.7),
        );
    }
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
