# SongWalker VSTi — Project Plan

## Overview

A cross-platform VST3/CLAP instrument plugin that brings the SongWalker preset library and `.sw` playback engine into any DAW. The plugin serves two roles:

1. **Preset Player** — Browse and load remote presets from the songwalker-library (samplers, synths, composites) and play them via MIDI like any other VSTi.
2. **Song/Track Runner** — Load `.sw` snippets (drum loops, arpeggios, generative patterns) and execute them in sync with the DAW transport, enabling advanced playback techniques without leaving the DAW.

The embedded UI mirrors the SongWalker web editor so the experience is consistent across web and plugin contexts.

---

## Goals

| Goal | Detail |
|------|--------|
| **Max performance** | **Primary goal.** Zero-allocation audio path. Pure Rust DSP with `#[target_feature]` SIMD (SSE2/NEON). Lock-free audio thread. Pre-allocated voice pools. Sample pre-decode to native f32 at host sample rate. Batch voice rendering (process all voices of the same preset type together for cache locality). Profile-guided optimization (`cargo pgo`). |
| **Max compatibility** | VST3 + CLAP formats. Windows, macOS (x86_64 + aarch64), Linux. All major DAWs (Ableton, FL Studio, Bitwig, Reaper, Logic, Cubase, Studio One). |
| **Multi-timbral** | Kontakt-style multi-slot architecture. Multiple presets loaded simultaneously in named slots. Required for combination presets (orchestra, quartet, layered stacks). Each slot has its own MIDI channel or shares the global channel. |
| **UI parity with web** | Same preset browser, same `.sw` code editor, same visualizer — rendered via egui. |
| **Remote preset loading** | Fetch presets from `https://clevertree.github.io/songwalker-library` (or configurable mirror). Cache library indexes and used presets on demand. Optional "Download for Offline" to bulk-cache entire libraries. |
| **Songwalker integration** | Compile and execute `.sw` tracks in real-time. MIDI Note On triggers playback with transposition; Note Off stops it. DAW BPM/transport fills `.sw` variables when not set by the track. |
| **Free & open source** | GPLv3 (or similar copyleft). Donation-based sustainability. No paywalls, no license keys. |

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                     DAW Host (VST3/CLAP)                     │
│                                                              │
│  MIDI In ──►┌───────────────────────────────────────┐        │
│             │           SongWalker VSTi             │──► Audio Out
│  Transport ►│                                       │        │
│             │  ┌─────────────┐   ┌───────────────┐  │        │
│             │  │  MIDI Router │   │   UI (egui)   │  │        │
│             │  │  (by channel)│   │  Slot Rack    │  │        │
│             │  └──────┬──────┘   │  Browser      │  │        │
│             │         │          │  .sw Editors   │  │        │
│             │  ┌──────▼──────┐   │  Visualizer   │  │        │
│             │  │  Slot Mgr   │   └──────┬────────┘  │        │
│             │  │ (Kontakt)   │          │           │        │
│             │  ├─────────────┤   ┌──────┴────────┐  │        │
│             │  │ Slot 1 [P]  │   │ Preset Loader │  │        │
│             │  │ Slot 2 [P]  │   │ (async HTTP + │  │        │
│             │  │ Slot 3 [R]  │   │  disk cache)  │  │        │
│             │  │ ...         │   └───────────────┘  │        │
│             │  ├─────────────┤                      │        │
│             │  │ songwalker- │   [P] = Preset slot  │        │
│             │  │ core (DSP)  │   [R] = Runner slot  │        │
│             │  └─────────────┘                      │        │
│             └───────────────────────────────────────┘        │
└──────────────────────────────────────────────────────────────┘
```

### Crate Dependencies

```
songwalker-vsti/
├── Cargo.toml
├── src/
│   ├── lib.rs              // nih-plug entry, plugin descriptor
│   ├── plugin.rs           // Plugin struct, params, state
│   ├── params.rs           // DAW-exposed parameters (per-slot + global)
│   ├── audio.rs            // process() impl — DSP dispatch, batch rendering
│   ├── midi.rs             // MIDI event handling (channel routing, transpose)
│   ├── transport.rs        // DAW transport sync, variable injection
│   ├── slots/
│   │   ├── mod.rs          // Slot manager (Kontakt-style multi-timbral)
│   │   ├── slot.rs         // Single instrument slot (preset + state)
│   │   ├── preset_slot.rs  // Slot in Preset mode (direct MIDI playback)
│   │   └── runner_slot.rs  // Slot in Runner mode (.sw MIDI-triggered exec)
│   ├── preset/
│   │   ├── loader.rs       // HTTP fetch + JSON parse + sample decode
│   │   ├── cache.rs        // Disk cache (on-demand + offline download)
│   │   └── manager.rs      // In-memory preset registry, hot-swap
│   ├── editor/
│   │   ├── mod.rs          // Editor lifecycle (open/close/resize)
│   │   ├── browser.rs      // Preset browser panel
│   │   ├── slot_rack.rs    // Multi-slot rack view (add/remove/reorder)
│   │   ├── code_editor.rs  // .sw code editing panel (Runner slots)
│   │   └── visualizer.rs   // Waveform / spectrum / meters
│   ├── perf/
│   │   ├── mod.rs          // Performance monitoring
│   │   ├── pool.rs         // Pre-allocated object pools
│   │   └── simd.rs         // SIMD batch processing utilities
│   └── state.rs            // Serialization (slot configs + .sw sources)
└── docs/
    └── plan.md             // This file
```

### Key Crate Choices

| Crate | Role | Rationale |
|-------|------|-----------|
| **nih-plug** | Plugin framework | Best Rust-native VST3 + CLAP support. Self-contained (no C++ SDK dependency). Bundles with `cargo xtask`. Active maintenance. Used by major Rust plugins. |
| **songwalker-core** | Compiler + DSP | Already implements the full pipeline: lexer → parser → compiler → DSP (oscillator, envelope, filter, sampler, mixer, renderer). Dual-target `rlib` works natively. |
| **baseview** + **egui** | GUI | nih-plug's standard GUI path. `nih_plug_egui` crate provides the integration. Cross-platform OpenGL/Metal window embedding. |
| **reqwest** | HTTP client | Async preset/sample fetching. Rustls backend (no OpenSSL dep = easier cross-compile). |
| **tokio** (lightweight) | Async runtime | Background I/O for preset loading. Only `rt`, `net`, `fs` features. Runs on a dedicated non-audio thread. |
| **directories** | Cache paths | Platform-correct cache directory (`~/.cache/songwalker/` on Linux, `~/Library/Caches/` on macOS, `%LOCALAPPDATA%` on Windows). |
| **serde** / **serde_json** | Serialization | Already used by songwalker-core for presets, events, AST. |
| **minimp3** / **hound** | Sample decoding | Already used in songwalker-cli. Decode MP3/WAV sample files fetched from library. |

---

## Plugin Modes

### Mode 1: Preset Player

The default mode. Behaves like a conventional sample-based / synth VSTi.

**Flow:**
1. User opens preset browser in the plugin editor.
2. Browser fetches library index from remote (or cache), displays categories/search.
3. User selects a preset → `PresetDescriptor` JSON is fetched and parsed.
4. Sample zones are fetched (MP3/WAV from GitHub Pages CDN), decoded to PCM, cached to disk.
5. A `PresetNode` DSP graph is instantiated from the descriptor.
6. Incoming MIDI Note On/Off events are routed to the DSP graph's voice allocator.
7. Audio is rendered per-buffer in `process()`.

**DAW Parameters (automatable):**
- Volume, Pan
- Attack, Decay, Sustain, Release (override envelope)
- Filter cutoff, resonance
- Pitch bend range
- Polyphony limit

**State Serialization:**
The DAW project saves: preset identifier (library + path), any parameter overrides, and cache manifest hash. On reload, preset is restored from cache or re-fetched.

### Mode 2: Track Runner

Advanced mode. Loads a `.sw` snippet and executes it in real-time, **triggered and transposed by incoming MIDI**.

**Core Concept: MIDI-Triggered Playback**

The `.sw` track is a self-contained musical phrase (drum loop, arpeggio, riff, etc.). MIDI input controls *when* and *how* it plays:

- **MIDI Note On** → Start playback of the `.sw` track. A transpose node shifts all note events by the interval between the incoming MIDI note and the track's root note (default C4). Multiple simultaneous Note Ons = multiple overlapping instances (polyphonic triggering).
- **MIDI Note Off** → Release the corresponding instance. Active voices enter their release phase; no new events are scheduled from that instance.
- **MIDI CC** → Map to `.sw` track variables (e.g., CC1 → `track.modulation`, CC11 → `track.expression`).
- **MIDI Pitch Bend** → Applied as additional fine transposition on top of the note-based transpose.

**DAW Variable Injection:**

When the `.sw` track does *not* explicitly set certain variables, the plugin fills them from the DAW host:

| `.sw` variable | Source | Fallback |
|----------------|--------|----------|
| `track.beatsPerMinute` | Host transport BPM | 120 |
| `track.timeSignature` | Host time signature | 4/4 |
| `track.sampleRate` | Host sample rate | 44100 |
| `track.key` | MIDI key signature (if available) | C |
| `track.velocity` | Triggering MIDI note velocity (0–1) | 1.0 |

If the `.sw` source explicitly sets `track.beatsPerMinute = 140`, that value is used instead of the host BPM. This lets authors write tempo-locked patterns or tempo-independent ones.

**Flow:**
1. User writes or pastes `.sw` code in the embedded code editor.
2. Code is compiled via `songwalker-core::compile()` → `EventList`.
3. All `PresetRef` events are resolved → presets are fetched/loaded (same as Mode 1).
4. Plugin is idle until MIDI Note On arrives.
5. On MIDI Note On: a new playback instance is spawned with transpose offset applied. Events are scheduled relative to the host transport position.
6. On MIDI Note Off: the corresponding instance is released (voices enter release, scheduling stops).
7. On DAW stop: all instances released, all voices silenced.

**Use Cases:**
- **Drum loops:** Write a drum pattern in `.sw`, trigger it with a single key. Different keys = same pattern at different pitches (useful for tuned percussion).
- **Arpeggios:** `.sw` track defines an arpeggio shape. Press a chord → multiple transposed instances layer together.
- **Riffs & phrases:** A bass riff triggered by a single note, transposed to match the harmony.
- **Layered textures:** Multiple tracks with different presets running simultaneously.
- **Generative sequences:** `.sw` `for` loops + randomization for evolving patterns.

**DAW Integration:**
- Tempo: read from host transport, inject as `track.beatsPerMinute` default.
- Time signature: read from host, inject as `track.timeSignature` default.
- Play/pause/stop: follow host transport state. Pause suspends all active instances.
- Loop: detect host loop points. On loop restart, reset any instances that were started before the loop region.

**State Serialization:**
The DAW project saves: the full `.sw` source text, root note setting, all resolved preset identifiers, parameter overrides. On reload, code is recompiled and presets reloaded from cache.

---

## UI Design

The editor window mirrors the SongWalker web app layout, adapted for plugin embedding.

### Layout

```
┌──────────────────────────────────────────────────────────────┐
│  SongWalker    [+ Add Slot]   [⚙ Settings]   [♡ Donate]    │  ← Header
├────────────────────┬─────────────────────────────────────────┤
│                    │                                         │
│  Preset Browser    │   Slot Rack (Kontakt-style)             │
│  ┌──────────────┐  │   ┌─────────────────────────────────┐   │
│  │ 🔍 Search    │  │   │ Slot 1: FluidR3/Grand Piano     │   │
│  ├──────────────┤  │   │ [Preset ▼] Ch:1  Vol ━━━ Pan ━  │   │
│  │ Libraries    │  │   │ ADSR [==|===|====|==]  Filter ━  │   │
│  │ [✓] FluidR3  │  │   ├─────────────────────────────────┤   │
│  │ [✓] JCLive   │  │   │ Slot 2: JCLive/Strings          │   │
│  │ [ ] Aspirin  │  │   │ [Preset ▼] Ch:2  Vol ━━━ Pan ━  │   │
│  │ [⬇ Offline]  │  │   │ ADSR [==|===|====|==]  Filter ━  │   │
│  ├──────────────┤  │   ├─────────────────────────────────┤   │
│  │ Categories   │  │   │ Slot 3: drumloop.sw  [Runner ▼] │   │
│  │ [Piano]      │  │   │ Root: C4  Ch:10  Vol ━━━        │   │
│  │ [Guitar]     │  │   │ ┌─────────────────────────────┐ │   │
│  │ [Drums]      │  │   │ │ track drums {              │ │   │
│  │ [Synth]      │  │   │ │   kick /4 snare /4         │ │   │
│  │ [Strings]    │  │   │ │   kick /8 kick /8 snare /4 │ │   │
│  ├──────────────┤  │   │ │ }                          │ │   │
│  │ Instruments  │  │   │ └─────────────────────────────┘ │   │
│  │  Piano       │  │   └─────────────────────────────────┘   │
│  │  Strings     │  │                                         │
│  │  Guitar      │  │   ┌─────────────────────────────────┐   │
│  │  Drums       │  │   │  Visualizer                     │   │
│  └──────────────┘  │   │  ▁▂▃▅▇▅▃▂  Peak  [L ████░ R]   │   │
│                    │   └─────────────────────────────────┘   │
├────────────────────┴─────────────────────────────────────────┤
│  Voices: 34/256  │  Slots: 3  │  CPU: 4.1%  │  Cache: 142MB │
└──────────────────────────────────────────────────────────────┘
```

**Slot Rack Behavior:**
- Each slot is independently a Preset slot or a Runner slot.
- Slots can be added, removed, reordered, solo'd, muted.
- Each slot can be assigned a MIDI channel (or "All").
- Runner slots show an inline `.sw` code editor; clicking expands to full editor.
- Drag a preset from the browser onto a slot to load it.
- Combination presets ("Orchestra", "Quartet") create multiple slots automatically.

### UI Technology

**Primary: egui (via `nih_plug_egui`)**
- Immediate-mode Rust GUI. No web dependencies at runtime.
- Custom widgets for: knobs, sliders, peak meters, spectrum bars, keyboard display.
- Syntax highlighting for `.sw` code via a custom egui text editor widget (using the songwalker-core lexer token types for coloring).
- Matches the web editor's dark theme and color palette.

**Rationale for egui over embedded webview:**
- Better DAW compatibility (webview embedding is fragile across hosts).
- Lower latency, no IPC overhead.
- No browser runtime dependency.
- Consistent rendering across platforms.
- The songwalker-core lexer already provides token types usable for syntax coloring.

**Visual Consistency with Web:**
- Same color scheme (dark background, same accent colors for notes, tracks, errors).
- Same preset browser layout (search → library chips → category chips → list).
- Same visualizer style (RMS + peak bars, log-scale FFT spectrum, waveform).
- Code editor with equivalent syntax highlighting colors (notes = cyan, tracks = yellow, keywords = purple, etc.).

---

## Remote Preset Loading

### Architecture

```
                    ┌──────────────────┐
                    │  GitHub Pages    │
                    │  CDN             │
                    │  (or mirror)     │
                    └───────┬──────────┘
                            │ HTTPS
                    ┌───────▼──────────┐
                    │  Async Fetcher   │  ← Background thread (tokio)
                    │  (reqwest)       │
                    └───────┬──────────┘
                            │
                    ┌───────▼──────────┐
                    │  Disk Cache      │  ← Content-addressed
                    │  ~/.cache/       │     (SHA-256 of URL)
                    │  songwalker/     │
                    └───────┬──────────┘
                            │
                    ┌───────▼──────────┐
                    │  Preset Manager  │  ← In-memory registry
                    │  (lock-free read │     Hot-swap on load
                    │   from audio     │
                    │   thread)        │
                    └──────────────────┘
```

### Loading Sequence

1. **Boot:** Plugin loads cached library index (if exists). Displays immediately.
2. **Background refresh:** Fetches fresh `index.json` from remote. Merges updates.
3. **Preset select:** Fetches `preset.json` → parses `PresetDescriptor`.
4. **Sample fetch:** For each `SampleZone` with an `External` `AudioReference`:
   - Check disk cache by content hash or URL hash.
   - If miss: fetch via HTTPS, decode (MP3/WAV → f32 PCM), store in cache.
   - Samples are decoded at the target sample rate (resampled if needed).
5. **Activate:** Preset node graph is built, voice pool allocated. Swap into audio thread via atomic pointer.
6. **Fallback:** If offline and not cached, show error in UI. Previously loaded preset continues playing.

### Cache Strategy

**Principle:** Cache library indexes always. Cache preset data (descriptors + samples) only when used. Provide an explicit "Download for Offline" option per library.

- **Location:** `{platform_cache_dir}/songwalker/`
- **Structure:**
  ```
  songwalker/
  ├── indexes/
  │   ├── root_index.json         // Root library list (always cached, refreshed on boot)
  │   └── {library}/index.json    // Per-library preset listing (cached on first browse)
  ├── presets/
  │   └── {library}/{path}/
  │       ├── preset.json          // Cached preset descriptor (on first use)
  │       └── samples/
  │           ├── {sha256}.pcm     // Decoded f32 PCM (on first use)
  │           └── {sha256}.meta    // Sample rate, channels, original URL, fetch date
  └── offline/
      └── {library}.complete      // Marker file: this library was fully downloaded
  ```
- **On-demand caching:** When a user selects a preset, its descriptor and samples are fetched and cached. Subsequent loads are instant from cache.
- **"Download for Offline" button:** Available per library in the preset browser. Downloads all preset descriptors and samples for that library in the background. Shows progress bar. Creates `offline/{library}.complete` marker.
- **Index refresh:** Library indexes are re-fetched in the background on plugin boot (max once per session). If offline, stale cache is used.
- **Eviction:** LRU by last-access time on individual preset caches. Configurable max cache size (default 2 GB). Libraries marked as "offline" are exempt from eviction.
- **Integrity:** SHA-256 verification on cached files. Re-fetch on mismatch.

### URL Configuration

Default base URL: `https://clevertree.github.io/songwalker-library`

User-configurable in plugin settings to support:
- Self-hosted mirrors
- Local file server (`file:///` or `http://localhost:...`)
- Private preset libraries

---

## Audio Thread Design

### Constraints

The `process()` callback runs on the DAW's real-time audio thread. Strict rules:

- **No allocations** in the audio path.
- **No blocking** (no locks, no I/O, no HTTP).
- **No syscalls** beyond reading atomic values.

### Implementation

```rust
fn process(&mut self, buffer: &mut Buffer, context: &mut impl ProcessContext) {
    // 1. Read transport state (atomics from UI/background thread)
    // 2. Check for hot-swapped presets (atomic pointer compare-exchange)
    // 3. Process MIDI events from host:
    //    - Route by channel to target slot(s)
    //    - Preset slots: Note On/Off → voice allocator
    //    - Runner slots: Note On → spawn RunnerInstance (transposed)
    //                    Note Off → release RunnerInstance
    //    - Inject host BPM / time sig / velocity into runner variables
    // 4. For each slot, batch-render active voices:
    //    - Group voices by preset type for cache locality
    //    - SIMD-accelerated sample interpolation + envelope
    //    - Apply per-slot volume/pan
    // 5. Mix all slot outputs into final buffer
    // 6. Feed samples to visualizer (lock-free ring buffer)
}
```

**Voice Management:**
- Pre-allocated voice pool per slot (configurable, default 64 voices per slot, 256 global max).
- Round-robin voice stealing when pool exhausted.
- Each voice owns its DSP graph instance (oscillator/sampler + envelope + filter).
- Voices for the same preset type are rendered in batch (contiguous memory, cache-friendly).

**Preset Hot-Swap:**
- New preset is fully loaded on background thread.
- Swapped into audio thread via `Arc<AtomicPtr<PresetInstance>>`.
- Old preset is dropped on background thread (deferred via channel).

**Event Scheduling (Runner Mode — MIDI-Triggered):**
- `EventList` from compiler is sorted by beat position.
- Each MIDI Note On spawns a new `RunnerInstance` with:
  - A cursor into the `EventList` (starts at beat 0).
  - A transpose offset = `(midi_note - root_note)` semitones.
  - Injected variables from host transport (BPM, time sig, velocity).
- On each `process()` call, advance all active instance cursors by elapsed beats.
- Fire events whose beat position falls within the current buffer window, transposed.
- On MIDI Note Off: mark instance as releasing. No new events scheduled; active voices enter release phase.
- When all voices in a released instance finish their release, the instance is returned to the pool.

**Multi-Slot Rendering:**
- Each slot is processed independently (own voice pool, own preset/runner state).
- Slots are mixed into the output buffer in order.
- Per-slot volume/pan applied before mix.
- Future: per-slot output routing to DAW aux buses (VST3 multi-out).

---

## Performance Strategy

Performance is the **primary design goal**. Every architectural decision prioritizes real-time audio safety and throughput.

### Zero-Allocation Audio Path

- All voice pools, mix buffers, and scratch buffers are pre-allocated at `initialize()` time.
- No `Vec` resizes, no `Box::new()`, no `String` formatting in `process()`.
- Runner instances are drawn from a pre-allocated pool (fixed max, configurable).
- Preset hot-swap uses `AtomicPtr` — the only "allocation" visible to the audio thread is a pointer read.

### SIMD Acceleration

| Operation | SIMD Strategy |
|-----------|---------------|
| Sample interpolation (linear/cubic) | Process 4 samples per iteration (SSE2 `_mm_mul_ps` / NEON `vmulq_f32`). |
| Envelope application | Multiply gain ramp across buffer in 4-wide chunks. |
| Slot mixing | Add slot buffers into output using SIMD add. |
| Filter (biquad) | Vectorize across voices (same filter coefficients). |
| Stereo pan | Constant-power pan via SIMD sin/cos approximation. |

Use `std::arch` intrinsics with runtime feature detection (`is_x86_feature_detected!("sse2")`, `is_aarch64_feature_detected!("neon")`). Scalar fallback always available.

### Batch Voice Rendering

Instead of rendering voices one-by-one (poor cache behavior), batch by preset type:

```
Traditional:  Voice1.render() → Voice2.render() → Voice3.render()
              (each touches different sample data = cache thrashing)

Batched:      Group voices using PresetA → render all together
              Group voices using PresetB → render all together
              (same sample data stays hot in L2/L3 cache)
```

This is especially important for sampler presets where the sample data is large.

### Memory Layout

- Sample data stored as contiguous `f32` slices (not `Vec<Vec<f32>>`).
- Voice state stored in struct-of-arrays (SoA) where beneficial for SIMD:
  ```rust
  struct VoicePool {
      phases: [f32; MAX_VOICES],      // all phases together
      gains: [f32; MAX_VOICES],       // all gains together
      pitches: [f32; MAX_VOICES],     // all pitches together
      active: [bool; MAX_VOICES],     // active flags together
      // ... vs per-voice structs that scatter data across memory
  }
  ```

### Sample Decoding Pipeline

Samples are decoded and resampled on background threads, **never** on the audio thread:

1. Fetch MP3/WAV from network or cache (background thread).
2. Decode to f32 PCM (background thread).
3. Resample to host sample rate if needed (background thread, using sinc interpolation).
4. Store decoded PCM in `Arc<[f32]>` (immutable, shared safely with audio thread).
5. Swap into preset via atomic pointer.

### CPU Budget Monitoring

- Per-slot CPU measurement (compare `process()` elapsed time against buffer deadline).
- Expose as DAW parameter + UI display.
- Auto-polyphony reduction: if CPU consistently > 80%, reduce max voices per slot.
- Voice priority: newer voices and higher-velocity voices survive stealing.

### Profile-Guided Optimization (PGO)

Release builds use a two-pass PGO workflow:
1. Build instrumented binary.
2. Run benchmark suite (render various preset types + runner patterns).
3. Rebuild with profile data → 10–20% improvement on hot paths.

Integrated into CI via `cargo pgo`.

---

## Integration with songwalker-core

### What's Reused Directly

| Component | Notes |
|-----------|-------|
| Lexer + Parser + AST | Full `.sw` compilation pipeline |
| Compiler | `compile()` → `EventList` for Runner Mode |
| `PresetDescriptor` / `PresetNode` types | Deserialization of remote preset JSON |
| DSP engine (`dsp/` module) | Oscillator, Envelope, Filter, Sampler, Mixer, Composite, Voice |
| Tuning system | `TuningInfo`, pitch detection results |
| Error types | `ariadne`-based diagnostics for editor error display |

### What's New in the VSTi

| Component | Notes |
|-----------|-------|
| MIDI input handling | Route by channel to slots. Runner: Note On/Off triggers transposed playback. Preset: standard voice allocation. |
| DAW transport sync | Read host BPM, time sig, play state, loop points. Inject as `.sw` variable defaults. |
| Multi-slot manager | Kontakt-style slot rack. Add/remove/reorder preset and runner slots. Per-slot MIDI channel, volume, pan. |
| Voice allocator | Per-slot polyphonic voice pools with stealing. Batch rendering for cache locality. |
| Real-time scheduler | Per-runner-instance cursors through `EventList`, spawned/released by MIDI. |
| HTTP preset fetcher | Async remote loading (core has no networking). On-demand + "Download for Offline". |
| Disk cache | Cache indexes always, presets on use. LRU eviction. Offline library download. |
| Plugin state save/restore | DAW project serialization (all slot configs, .sw sources, parameter overrides). |
| SIMD + perf utilities | Batch voice processing, SIMD sample interpolation, pre-allocated pools. |
| egui editor | Full GUI: slot rack, preset browser, inline .sw editors, visualizer. |

### Required Core Changes

Minimal. The core is already well-structured for embedding. Potential changes:

1. **Feature-gate WASM exports** — `#[cfg(target_arch = "wasm32")]` on wasm-bindgen items so they don't compile for native VSTi builds. (May already be gated.)
2. **Expose incremental rendering API** — Current `render_song_samples()` renders the entire song at once. The VSTi needs a `render_block(events, num_samples)` style API for real-time buffer-by-buffer rendering. The DSP primitives already support this; it's a matter of exposing a suitable entry point.
3. **Sample rate propagation** — Ensure all DSP nodes accept runtime sample rate (already parameterized in renderer, verify for individual nodes).
4. **Voice pool integration** — The existing `Voice` type may need adaptation for a shared pool with steal/release semantics.

---

## Build & Distribution

### Build System

```bash
# Build VST3 + CLAP for current platform
cargo xtask bundle songwalker-vsti --release

# Cross-compile (via cross or cargo-zigbuild)
cargo xtask bundle songwalker-vsti --release --target x86_64-pc-windows-gnu
cargo xtask bundle songwalker-vsti --release --target aarch64-apple-darwin
```

nih-plug's `cargo xtask bundle` produces correctly structured plugin bundles:
- **VST3:** `songwalker.vst3/` bundle directory
- **CLAP:** `songwalker.clap` single file

### CI/CD (GitHub Actions)

| Step | Detail |
|------|--------|
| Build matrix | `{linux-x86_64, macos-x86_64, macos-aarch64, windows-x86_64}` × `{vst3, clap}` |
| Test | `cargo test` (unit + integration, including compile → render round-trips) |
| Bundle | `cargo xtask bundle --release` per target |
| Sign | macOS: codesign + notarize. Windows: signtool (if cert available). |
| Package | ZIP per platform with install instructions |
| Release | GitHub Releases with per-platform assets |

### Installer

- **macOS:** `.pkg` installer (copies to `~/Library/Audio/Plug-Ins/VST3/` and `~/Library/Audio/Plug-Ins/CLAP/`)
- **Windows:** Simple ZIP with instructions (copy to `C:\Program Files\Common Files\VST3\` / CLAP equivalent). Optional NSIS installer later.
- **Linux:** ZIP with instructions (copy to `~/.vst3/` / `~/.clap/`). Optional `.deb`/`.rpm` later.

---

## Development Phases

### Phase 1 — Scaffold & Audio (Weeks 1–2)

- [ ] Initialize `songwalker-vsti` crate with nih-plug boilerplate
- [ ] Add `songwalker-core` as path dependency
- [ ] Implement basic `process()` — sine wave output to verify DAW hosting works
- [ ] MIDI input handling — Note On/Off → simple oscillator voices (single slot)
- [ ] DAW transport reading (BPM, play state, time signature)
- [ ] Pre-allocated voice pool with zero-alloc `process()` path
- [ ] Verify builds & loads in: Reaper, Bitwig, Ableton (VST3 + CLAP)
- [ ] Basic performance benchmark (voice count at <1% CPU per voice)

### Phase 2 — Preset Loading & Caching (Weeks 3–4)

- [ ] Implement `PresetLoader` — async HTTP fetch of index + preset JSON
- [ ] On-demand disk cache (indexes always, presets on use)
- [ ] Decode MP3/WAV samples to f32 PCM at host sample rate
- [ ] Build `PresetNode` DSP graph from `PresetDescriptor`
- [ ] Voice allocator with polyphony management and voice stealing
- [ ] Atomic preset hot-swap (background → audio thread)
- [ ] End-to-end: select preset → MIDI plays correct samples
- [ ] SIMD-accelerated sample interpolation (SSE2 / NEON)

### Phase 3 — Multi-Slot & Editor UI (Weeks 5–7)

- [ ] Implement slot manager (add/remove/reorder slots)
- [ ] Per-slot MIDI channel routing
- [ ] Per-slot volume, pan, mute, solo
- [ ] Set up `nih_plug_egui` editor scaffold
- [ ] Slot rack UI (Kontakt-style collapsible slot strips)
- [ ] Preset browser panel (search, library chips, category filter, drag-to-slot)
- [ ] "Download for Offline" button per library with progress bar
- [ ] ADSR and filter knob controls per slot
- [ ] Peak meter + spectrum visualizer
- [ ] Status bar (voice count, slot count, CPU, cache size)
- [ ] Dark theme matching web editor colors

### Phase 4 — Runner Mode (Weeks 8–10)

- [ ] Integrate `songwalker-core` compiler in plugin
- [ ] MIDI Note On → spawn transposed `RunnerInstance`
- [ ] MIDI Note Off → release `RunnerInstance` (voices enter release)
- [ ] DAW variable injection (BPM, time sig, velocity → `.sw` defaults)
- [ ] `.sw` code editor widget with syntax highlighting (using core lexer tokens)
- [ ] Error display with source location markers
- [ ] Note highlighting during playback
- [ ] Polyphonic triggering (multiple simultaneous MIDI notes = multiple instances)
- [ ] DAW loop point detection → instance reset
- [ ] Root note selector per runner slot

### Phase 5 — Polish & Release (Weeks 11–13)

- [ ] Plugin state save/restore (all slots, .sw sources, parameters)
- [ ] Preset parameter automation (expose per-slot params to DAW)
- [ ] Combination presets ("Orchestra" auto-creates multiple slots)
- [ ] Cache management UI (per-library size, clear, offline status)
- [ ] Keyboard display widget (shows active notes across all slots)
- [ ] Performance profiling: batch rendering, memory layout optimization
- [ ] Profile-guided optimization (`cargo pgo`) for release builds
- [ ] CI/CD pipeline for all platforms
- [ ] User documentation + donation links (GitHub Sponsors, Ko-fi)
- [ ] Beta testing across DAWs

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| egui rendering issues in some DAWs | UI glitches or crashes | Test early in Phase 1. Fallback: generic nih-plug parameter UI. |
| Large preset download sizes | Slow first use | On-demand caching means only used presets are fetched. Progressive index loading. "Download for Offline" is opt-in. |
| Audio glitches during preset swap | Audible artifacts | Crossfade between old and new preset (50ms). Voice release before swap. |
| HTTPS blocked in some DAW sandboxes | Can't fetch presets | Offline mode from cache. "Download for Offline" when connected. Manual import of preset packs. Local file server option. |
| Multi-slot voice count explosion | CPU overload | Per-slot and global voice limits. Aggressive voice stealing. CPU meter in UI. Auto-reduce polyphony under load. |
| MIDI-triggered runner timing drift | Audible timing issues | Beat-quantized instance start (snap to nearest subdivison). Host transport as authoritative clock. |
| songwalker-core API changes | Build breakage | Pin to git tag/rev. Integration tests in CI. |
| Cross-platform build complexity | Missing targets | Use `cross` for Linux→Windows. GitHub Actions matrix. Test on real machines. |
| Multi-timbral parameter explosion | DAW confusion | Expose only active slot parameters. Dynamic parameter list (nih-plug supports this). |

---

## Decisions (Resolved)

| # | Question | Decision |
|---|----------|----------|
| 1 | GUI framework | **egui** via `nih_plug_egui`. Pragmatic, performant, no runtime deps. |
| 2 | Preset caching | **On-demand.** Cache library indexes + used presets. "Download for Offline" button per library for bulk caching. |
| 3 | Runner Mode MIDI | **MIDI Note On triggers playback with transposition** (interval from root note). **Note Off releases.** DAW BPM/transport injected as `.sw` variable defaults. |
| 4 | Multi-timbral | **Yes, Kontakt-style.** Multiple named preset/runner slots. Required for combination presets (orchestra, quartet, layered instruments). |
| 5 | Licensing | **Free & open source.** GPLv3 (or similar). Donation-based (GitHub Sponsors, Ko-fi, etc.). No paywalls. |
| 6 | Plugin name | **SongWalker** |
