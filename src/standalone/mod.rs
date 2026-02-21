//! Custom standalone application — replaces nih-plug's standalone wrapper.
//!
//! Uses cpal for audio output, midir for MIDI input, and eframe for the GUI.
//! This gives us runtime device switching, PulseAudio/PipeWire support,
//! and full control over the audio pipeline.

pub mod app;
pub mod audio_backend;
pub mod midi_backend;
pub mod params;

pub use app::run;
