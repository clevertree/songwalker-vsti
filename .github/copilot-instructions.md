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

## GitHub Actions

The `Build & Release` workflow (`.github/workflows/release.yml`) triggers on:
- Tag pushes matching `v*` (creates a GitHub Release with cross-platform builds)
- `workflow_dispatch` (manual trigger)

### When to trigger a release
Push a version tag after significant changes (bug fixes, new features, breaking
changes) — **only after all tests pass** across all repos.

```bash
# 1. Bump version in Cargo.toml
# 2. Commit and push the version bump
# 3. Tag and push to trigger the build
git tag v<NEW_VERSION>
git push origin v<NEW_VERSION>
```

Do **not** tag for docs-only, test-only, or refactor-only changes.

### Verifying builds
```bash
gh run list --limit 3                # check recent runs
gh run watch <run-id>                # watch a running build
gh run view <run-id> --log-failed    # inspect failures
```

If a run fails, inspect the logs, fix the issue, and push again.
Iterate until the workflow passes before moving on.
