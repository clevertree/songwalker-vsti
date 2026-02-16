# Copilot Instructions — songwalker-vsti

## Project Overview

`songwalker-vsti` is a VST3 instrument plugin built with `nih-plug`. It exposes SongWalker's synthesis engine as a multi-timbral plugin for use in DAWs.

## Dependency on songwalker-core

Direct Cargo path dependency (no default features):
```toml
songwalker_core = { path = "../songwalker-core", default-features = false }
```

Changes to songwalker-core are picked up automatically on `cargo build` / `cargo test`.

## Key Files

- `src/lib.rs` — plugin lib root
- `src/plugin.rs` — nih-plug plugin implementation
- `src/audio.rs` — audio processing
- `src/midi.rs` — MIDI input handling
- `src/params.rs` — plugin parameters
- `src/state.rs` — plugin state serialization
- `src/transport.rs` — DAW transport sync
- `src/main.rs` — standalone binary entry
- `src/editor/` — egui-based plugin editor UI
- `src/perf/` — performance utilities (pool, SIMD)
- `src/preset/` — preset management
- `src/slots/` — multi-timbral slot handling
- `xtask/` — build/bundle task runner

## Testing

```bash
cargo test
```

Unit tests exist in `src/perf/pool.rs` and `src/perf/simd.rs`.

## Building

```bash
cargo xtask bundle songwalker_vsti --release
```

## Deploying Core Updates

When songwalker-core changes:
```bash
cd /home/ari/dev/songwalker-vsti
cargo test    # recompiles with updated core automatically
```

No additional build steps needed — the path dependency ensures the latest core is used.
