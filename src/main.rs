
/// Standalone entry point — uses custom cpal/midir/eframe backend
/// instead of nih-plug's standalone wrapper.
///
/// This gives us runtime audio device switching, PulseAudio/PipeWire support,
/// and MIDI device selection from the Settings panel.
fn main() {
    // Initialize logger for easier automated testing.
    env_logger::init();

    // Ensure all panics are logged properly before crashing.
    std::panic::set_hook(Box::new(|panic_info| {
        let (filename, line) = panic_info
            .location()
            .map(|loc| (loc.file(), loc.line()))
            .unwrap_or(("<unknown>", 0));
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic_info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<no message>");
        log::error!("CRASH in {}:{}: {}", filename, line, message);
        eprintln!("CRASH in {}:{}: {}", filename, line, message);
    }));

    // Launch the custom standalone app (cpal + midir + eframe)
    songwalker_vsti::standalone::run();
}
