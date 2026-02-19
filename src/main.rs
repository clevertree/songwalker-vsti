use nih_plug::prelude::*;

#[cfg(target_os = "linux")]
mod x11_setup;

/// Standalone entry point for testing without a host DAW.
fn main() {
    // On Linux, spawn a thread to set WM_CLASS and _NET_WM_ICON once the
    // window appears so the desktop environment shows the correct name and icon.
    #[cfg(target_os = "linux")]
    x11_setup::spawn_x11_setup_thread();

    nih_export_standalone::<songwalker_vsti::SongWalkerPlugin>();
}
