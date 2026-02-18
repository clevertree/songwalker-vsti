#!/usr/bin/env bash
# Launch the SongWalker VSTi in standalone mode (no DAW required).
set -e
cd "$(dirname "$0")"
cargo run --bin songwalker-standalone -- "$@"
