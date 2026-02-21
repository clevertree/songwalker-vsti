# MIDI Piano & Visualizer Implementation Plan

**Objective**: Achieve feature parity between the SongWalker Web Editor and VSTi Plugin for interactive MIDI testing and real-time audio visualization.

## 1. MIDI Piano Keyboard (Web & VSTi)

### Requirements
- **Bottom-Aligned**: The piano should be docked at the bottom of the main UI container.
- **Glissando Interaction**: Dragging the mouse across keys should trigger successive NoteOn/NoteOff events.
- **Visual Feedback**: Keys should highlight when pressed (via mouse or external MIDI).
- **Octave Shifting**: Controls to shift the visible range (e.g., C0–C8).
- **Targeting**: Triggered notes should target the "active instrument" (slot in VSTi, cursor position in Web).

### Implementation (VSTi - egui)
1. **Refine `piano.rs`**: 
    - Ensure `allocate_exact_size` with `Sense::click_and_drag()` is handling the pointer state correctly.
    - Implement a "capture" state so that dragging from a white key to a black key (or vice-versa) properly swaps the notes.
    - Handle `NoteOff` for the previous note when the pointer moves to a new key *during* a drag.
2. **Persistence**: Ensure it's clearly visible and doesn't conflict with the visualizer panel.

### Implementation (Web - HTML/CSS/JS)
1. **Components**:
    - Build a CSS-based piano layout in `index.html`.
    - Use `pointerdown`, `pointerenter`, `pointerleave`, and `pointerup` for smooth glissando behavior.
2. **Audio Bridge**:
    - In `songwalker-js`, implement a `playNote(note, instrument)` method that uses the WASM engine to render a short burst of audio for previewing without waiting for a full song render.

## 2. Real-time Visualizer (Peak & Waveform)

### Requirements
- **Peak Bars**: Vertical (or horizontal) bars showing the instantaneous and peak (hold) volumes for Left and Right channels.
- **Oscilloscope**: Scrolling waveform view showing the last 50-100ms of audio.
- **Performance**: Must run at 60fps without impacting audio processing (low-overhead shared memory or ring buffers).

### Implementation (VSTi - egui/audio)
1. **Update `VisualizerState`**: Add fields for `peak_l`, `peak_r`, and `rms`.
2. **Audio Thread**: In `audio.rs`, calculate the peak of each processed block and send to the UI.
3. **Drawing**: In `visualizer.rs`, add the peak meter UI alongside the existing waveform drawing.

### Implementation (Web)
1. **AnalyserNode**: Use `AudioContext.createAnalyser()` to get real-time frequency and time-domain data.
2. **Canvas Rendering**: Use a `<canvas>` element to draw both the peak meters and the oscilloscope, mimicking the VSTi's appearance.

## 3. Core Logic Extensions (songwalker-core)

- **Instrument Discovery**: Implement `get_instrument_at_cursor(source: &str, byte_offset: usize) -> Option<InstrumentId>` to allow the Web UI to know which instrument to play when the user clicks the piano.
- **Live Rendering**: Optimize `render_song` (or create `render_note`) to handle single-note playback with minimal latency.

## Phase Phased Rollout Schedule

1. **Phase A (VSTi Visualizer)**: Add peak meters to VSTi to debug current "audio issues" (clipping/silence).
2. **Phase B (VSTi Piano)**: Refine glissando and note-off logic.
3. **Phase C (Web Piano & Visualizer)**: Implement the UI and WASM wiring in `songwalker-js` and `songwalker-site`.
4. **Phase D (Integration)**: Final testing of MIDI flow from Piano -> Engine -> Visualizer across both platforms.
