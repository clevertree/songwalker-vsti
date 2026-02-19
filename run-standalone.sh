#!/usr/bin/env bash
# Launch the SongWalker VSTi in standalone mode (no DAW required).
set -e
cd "$(dirname "$0")"

# Workaround: if libxcb-dri2-0-dev is not installed, provide a symlink shim.
# VIZIA's GL backend links against -lxcb-dri2 which needs the .so symlink.
SHIM_DIR="target/lib-shims"
if [ ! -f /usr/lib/x86_64-linux-gnu/libxcb-dri2.so ] && [ -f /usr/lib/x86_64-linux-gnu/libxcb-dri2.so.0 ]; then
    mkdir -p "$SHIM_DIR"
    ln -sf /usr/lib/x86_64-linux-gnu/libxcb-dri2.so.0 "$SHIM_DIR/libxcb-dri2.so"
    export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L$(pwd)/$SHIM_DIR"
fi

cargo run --bin songwalker-standalone -- "$@"
