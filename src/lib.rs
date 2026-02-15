//! SongWalker VSTi — Multi-timbral instrument plugin.
//!
//! A Kontakt-style multi-slot instrument that loads presets from the
//! songwalker-library and can execute `.sw` track snippets triggered by MIDI.

use nih_plug::prelude::*;

pub mod audio;
pub mod editor;
pub mod midi;
pub mod params;
pub mod perf;
pub mod plugin;
pub mod preset;
pub mod slots;
pub mod state;
pub mod transport;

pub use plugin::SongWalkerPlugin;

nih_export_clap!(SongWalkerPlugin);
nih_export_vst3!(SongWalkerPlugin);
