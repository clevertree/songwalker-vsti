# SongWalker VSTi

A cross-platform **VST3 / CLAP** instrument plugin that brings the SongWalker preset library and `.sw` playback engine into any DAW.

Every slot is a `.sw` program that **re-executes on every MIDI note** — like a React component re-rendering. The built-in `song` variable carries the previous execution's state (active voices, timing, user properties), enabling pitch bending, loop continuation, chord matching, and more.

## Key Concepts

### Reactive Execution

Each MIDI Note On triggers a full re-execution of the slot's `.sw` source. There are no separate "modes" — a simple 2-line preset plays notes; adding logic enables advanced behaviors.

### The `song` Variable

| Property | Type | Description |
|----------|------|-------------|
| `song` | `object \| undefined` | `undefined` on first firing; snapshot of previous execution on consecutive firings |
| `song.voices` | `Voice[]` | Active voices from the previous firing |
| `song.note` | `number` | MIDI note of the previous firing |
| `song.velocity` | `number` | Velocity of the previous firing |
| `song.elapsed` | `number` | Seconds since the previous firing |
| `song.beat` | `number` | Beats since the previous firing |
| `song.cursor` | `number` | User-settable position counter (for sequencing) |
| `song.*` | `any` | User-defined properties (write any property, read it next firing) |

Voice manipulation methods: `.pitchBend(target, duration)`, `.release()`, `.stop()`, `.extend(duration)`.

### Preset-as-Note Syntax

```sw
kick /4                    # Percussion — plays at default pitch
piano(C4) /4               # Melodic — explicit pitch
piano(midi.note) *midi.velocity  # Melodic — from MIDI input
```

## Scenarios

### Pitch Bend on Consecutive Notes

First note plays normally. Consecutive notes grab the active voice and glide to the new pitch.

```sw
const synth = loadPreset("FluidR3_GM/Synth Strings 1")
const bendTime = 1/4

if (song && song.voices.length > 0) {
    song.voices[0].pitchBend(midi.note, bendTime)
} else {
    synth(midi.note) *midi.velocity
}
```

### Close Harmony (Chord Matching)

Each note is matched to the closest lower chord tone. `song` is not used — each note is independent.

```sw
const piano = loadPreset("FluidR3_GM/Acoustic Grand Piano")
const baseNote = C3
const chord = [0, 4, 7]  // major triad

const interval = (midi.note - baseNote) % 12
let bestMatch = chord[0]
for i in 0..chord.length {
    if (chord[i] <= interval) { bestMatch = chord[i] }
}
const harmonyNote = midi.note - (interval - bestMatch)

piano(midi.note) *midi.velocity
piano(harmonyNote) *(midi.velocity * 0.7)
```

### Drum Loop with Gap Tolerance

Loop continues across note firings within a beat gap. If the gap is exceeded, voices release naturally and the next note restarts the loop.

```sw
const kick  = loadPreset("FluidR3_GM/Standard Kit/Kick")
const hat   = loadPreset("FluidR3_GM/Standard Kit/Closed Hi-Hat")
const snare = loadPreset("FluidR3_GM/Standard Kit/Snare")
const gapBeats = 2

if (song && song.elapsed < gapBeats * (60 / transport.bpm)) {
    song.cursor = song.cursor
} else {
    song.cursor = 0
}

while (midi.gate) {
    kick /4
    hat /8
    hat /8
    snare *0.9 /4
    hat /8
    hat /8
    song.cursor += 6
}
```

## Architecture

- **nih-plug** — VST3/CLAP plugin framework
- **songwalker-core** — Rust DSP engine (lexer, parser, compiler, sampler, mixer)
- **songwalker-js** — Shared preset browser and Monaco editor (via embedded webview)
- **Remote preset loading** — Fetches from `songwalker-library` (GitLab/GitHub Pages), caches on disk

## Building

```bash
# Plugin bundle (VST3 + CLAP)
cargo xtask bundle songwalker_vsti --release

# Standalone binary
cargo build --release

# Run tests
cargo test
```

## Supported Platforms

- Windows (x86_64)
- macOS (x86_64 + aarch64)
- Linux (x86_64)

## License

GPL-3.0-or-later

## Documentation

See [docs/plan.md](docs/plan.md) for the full project plan including architecture, syntax reference, examples, and development phases.
