# Interactive Piano Keyboard — VSTi Plan

## Overview

Add a toggleable piano keyboard widget to the bottom of the VSTi editor. The piano
targets the currently selected slot in the slot rack. When a key is pressed, the VSTi
synthesizes the note through the slot's instrument (preset or source-code-defined),
producing immediate audio feedback without needing the DAW to send MIDI.

## Current State

### Editor Layout (800×600)

```
┌─────────────────────────────────────────┐
│ Header: SongWalker VSTi | Tabs | Donate │  TopBottomPanel::top
├──────────┬──────────────────────────────┤
│ Browser  │                              │
│ (left    │  Central: Slot Rack or       │
│  panel,  │  Settings tab                │
│  200px)  │                              │
│          │                              │
├──────────┴──────────────────────────────┤
│ Status: Ready | Voices | CPU | Cache    │  TopBottomPanel::bottom
└─────────────────────────────────────────┘
```

### Audio Thread Communication

- Editor state: `Arc<Mutex<PluginState>>` (shared with audio thread)
- No existing mechanism for editor → audio thread MIDI events
- MIDI events come from the host via `context.next_event()` in `process()`
- `crossbeam-channel = "0.5"` is already in Cargo.toml

## Plan

### 1. New Module: `src/editor/piano.rs`

**`PianoState`:**
```rust
pub struct PianoState {
    pub visible: bool,                    // toggle via header button
    pub octave_offset: i8,               // shift range (default 0 = C3–B4)
    pub active_notes: HashSet<u8>,       // visually highlighted keys
}
```

**`PianoEvent` enum:**
```rust
pub enum PianoEvent {
    NoteOn { note: u8, velocity: f32 },
    NoteOff { note: u8 },
}
```

**`draw()` function:**
- Renders a 2-octave piano (C3–B4, 24 keys) using custom egui painting
- White keys: 14 tall rectangles, `colors::TEXT` fill
- Black keys: 10 shorter overlaid rectangles, `colors::CRUST` fill, aligned to bottom
  of allocated rect (per user request — inverted from traditional layout)
- Active notes highlighted with `colors::BLUE`
- Left side: octave shift buttons (`◀` `▶`) and range label ("C3–B4")
- Mouse interaction:
  - Pointer press → determine key (check black keys first, they overlap white),
    send `PianoEvent::NoteOn` via crossbeam sender, add to `active_notes`
  - Pointer release → send `PianoEvent::NoteOff`, remove from `active_notes`
  - Drag across keys → re-trigger on enter new key

### 2. Wire into Editor Layout

**In `src/editor/mod.rs`:**

- Register module: `pub mod piano;`
- Add to `EditorState`:
  ```rust
  pub piano_state: piano::PianoState,
  pub piano_tx: crossbeam_channel::Sender<piano::PianoEvent>,
  ```
- Add toggle button in header bar: `⌨` icon next to tab selectors
- Add `TopBottomPanel::bottom("piano")` **before** the status bar panel:
  - Only shown when `piano_state.visible`
  - Fixed height ~80px
  - Frame: `CRUST` background, 1px `SURFACE0` top border

### 3. Crossbeam Channel: Editor → Audio Thread

**In `src/plugin.rs`:**

- In `SongWalkerPlugin::default()`:
  ```rust
  let (piano_tx, piano_rx) = crossbeam_channel::bounded(64);
  ```
- Store `piano_rx: crossbeam_channel::Receiver<PianoEvent>` on the plugin struct
- Pass `piano_tx.clone()` to `editor::create()`

**In `src/audio.rs` `process_block()`:**
- At the start, drain `piano_rx`:
  ```rust
  while let Ok(event) = piano_rx.try_recv() {
      let note_event = match event {
          PianoEvent::NoteOn { note, velocity } => NoteEvent::NoteOn {
              timing: 0, voice_id: None, channel: 0, note, velocity,
          },
          PianoEvent::NoteOff { note } => NoteEvent::NoteOff {
              timing: 0, voice_id: None, channel: 0, note, velocity: 0.0,
          },
      };
      // Route to selected slot specifically (not broadcast)
      if let Some(slot) = slot_manager.slots_mut().get_mut(selected_slot_idx) {
          slot.handle_midi_event(&note_event, transport);
      }
  }
  ```

**Selected slot index:** The piano targets `slot_rack_state.selected_slot`. This value
lives in `EditorState` (GUI thread). To share it with the audio thread, either:
- (a) Add `selected_piano_slot: Arc<AtomicUsize>` shared between editor and plugin
- (b) Send the slot index as part of `PianoEvent`
- Option (b) is simpler — extend `PianoEvent::NoteOn/Off` with a `slot_index: usize` field

### 4. Updated Layout With Piano

```
┌─────────────────────────────────────────┐
│ Header: SongWalker VSTi | Tabs | ⌨ | ♥ │
├──────────┬──────────────────────────────┤
│ Browser  │                              │
│ (left    │  Central: Slot Rack or       │
│  panel)  │  Settings tab                │
│          │                              │
├──────────┴──────────────────────────────┤
│ Piano: ◀ C3–B4 ▶ [white+black keys]    │  TopBottomPanel::bottom (togglable)
├─────────────────────────────────────────┤
│ Status: Ready | Voices | CPU | Cache    │  TopBottomPanel::bottom
└─────────────────────────────────────────┘
```

## Key Layout: Black Keys at Bottom

Per user request, black keys are aligned to the **bottom** of the piano rect. This
inverts the traditional piano layout:

```
  ┌──┬──┬──┬──┬──┬──┬──┬──┬──┬──┬──┬──┬──┬──┐
  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  White keys (full height)
  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │
  │  │▓▓│▓▓│  │▓▓│▓▓│▓▓│  │▓▓│▓▓│  │▓▓│▓▓│▓▓│  Black keys (bottom 60%)
  └──┴──┴──┴──┴──┴──┴──┴──┴──┴──┴──┴──┴──┴──┘
       C# D#      F# G# A#      C# D#      F# G# A#
```

## Streaming Execution Note

The core plan introduces a **streaming execution model** with a `SongRunner`
and `EventBuffer` (see `songwalker-core/docs/cursor_aware_plan.md`,
Architectural Decision #2). The VSTi piano does **not** need streaming — it
sends notes directly to the slot's DSP engine via crossbeam channel. However,
when the VSTi gains a source-code editing mode (playing `.sw` files), it will
use the `SongRunner` for playback instead of the current batch render path.

## Depends On

- **songwalker-core cursor-aware API** — not needed for basic VSTi piano (VSTi uses
  its own slot instrument). The core APIs are needed for the *web editor* piano.
- **crossbeam-channel** — already in Cargo.toml

## Outstanding Questions

1. **Velocity mapping:** Should mouse Y-position within a key map to velocity? (top of
   key = soft, bottom = hard?) Or use a fixed velocity (0.8)?

2. **Keyboard shortcuts:** Should QWERTY keyboard rows map to piano keys (like a DAW)?
   Z=C, X=D, C=E, V=F, B=G, N=A, M=B, with S/D/G/H/J for sharps? This conflicts
   with potential editor shortcuts.

3. **Polyphony:** The mouse can only press one key at a time. Should we support
   computer keyboard for polyphonic input (multiple keys held)?

4. **Note-off timing:** When the user clicks and releases quickly, the note should
   still have a minimum duration (e.g., 100ms gate) to be audible. Handle this with
   ADSR release in the slot, or enforce a minimum gate in the piano?

5. **Slot with no preset:** If the selected slot is empty (no preset assigned, no
   source code), the piano press would trigger a sine-wave fallback from the slot's
   `render()` function. Is this acceptable, or should the piano show a warning?

## File Impact

| File | Changes |
|------|---------|
| `src/editor/piano.rs` | **NEW** — PianoState, PianoEvent, draw() |
| `src/editor/mod.rs` | Register module, add to EditorState, toggle button, bottom panel |
| `src/plugin.rs` | Create crossbeam channel, store Receiver, pass Sender to editor |
| `src/audio.rs` | Drain piano_rx at start of process_block, route to selected slot |
