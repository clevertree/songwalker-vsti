use nih_plug::prelude::*;

/// Snapshot of the host DAW transport state, updated each process block.
///
/// This is read by the audio thread and shared (as defaults) with `.sw` runner
/// instances for variable injection.
#[derive(Debug, Clone)]
pub struct TransportState {
    /// Beats per minute from the host. Falls back to 120.
    pub bpm: f64,
    /// Time signature numerator (e.g., 4 in 4/4). Falls back to 4.
    pub time_sig_numerator: i32,
    /// Time signature denominator (e.g., 4 in 4/4). Falls back to 4.
    pub time_sig_denominator: i32,
    /// Whether the host transport is currently playing.
    pub playing: bool,
    /// Current position in beats from the start of the song.
    pub position_beats: f64,
    /// Current position in samples from the start of the song.
    pub position_samples: i64,
    /// Host sample rate.
    pub sample_rate: f32,
    /// Whether the host is looping.
    pub looping: bool,
    /// Loop start position in beats (if looping).
    pub loop_start_beats: f64,
    /// Loop end position in beats (if looping).
    pub loop_end_beats: f64,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            time_sig_numerator: 4,
            time_sig_denominator: 4,
            playing: false,
            position_beats: 0.0,
            position_samples: 0,
            sample_rate: 44100.0,
            looping: false,
            loop_start_beats: 0.0,
            loop_end_beats: 0.0,
        }
    }
}

impl TransportState {
    /// Update from the host's Transport struct provided by nih-plug.
    pub fn update(&mut self, transport: &Transport) {
        if let Some(bpm) = transport.tempo {
            self.bpm = bpm;
        }
        if let Some((num, denom)) = transport.time_sig_numerator.zip(transport.time_sig_denominator) {
            self.time_sig_numerator = num;
            self.time_sig_denominator = denom;
        }
        self.playing = transport.playing;
        if let Some(pos) = transport.pos_beats() {
            self.position_beats = pos;
        }
        if let Some(pos) = transport.pos_samples() {
            self.position_samples = pos;
        }
        // nih-plug doesn't expose loop points directly, but some hosts do
        // via CLAP transport extensions. We'll handle that in the future.
    }

    /// Convert a duration in beats to samples at the current BPM and sample rate.
    #[inline]
    pub fn beats_to_samples(&self, beats: f64) -> f64 {
        let seconds_per_beat = 60.0 / self.bpm;
        beats * seconds_per_beat * self.sample_rate as f64
    }

    /// Convert a number of samples to beats at the current BPM and sample rate.
    #[inline]
    pub fn samples_to_beats(&self, samples: f64) -> f64 {
        let seconds_per_beat = 60.0 / self.bpm;
        samples / (seconds_per_beat * self.sample_rate as f64)
    }
}
