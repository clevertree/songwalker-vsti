use nih_plug::prelude::*;

/// Standalone entry point for testing without a host DAW.
fn main() {
    nih_plug::wrapper::standalone::nih_export_standalone::<songwalker_vsti::SongWalkerPlugin>();
}
