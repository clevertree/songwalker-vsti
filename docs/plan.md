# SongWalker VSTi — Project Plan

## Overview

A cross-platform VST3/CLAP instrument plugin that brings the SongWalker preset library and `.sw` playback engine into any DAW. Every slot in the plugin is backed by a `.sw` source file that **re-executes on every MIDI note** — like a React component re-rendering. A simple preset is just a 2-line `.sw` program (`loadPreset` + play at `midi.note`). Advanced use cases — arpeggiation, harmonization, drum patterns, generative music — are achieved by editing that same `.sw` source to add `state`, `sequence`, control flow, and multiple presets.

The embedded UI reuses the **songwalker-js** editor and preset browser (same Monaco-based code editor, same preset loader, same visualizer) so the experience is identical across web and plugin contexts.

---

## Goals

| Goal | Detail |
|------|--------|
| **Max performance** | **Primary goal.** Zero-allocation audio path. Pure Rust DSP with `#[target_feature]` SIMD (SSE2/NEON). Lock-free audio thread. Pre-allocated voice pools. Sample pre-decode to native f32 at host sample rate. Batch voice rendering (process all voices of the same preset type together for cache locality). Profile-guided optimization (`cargo pgo`). |
| **Max compatibility** | VST3 + CLAP formats. Windows, macOS (x86_64 + aarch64), Linux. All major DAWs (Ableton, FL Studio, Bitwig, Reaper, Logic, Cubase, Studio One). |
| **Multi-timbral** | Kontakt-style multi-slot architecture. Multiple presets loaded simultaneously in named slots. Required for combination presets (orchestra, quartet, layered stacks). Each slot has its own MIDI channel or shares the global channel. |
| **Unified .sw slots** | Every slot is a `.sw` program that re-executes per MIDI note. "Load preset and play" is a 2-line default `.sw`. Users edit to add arpeggiation, harmonization, sequencing — no mode switches. Percussion presets play by name (`kick /4`). Melodic presets take pitch in parens (`piano(midi.note)`). |
| **UI parity with web** | Reuse **songwalker-js** components: same Monaco-based code editor, same preset browser/loader (PresetBrowser, PresetLoader), same visualizer. egui hosts an embedded webview or native port of these components. |
| **Remote preset loading** | Fetch presets from `https://clevertree.github.io/songwalker-library` (or configurable mirror). Cache library indexes and used presets on demand. Optional "Download for Offline" to bulk-cache entire libraries. |
| **Songwalker integration** | Compile and execute `.sw` programs reactively. Every slot compiles its `.sw` source via songwalker-core. Each MIDI note triggers full re-execution. `state` variables persist across firings. `sequence` blocks continue from saved cursor. `midi.*` / `transport.*` injected from host. |
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
│             │  │  MIDI Router │   │  UI (egui +   │  │        │
│             │  │  (by channel)│   │  webview)     │  │        │
│             │  └──────┬──────┘   │  Slot Rack    │  │        │
│             │         │          │  .sw Editor   │  │        │
│             │  ┌──────▼──────┐   │  Preset       │  │        │
│             │  │  Slot Mgr   │   │   Browser     │  │        │
│             │  │ (Kontakt)   │   │  Visualizer   │  │        │
│             │  ├─────────────┤   └──────┬────────┘  │        │
│             │  │ Slot 1 .sw  │          │           │        │
│             │  │ Slot 2 .sw  │   ┌──────┴────────┐  │        │
│             │  │ Slot 3 .sw  │   │ Preset Loader │  │        │
│             │  │ ...         │   │ (songwalker-  │  │        │
│             │  ├─────────────┤   │  js / HTTP +  │  │        │
│             │  │ songwalker- │   │  disk cache)  │  │        │
│             │  │ core (DSP)  │   └───────────────┘  │        │
│             │  └─────────────┘                      │        │
│             │                                       │        │
│             │  Every slot = .sw source + compiled    │        │
│             │  program + state vars + sequence cursors │        │
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
│   ├── midi.rs             // MIDI event handling (channel routing, event injection)
│   ├── transport.rs        // DAW transport sync, variable injection
│   ├── slots/
│   │   ├── mod.rs          // Slot manager (Kontakt-style multi-timbral)
│   │   ├── slot.rs         // Single .sw slot (source + compiled state + voices)
│   │   └── context.rs      // Per-note execution context (midi.* variables)
│   ├── preset/
│   │   ├── loader.rs       // HTTP fetch + JSON parse + sample decode
│   │   ├── cache.rs        // Disk cache (on-demand + offline download)
│   │   └── manager.rs      // In-memory preset registry, hot-swap
│   ├── editor/
│   │   ├── mod.rs          // Editor lifecycle (open/close/resize)
│   │   ├── browser.rs      // egui chrome + webview bridge for preset browser
│   │   ├── slot_rack.rs    // Multi-slot rack view (add/remove/reorder)
│   │   ├── code_editor.rs  // .sw code editor (webview: Monaco, fallback: egui)
│   │   └── visualizer.rs   // Waveform / spectrum / meters
│   ├── perf/
│   │   ├── mod.rs          // Performance monitoring
│   │   ├── pool.rs         // Pre-allocated object pools
│   │   └── simd.rs         // SIMD batch processing utilities
│   └── state.rs            // Serialization (slot .sw sources + params)
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

## Reactive .sw Execution Model

Every slot in the plugin is backed by a `.sw` source file. The **entire file re-executes
on every MIDI note** — like a React component re-rendering on each state change. There is
no `on noteOn` wrapper; the file body IS the note handler.

### How It Works

1. User selects a preset from the browser → the plugin generates a **default `.sw`**.
2. The `.sw` source is visible and editable in the code editor at all times.
3. Each MIDI Note On triggers a **full top-to-bottom execution** of the `.sw` file.
4. Multiple held notes = multiple parallel executions (polyphonic).
5. When `midi.gate` goes false (Note Off), `while (midi.gate)` loops exit and voices release.
6. `state` variables persist across note firings. Everything else re-initializes.
7. `sequence` blocks continue from their saved cursor across note firings.
8. `on cc(n)` / `on pitchBend` auxiliary handlers react to non-note MIDI events.

This means a "simple preset player" and a "complex MIDI-reactive sequencer" are the
same system — the only difference is how much `.sw` code the user writes.

### Default .sw Generation

When a user selects a preset, the plugin auto-generates:

```sw
const piano = loadPreset("FluidR3_GM/Acoustic Grand Piano")
piano(midi.note) *midi.velocity
```

That's it — a 2-line program. Each MIDI note fires the file, which plays the preset
at the incoming pitch and velocity. The user can immediately modify it.

---

## Preset-as-Note Syntax

Preset variables can be used directly in note position. Percussion presets (which have a
fixed default root pitch) need no pitch argument. Melodic presets take a pitch in parentheses.

### Percussion (no pitch needed)

Presets carry a root pitch from their sample definition. When a preset identifier appears
in note position without parentheses, it plays at its root pitch — just like writing a
drum hit in a song.

```sw
const kick  = loadPreset("FluidR3_GM/Standard Kit/Kick")
const hat   = loadPreset("FluidR3_GM/Standard Kit/Closed Hi-Hat")
const snare = loadPreset("FluidR3_GM/Standard Kit/Snare")

kick /4              // play kick at default pitch, quarter note step
kick *0.5 /4         // half velocity
hat /8               // hi-hat, eighth note
snare *0.9 /4        // snare, 90% velocity
```

### Melodic (pitch in parentheses)

For pitched instruments, pass the target pitch in parentheses:

```sw
const piano = loadPreset("FluidR3_GM/Acoustic Grand Piano")

piano(C4) /4                      // play piano at C4
piano(midi.note) *midi.velocity   // play at incoming MIDI pitch
piano(midi.note + 12) *0.8        // one octave up
```

This is syntactically a function call — `piano(C4)` calls the preset with a pitch argument.
The compiler sees that `piano` resolves to a preset (not a function like `loadPreset`) and
interprets the call as "play this preset at this pitch."

### Existing note syntax unchanged

The traditional `track.instrument` + bare note syntax still works:

```sw
track.instrument = piano
C4 /4                    // uses track.instrument
[C4, E4, G4] /4         // chord
. /4                     // rest
```

### Grammar Summary

```
<preset_id> [(<pitch_expr>)] [*<velocity>] [@<audible_dur>] [/<step_dur>]
```

| Form | Meaning |
|------|---------|
| `kick /4` | Play preset at it default root pitch |
| `kick *0.5 /4` | Same, half velocity |
| `piano(C4) /4` | Play preset at C4 |
| `piano(midi.note) *midi.velocity` | Play at MIDI input pitch and velocity |
| `piano(midi.note + 7) *0.8 /8` | Play a fifth up, 80% velocity, eighth step |
| `C4 /4` | Existing syntax — uses `track.instrument` |

---

## MIDI-Reactive .sw Syntax

### MIDI Input Variables

When running inside the VSTi, the following read-only variables are injected by the host
and updated per MIDI note event:

| Variable | Type | Description |
|----------|------|-------------|
| `midi.note` | `int` | MIDI note number (0–127) of the triggering event |
| `midi.velocity` | `float` | Velocity normalized to 0.0–1.0 |
| `midi.frequency` | `float` | Frequency in Hz (derived from `midi.note` + tuning) |
| `midi.channel` | `int` | MIDI channel (1–16) |
| `midi.gate` | `bool` | `true` while the triggering note is held, `false` after note-off |
| `midi.pitchBend` | `float` | Pitch bend value (−1.0 to +1.0) |
| `midi.cc[n]` | `float` | Value of MIDI CC number `n` (0.0–1.0) |

### DAW Transport Variables

Injected from the host transport. The `.sw` can override any of these explicitly.

| Variable | Type | Source | Default |
|----------|------|--------|---------|
| `transport.bpm` | `float` | Host tempo | 120 |
| `transport.timeSigNum` | `int` | Host time signature numerator | 4 |
| `transport.timeSigDen` | `int` | Host time signature denominator | 4 |
| `transport.sampleRate` | `int` | Host sample rate | 44100 |
| `transport.playing` | `bool` | Host play state | false |
| `transport.beat` | `float` | Current beat position | 0.0 |

### State Variables

Variables declared with `state` persist across note firings — like React's `useState`.
Regular `let` and `const` are re-initialized on each firing.

```sw
state let step = 0           // initialized once, persists across firings
const pattern = [0, 4, 7]   // re-initialized each firing (same value, no cost)

piano(midi.note + pattern[step % pattern.length]) *midi.velocity
step += 1
```

### Sequence Blocks

A `sequence` block remembers its cursor position across note firings. The pattern plays
in real-time with actual durations, continuing from where it left off.

```sw
const kick  = loadPreset("FluidR3_GM/Standard Kit/Kick")
const hat   = loadPreset("FluidR3_GM/Standard Kit/Closed Hi-Hat")
const snare = loadPreset("FluidR3_GM/Standard Kit/Snare")

sequence {
    while (midi.gate) {
        kick /4
        hat /8
        hat /8
        snare /4
        hat /8
        hat /8
    }
}
```

- **First note press:** starts the pattern from the top, plays in real-time.
- **Note release:** `midi.gate` → false, loop exits, cursor position saves.
- **Next note press:** resumes from saved cursor, loop restarts with `midi.gate` = true.

Without `while`, the sequence advances one event per note firing (step sequencer mode):

```sw
sequence {
    kick         // firing 1
    hat          // firing 2
    hat          // firing 3
    snare        // firing 4
    hat          // firing 5
    hat          // firing 6 → wraps back to start
}
```

### Auxiliary Event Handlers

The file body handles Note On implicitly. For other MIDI events, use `on` handlers:

```sw
on cc(1) {
    // Executes when MIDI CC #1 (mod wheel) changes.
    synth.filterCutoff = midi.cc[1] * 8000
}

on pitchBend {
    // Executes when pitch bend changes.
}

on noteOff {
    // Explicit cleanup (usually not needed — voices release automatically
    // when midi.gate goes false and while loops exit).
}
```

These run alongside the main body, not instead of it.

### Polyphonic Execution

Each MIDI note triggers an independent execution of the entire `.sw` file. If the user
holds three notes simultaneously, three execution contexts run in parallel, each with
their own `midi.note`, `midi.velocity`, and `midi.gate` values.

`state` variables and `sequence` cursors are **shared** across all contexts — they are
slot-level persistent state, not per-note state. This is how an arpeggiator can advance
a shared step counter regardless of which note triggers it.

### Control Flow (JavaScript-style)

```sw
// If-else
if (midi.velocity > 0.8) {
    piano(midi.note) *1.0
} else if (midi.velocity > 0.4) {
    piano(midi.note) *midi.velocity
} else {
    piano(midi.note) *(midi.velocity * 0.5)
}

// While loop (runs in real-time as long as condition is true)
while (midi.gate) {
    piano(midi.note) *midi.velocity /16
}

// For loop (existing syntax, unchanged)
for i in 1..4 {
    piano(midi.note + i * 12) *midi.velocity /8
}
```

### Expressions & Operators

Standard JavaScript arithmetic and logical operators:

| Category | Operators |
|----------|-----------|
| Arithmetic | `+`, `-`, `*`, `/`, `%` |
| Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` |
| Logical | `&&`, `\|\|`, `!` |
| Ternary | `condition ? valueA : valueB` |
| Assignment | `=`, `+=`, `-=`, `*=`, `/=` |

### Built-in Functions

| Function | Description |
|----------|-------------|
| `noteToFreq(note)` | Convert MIDI note number to frequency in Hz |
| `freqToNote(freq)` | Convert frequency to nearest MIDI note number |
| `interval(semitones)` | Returns `midi.note + semitones` (convenience) |
| `scaleNote(root, scale, degree)` | Returns the MIDI note at `degree` steps above `root` in `scale` |
| `random(min, max)` | Random float between `min` and `max` |
| `randomInt(min, max)` | Random integer between `min` and `max` (inclusive) |

### Array Literals

```sw
const pattern = [0, 4, 7, 12]            // semitone offsets
const velocities = [1.0, 0.6, 0.8, 0.6]  // accent pattern
const scale = [0, 2, 4, 5, 7, 9, 11]     // major scale intervals
```

---

## .sw Examples

### Example 1: Default Preset (auto-generated)

The simplest `.sw` — a 2-line program auto-generated when the user picks a preset.
Each MIDI note fires the entire file, which plays the preset at the input pitch.

```sw
const piano = loadPreset("FluidR3_GM/Acoustic Grand Piano")
piano(midi.note) *midi.velocity
```

### Example 2: Arpeggiator

Each note press plays the next note in the pattern. `state` persists the step counter.

```sw
const synth = loadPreset("FluidR3_GM/Synth Strings 1")
const pattern = [0, 4, 7, 12, 7, 4]  // up-down triad + octave
state let step = 0

synth(midi.note + pattern[step % pattern.length]) *midi.velocity
step += 1
```

### Example 3: Drum Kit (percussion preset-as-note)

Percussion presets played by name — no pitch mapping needed. The `sequence` block
continues from its saved cursor across note firings.

```sw
const kick  = loadPreset("FluidR3_GM/Standard Kit/Kick")
const hat   = loadPreset("FluidR3_GM/Standard Kit/Closed Hi-Hat")
const snare = loadPreset("FluidR3_GM/Standard Kit/Snare")

sequence {
    while (midi.gate) {
        kick /4
        hat /8
        hat /8
        snare *0.9 /4
        hat /8
        hat /8
    }
}
```

### Example 4: Harmonic Accompaniment

Play the input note plus a diatonic third. The entire file runs per note — both the
original and the harmony play simultaneously.

```sw
const piano = loadPreset("FluidR3_GM/Acoustic Grand Piano")
const root = C4
const scale = [0, 2, 4, 5, 7, 9, 11]  // major scale

// Play original note
piano(midi.note) *midi.velocity

// Calculate and play diatonic third
const degree = (midi.note - root) % 12
const idx = scale.indexOf(degree)
if (idx >= 0) {
    const thirdNote = midi.note + scale[(idx + 2) % scale.length] - degree
    piano(thirdNote) *(midi.velocity * 0.7)
}
```

### Example 5: Velocity-Layered Drum Pattern

Different instruments triggered based on velocity zones with a continuing sequence.

```sw
const kick  = loadPreset("FluidR3_GM/Standard Kit/Kick")
const snare = loadPreset("FluidR3_GM/Standard Kit/Snare")
const hat   = loadPreset("FluidR3_GM/Standard Kit/Closed Hi-Hat")

sequence {
    while (midi.gate) {
        if (midi.velocity > 0.5) {
            kick *midi.velocity /4
            hat *0.3 /8
            hat *0.3 /8
            snare *midi.velocity /4
            hat *0.3 /8
            hat *0.3 /8
        } else {
            hat *midi.velocity /8
            hat *(midi.velocity * 0.5) /8
        }
    }
}
```

### Example 6: Generative Texture

Randomized ambient notes that evolve while the key is held.

```sw
const pad = loadPreset("FluidR3_GM/Pad 2 (warm)")
const scale = [0, 2, 4, 7, 9]  // pentatonic

while (midi.gate) {
    const degree = randomInt(0, scale.length - 1)
    const octave = randomInt(-1, 1) * 12
    const note = midi.note + scale[degree] + octave
    const vel = random(0.2, midi.velocity)
    pad(note) *vel /8
}
```

### Example 7: CC Filter Control

Map mod wheel (CC1) to filter cutoff using an auxiliary handler.

```sw
const synth = loadPreset("FluidR3_GM/Pad 1 (new age)")

// Main body: plays on each MIDI note
synth(midi.note) *midi.velocity

// Auxiliary: reacts to CC changes (not note-triggered)
on cc(1) {
    synth.filterCutoff = midi.cc[1] * 8000 + 200  // 200–8200 Hz
}
```

### Example 8: Step Sequencer

Each note press fires the next step. No `while` loop, no real-time playback —
the `sequence` advances one event per note firing.

```sw
const synth = loadPreset("FluidR3_GM/Lead 1 (square)")
const notes = [C4, E4, G4, B4, C5, B4, G4, E4]
state let step = 0

synth(notes[step % notes.length]) *midi.velocity
step += 1
```

**State Serialization:**
The DAW project saves: the full `.sw` source text, all resolved preset identifiers,
parameter overrides, `state` variable values, `sequence` cursor positions. On reload,
code is recompiled and presets reloaded from cache.

---

## UI Design

The editor window reuses the **songwalker-js** web components in an embedded webview,
providing identical UX to the SongWalker and SNESology web editors.

### Layout

```
┌──────────────────────────────────────────────────────────────┐
│  SongWalker    [+ Add Slot]   [⚙ Settings]   [♡ Donate]    │  ← Header
├────────────────────┬─────────────────────────────────────────┤
│                    │                                         │
│  Preset Browser    │   Slot Rack (Kontakt-style)             │
│  (songwalker-js    │   ┌─────────────────────────────────┐   │
│   PresetBrowser)   │   │ Slot 1: FluidR3/Grand Piano     │   │
│  ┌──────────────┐  │   │ Ch:1  Vol ━━━  Pan ━  Mute Solo │   │
│  │ 🔍 Search    │  │   │ ┌─────────────────────────────┐ │   │
│  ├──────────────┤  │   │ │ const piano = loadPreset(…) │ │   │
│  │ ▸ FluidR3_GM │  │   │ │                             │ │   │
│  │ ▸ JCLive     │  │   │ │ piano(midi.note)            │ │   │
│  │ ▸ Aspirin    │  │   │ │   *midi.velocity            │ │   │
│  │ [⬇ Offline]  │  │   │ │                             │ │   │
│  ├──────────────┤  │   │ └─────────────────────────────┘ │   │
│  │ Categories   │  │   ├─────────────────────────────────┤   │
│  │ [Piano]      │  │   │ Slot 2: Arpeggiator             │   │
│  │ [Guitar]     │  │   │ Ch:2  Vol ━━━  Pan ━  Mute Solo │   │
│  │ [Drums]      │  │   │ ┌─────────────────────────────┐ │   │
│  │ [Synth]      │  │   │ │ const synth = loadPreset(…) │ │   │
│  │ [Strings]    │  │   │ │ state let step = 0          │ │   │
│  ├──────────────┤  │   │ │ synth(midi.note + …)        │ │   │
│  │ Instruments  │  │   │ │   *midi.velocity            │ │   │
│  │  Piano       │  │   │ └─────────────────────────────┘ │   │
│  │  Strings     │  │   └─────────────────────────────────┘   │
│  │  Guitar      │  │                                         │
│  │  Drums       │  │   ┌─────────────────────────────────┐   │
│  └──────────────┘  │   │  Visualizer (Peak + Spectrum)   │   │
│                    │   │  ▁▂▃▅▇▅▃▂  Peak  [L ████░ R]   │   │
│                    │   └─────────────────────────────────┘   │
├────────────────────┴─────────────────────────────────────────┤
│  Voices: 34/256  │  Slots: 2  │  CPU: 4.1%  │  Cache: 142MB │
└──────────────────────────────────────────────────────────────┘
```

**Slot Rack Behavior:**
- Every slot has a `.sw` source editor (collapsed by default, click to expand).
- Selecting a preset from the browser generates a default `.sw` and assigns it to the active slot.
- Slots can be added, removed, reordered, solo'd, muted.
- Each slot can be assigned a MIDI channel (or "All").
- Drag a preset from the browser onto a slot to load it (replaces the `loadPreset` call in the `.sw`).
- Combination presets ("Orchestra", "Quartet") create multiple slots automatically.

### UI Technology

**Shared songwalker-js components** — the following are reused directly from the npm package:
- `PresetBrowser` — collapsible tree view with lazy-loading, search, pagination
- `PresetLoader` — fetches indexes, presets, decodes audio, resolves archives
- `SongPlayer` — WASM compilation and audio rendering
- Monaco Editor — `.sw` code editing with syntax highlighting and completions

**Rendering approach: egui + embedded webview**
- The outer plugin chrome (slot rack, knobs, meters) is rendered via **egui** (`nih_plug_egui`).
- The preset browser and code editor panels embed a lightweight webview that loads the
  songwalker-js components, providing pixel-perfect parity with the web editor.
- Communication between egui and webview via a bidirectional message channel
  (preset selection → egui updates slot state; egui MIDI state → webview visualizer).

**Fallback: pure egui mode**
- If webview embedding proves fragile in certain DAWs, a pure-egui fallback is available:
  custom widgets for preset browsing, and a syntax-highlighted text editor using
  songwalker-core lexer tokens for coloring.
- Same dark theme and color palette as the web editor in either mode.

**Visual Consistency with Web:**
- Same color scheme (dark background, same accent colors for notes, tracks, errors).
- Same preset browser layout (search → library tree → category chips → preset list).
- Same visualizer style (RMS + peak bars, log-scale FFT spectrum, waveform).
- Same code editor with SongWalker language support (notes = cyan, tracks = yellow, keywords = purple, etc.).

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
    // 2. Check for hot-swapped .sw compilations (atomic pointer compare-exchange)
    // 3. Process MIDI events from host:
    //    - Route by channel to target slot(s)
    //    - Note On → re-execute the entire .sw file for that slot
    //      (injects midi.note, midi.velocity, midi.frequency, midi.gate=true)
    //      (state variables and sequence cursors persist from prior firings)
    //    - Note Off → set midi.gate=false on the corresponding context
    //      (while/sequence loops exit; voices enter release)
    //    - CC / Pitch Bend → fire auxiliary on cc/pitchBend handlers
    //    - Inject host BPM / time sig into transport.* variables
    // 4. For each slot, advance all active execution contexts:
    //    - Run .sw top-to-bottom per context, fire scheduled events
    //    - Batch-render active voices by preset type for cache locality
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

**.sw Execution Model (Reactive Re-Execution):**
- The `.sw` is compiled once (on edit or preset change) → produces a compiled program
  with `state` variable slots, `sequence` cursor slots, and auxiliary `on cc`/`on pitchBend` handlers.
- Each MIDI Note On triggers a **full top-to-bottom re-execution** of the `.sw` file in a new
  **execution context** with:
  - Its own `midi.*` variable state (note, velocity, frequency, gate=true).
  - Access to shared `state` variables (persist across firings, shared across all contexts).
  - Access to shared `sequence` cursors (continue from last position).
  - Injected `transport.*` from host (BPM, time sig, beat position).
- The context runs the entire file body. `while (midi.gate)` loops keep running across
  `process()` calls until the corresponding Note Off sets `gate=false`.
- Preset identifiers in note position (e.g., `kick /4`, `piano(midi.note)`) trigger playback
  using the preset-as-note syntax — no `.play()` method needed.
- On MIDI Note Off: `midi.gate` is set to `false` on the matching context.
  The `on noteOff` auxiliary handler (if any) is executed. Active voices enter release.
- When all voices in a context finish their release, the context is returned to the pool.
- `state` variables and `sequence` cursors survive context cleanup — they are slot-level state.

**Multi-Slot Rendering:**
- Each slot is processed independently (own voice pool, own .sw state/sequences).
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
| Lexer + Parser + AST | Full `.sw` compilation pipeline, extended with reactive MIDI syntax |
| Compiler | `compile()` → compiled program with `state` slots, `sequence` cursors, auxiliary `on cc`/`on pitchBend` handlers |
| `PresetDescriptor` / `PresetNode` types | Deserialization of remote preset JSON |
| DSP engine (`dsp/` module) | Oscillator, Envelope, Filter, Sampler, Mixer, Composite, Voice |
| Tuning system | `TuningInfo`, pitch detection, `noteToFreq` / `freqToNote` |
| Error types | `ariadne`-based diagnostics for editor error display |

### What's New in the VSTi

| Component | Notes |
|-----------|-------|
| Reactive `.sw` runtime | Whole-file re-execution per note, `state` persistence, `sequence` cursors, preset-as-note syntax, auxiliary `on cc`/`on pitchBend` handlers |
| MIDI input handling | Route by channel to slots. Note On spawns `.sw` execution context. Note Off sets `midi.gate=false`. |
| DAW transport sync | Read host BPM, time sig, play state, loop points. Inject as `transport.*` variable defaults. |
| Multi-slot manager | Kontakt-style slot rack. Add/remove/reorder slots. Per-slot MIDI channel, volume, pan. Every slot is a `.sw` source. |
| Voice allocator | Per-slot polyphonic voice pools with stealing. Batch rendering for cache locality. |
| Execution context pool | Pre-allocated per-note contexts (midi.* state, program cursor, voice refs). Polyphonic — one per held note. Shared `state` variables and `sequence` cursors at slot level. |
| HTTP preset fetcher | Async remote loading (reuses songwalker-js PresetLoader where possible). On-demand + "Download for Offline". |
| Disk cache | Cache indexes always, presets on use. LRU eviction. Offline library download. |
| Plugin state save/restore | DAW project serialization (all slot `.sw` sources, parameter overrides, `state` variable values, `sequence` cursor positions). |
| SIMD + perf utilities | Batch voice processing, SIMD sample interpolation, pre-allocated pools. |
| UI (egui + webview) | Slot rack in egui. Preset browser + code editor via embedded webview running songwalker-js components. |

### Required Core Changes

The core needs extension for MIDI-reactive `.sw`:

1. **Preset-as-note syntax in lexer/parser** — Allow preset identifiers in note position: `kick /4` (default pitch), `piano(C4) /4` (explicit pitch), `piano(midi.note) *midi.velocity` (computed pitch). Parser treats identifier-in-note-position as a preset play. Identifier followed by `(expr)` = preset + pitch.
2. **`state` keyword** — New `state let x = 0` declaration. Variables marked `state` are allocated in slot-level persistent storage, not per-execution-context storage. Initialized once, survive across note firings.
3. **`sequence` blocks** — New `sequence { ... }` block type. Cursor position persists at slot level. Without `while`, advances one event per note firing (step sequencer). With `while (midi.gate)`, plays in real-time and suspends on note-off.
4. **Auxiliary `on` handlers** — `on cc(n) { }`, `on pitchBend { }`, `on noteOff { }` for non-note MIDI events. The file body itself is the Note On handler (no `on noteOn` wrapper).
5. **`midi.*` and `transport.*` variables** — Injected read-only variables. `midi.note`, `midi.velocity`, `midi.frequency`, `midi.gate`, `midi.channel`, `midi.pitchBend`, `midi.cc[n]`. `transport.bpm`, `transport.timeSigNum`, etc.
6. **Control flow extensions** — `if/else`, `while`, arithmetic/comparison/logical operators, array literals, ternary operator, built-in functions (`noteToFreq`, `freqToNote`, `interval`, `scaleNote`, `random`, `randomInt`).
7. **Compiler: reactive program model** — `compile()` returns a compiled program suitable for per-note re-execution: the main body (runs per Note On), `state` variable descriptors, `sequence` cursor descriptors, and auxiliary handler blocks.
8. **Runtime variable injection** — The execution engine accepts a `MidiContext` struct (`note`, `velocity`, `frequency`, `gate`, `channel`, `pitchBend`, `cc[]`) and a `TransportContext` struct (`bpm`, `timeSigNum`, `timeSigDen`, `sampleRate`, `playing`, `beat`).
9. **Feature-gate WASM exports** — `#[cfg(target_arch = "wasm32")]` on wasm-bindgen items so they don't compile for native VSTi builds. (May already be gated.)
10. **Expose incremental rendering API** — Current `render_song_samples()` renders the entire song at once. The VSTi needs a `render_block(events, num_samples)` style API for real-time buffer-by-buffer rendering.
11. **Sample rate propagation** — Ensure all DSP nodes accept runtime sample rate (already parameterized in renderer, verify for individual nodes).
12. **Voice pool integration** — The existing `Voice` type may need adaptation for a shared pool with steal/release semantics.

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

### Phase 2 — Preset Loading & Default .sw (Weeks 3–4)

- [ ] Implement `PresetLoader` — async HTTP fetch of index + preset JSON
- [ ] On-demand disk cache (indexes always, presets on use)
- [ ] Decode MP3/WAV samples to f32 PCM at host sample rate
- [ ] Build `PresetNode` DSP graph from `PresetDescriptor`
- [ ] Voice allocator with polyphony management and voice stealing
- [ ] Atomic preset hot-swap (background → audio thread)
- [ ] Auto-generate default `.sw` when preset is selected
- [ ] End-to-end: select preset → default .sw wires MIDI → plays correct samples
- [ ] SIMD-accelerated sample interpolation (SSE2 / NEON)

### Phase 3 — Multi-Slot & Editor UI (Weeks 5–7)

- [ ] Implement slot manager (add/remove/reorder slots)
- [ ] Per-slot MIDI channel routing
- [ ] Per-slot volume, pan, mute, solo
- [ ] Set up `nih_plug_egui` editor scaffold with embedded webview
- [ ] Slot rack UI (Kontakt-style collapsible slot strips, each with .sw editor)
- [ ] Integrate songwalker-js preset browser via webview (same PresetBrowser component)
- [ ] Integrate Monaco .sw code editor via webview
- [ ] "Download for Offline" button per library with progress bar
- [ ] Peak meter + spectrum visualizer
- [ ] Status bar (voice count, slot count, CPU, cache size)
- [ ] Dark theme matching web editor colors

### Phase 4 — Reactive .sw Runtime (Weeks 8–10)

- [ ] Implement preset-as-note syntax: `kick /4`, `piano(C4) /4`, `piano(midi.note) *midi.velocity`
- [ ] Implement `state` keyword — slot-level persistent variables across note firings
- [ ] Implement `sequence` blocks — cursor persistence, step-sequencer and real-time modes
- [ ] Add `midi.*` and `transport.*` variable injection to compiler
- [ ] Add `if/else`, `while`, arithmetic/comparison/logical expressions to parser
- [ ] Add array literals, `.length`, `.indexOf()` to runtime
- [ ] Add built-in functions: `noteToFreq`, `freqToNote`, `interval`, `scaleNote`, `random`, `randomInt`
- [ ] Whole-file re-execution per note → per-note execution context
- [ ] Polyphonic execution: multiple simultaneous MIDI notes = multiple contexts
- [ ] Auxiliary `on cc(n)`, `on pitchBend`, `on noteOff` handlers
- [ ] `while (midi.gate)` loops: suspend across process() calls, exit on note-off
- [ ] Error display with source location markers in editor
- [ ] Verify examples: default preset, arpeggiator, drum kit, harmonizer, step sequencer
- [ ] DAW loop point detection → context reset

### Phase 5 — Polish & Release (Weeks 11–13)

- [ ] Plugin state save/restore (all slot .sw sources, parameters, state vars, sequence cursors)
- [ ] Preset parameter automation (expose per-slot params to DAW)
- [ ] Combination presets ("Orchestra" auto-creates multiple slots with .sw wiring)
- [ ] Cache management UI (per-library size, clear, offline status)
- [ ] Keyboard display widget (shows active notes across all slots)
- [ ] Pure-egui fallback UI for DAWs where webview is unstable
- [ ] Performance profiling: batch rendering, memory layout optimization
- [ ] Profile-guided optimization (`cargo pgo`) for release builds
- [ ] CI/CD pipeline for all platforms
- [ ] User documentation + donation links (GitHub Sponsors, Ko-fi)
- [ ] Beta testing across DAWs

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Webview embedding issues in some DAWs | UI glitches or crashes | Pure-egui fallback mode. Test early in Phase 3 across DAWs. |
| Large preset download sizes | Slow first use | On-demand caching means only used presets are fetched. Progressive index loading. "Download for Offline" is opt-in. |
| Audio glitches during .sw recompilation | Audible artifacts | Crossfade between old and new compiled state (50ms). Voice release before swap. |
| HTTPS blocked in some DAW sandboxes | Can't fetch presets | Offline mode from cache. "Download for Offline" when connected. Manual import of preset packs. Local file server option. |
| Multi-slot voice count explosion | CPU overload | Per-slot and global voice limits. Aggressive voice stealing. CPU meter in UI. Auto-reduce polyphony under load. |
| while(midi.gate) infinite loop risk | CPU hang if gate never clears | Timeout per execution context. Max iterations per process() call. Watchdog kills stuck contexts. |
| MIDI-reactive .sw timing drift | Audible timing issues | Beat-quantized context start (snap to nearest subdivision). Host transport as authoritative clock. |
| songwalker-core syntax extensions | Increased complexity | Implement incrementally. if/else and while first, then on handlers, then expressions. Thorough test suite per feature. |
| Cross-platform build complexity | Missing targets | Use `cross` for Linux→Windows. GitHub Actions matrix. Test on real machines. |
| Multi-timbral parameter explosion | DAW confusion | Expose only active slot parameters. Dynamic parameter list (nih-plug supports this). |

---

## Decisions (Resolved)

| # | Question | Decision |
|---|----------|----------|
| 1 | GUI framework | **egui + embedded webview** for songwalker-js component reuse. Pure-egui fallback for incompatible DAWs. |
| 2 | Preset caching | **On-demand.** Cache library indexes + used presets. "Download for Offline" button per library for bulk caching. |
| 3 | Preset vs Runner modes | **Unified.** No separate modes. Every slot is a `.sw` source. Preset selection auto-generates a 2-line default `.sw`. Users edit for advanced behavior. |
| 4 | Execution model | **Reactive re-execution.** Entire `.sw` file re-runs on every MIDI note (like React rendering). `state` persists across firings. `sequence` blocks continue from saved cursor. No `on noteOn` wrapper — the file body IS the note handler. Auxiliary `on cc`/`on pitchBend` for non-note events. |
| 5 | Note syntax | **Preset-as-note.** Preset identifiers in note position: `kick /4` (default pitch), `piano(midi.note) *midi.velocity` (computed pitch). No `.play()` method needed. |
| 6 | Multi-timbral | **Yes, Kontakt-style.** Multiple named `.sw` slots. Required for combination presets (orchestra, quartet, layered instruments). |
| 7 | Licensing | **Free & open source.** GPLv3 (or similar). Donation-based (GitHub Sponsors, Ko-fi, etc.). No paywalls. |
| 8 | Plugin name | **SongWalker** |
