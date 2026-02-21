# TDD & Feature Implementation Plan — songwalker-vsti

**Date:** 2025-06-30  
**Scope:** Unit tests, piano keyboard fixes, visualizer improvements, audio bug fixes  
**Approach:** Test-Driven Development — write tests first, then implement fixes

---

## Audit Summary

### Existing Test Coverage
- `src/perf/pool.rs` — MixBuffer tests (5 tests)
- `src/perf/simd.rs` — mix_add, apply_gain tests (2 tests)
- `src/slots/slot.rs` — VoicePool + Slot tests (4+ tests)
- **Everything else:** untested

### Issues Found

| File | Issue | Severity |
|------|-------|----------|
| `piano.rs` | Black keys positioned at TOP of piano area (user wants BOTTOM) | High |
| `visualizer.rs` | RMS bar drawn under peak bar (invisible — peak always ≥ RMS) | Medium |
| `visualizer.rs` | `clear()` method never called — dead code | Low |
| `visualizer.rs` | Vec allocation per frame for waveform points | Low |
| `audio.rs` | Step 2 clears entire buffer, steps 3-4 only process `num_samples` — tail data stale if buffer oversized | Medium |
| `midi.rs` | `midi_to_freq()` and `velocity_to_float()` appear unused | Low |
| `mod.rs` | `zs()` is a no-op → `apply_zoom_to_style()` runs every frame doing nothing | Low |
| `mod.rs` | Hardcoded "CPU: 0.0%" and "Cache: 0 MB" placeholders | Low |
| `mod.rs` | `_screen_size` and `_corner_id` unused variables | Low |
| `plugin.rs` | `sample_rate` field stored but accessed via `audio_engine.sample_rate()` | Low |
| `plugin.rs` | Editor channel re-creation on editor close/reopen could lose in-flight loads | Medium |

---

## Phase 1: Unit Tests for Pure Functions

Add `#[cfg(test)] mod tests` to files that lack them. Focus on deterministic, pure functions first.

### 1a. `midi.rs` tests
```
- test_midi_to_freq_a4: midi_to_freq(69) ≈ 440.0
- test_midi_to_freq_c4: midi_to_freq(60) ≈ 261.63
- test_midi_to_freq_boundaries: midi_to_freq(0) > 0, midi_to_freq(127) < 20000
- test_velocity_to_float_clamp: values at 0.0, 0.5, 1.0, >1.0, <0.0
- test_event_channel_note_on: verify channel extraction
- test_event_channel_wildcard: unknown events return 0
```

### 1b. `audio.rs` tests
```
- test_constant_power_pan_center: pan=0.0 → equal L/R (≈0.707)
- test_constant_power_pan_left: pan=-1.0 → L=1.0, R≈0.0
- test_constant_power_pan_right: pan=1.0 → L≈0.0, R=1.0
- test_constant_power_pan_symmetry: pan(x) L == pan(-x) R
```

### 1c. `piano.rs` tests
```
- test_is_black_key: verify C#, D#, F#, G#, A# are black
- test_is_white_key: verify C, D, E, F, G, A, B are white
- test_note_name_c4: note_name(60) == "C4"
- test_note_name_a4: note_name(69) == "A4"
- test_base_note_default: octave_offset=0 → 48 (C3)
- test_base_note_clamping: extreme offsets clamp to 0..108
- test_range_label: format check for "C3–B4"
```

### 1d. `visualizer.rs` tests
```
- test_push_wraps: push 512+ samples, cursor wraps to 0
- test_update_levels_peak_accumulate: multiple calls keep max peak
- test_decay_levels: peak decays toward zero
- test_decay_levels_floor: very small values snap to 0.0
- test_clear: all fields reset
```

### 1e. `state.rs` tests
```
- test_plugin_state_serialize_roundtrip: to_bytes → from_bytes
- test_slot_config_default: verify default values
- test_add_remove_slot_config: add/remove operations
```

### 1f. `slots/mod.rs` tests
```
- test_slot_manager_new_empty: starts with 0 slots
- test_slot_manager_allocate_all: 16 slots after allocate
- test_slot_manager_add_slot: incremental add
- test_slot_manager_remove_slot: remove + re-index
- test_slot_manager_max_slots: add returns None at 16
- test_any_solo: solo detection
```

### 1g. `preset_slot.rs` tests
```
- test_default_values: pitch_bend=0, expression=1, etc.
- test_handle_cc1_mod_wheel: CC1 sets mod_wheel
- test_handle_cc11_expression: CC11 sets expression
- test_load_unload_preset: load sets fields, unload clears them
```

---

## Phase 2: Fix Piano Keyboard

**Goal:** Black keys aligned to the BOTTOM of the piano container (like a real piano viewed from above, or matching the web editor layout).

### Changes
1. **Flip black key Y position:** Currently `egui::pos2(x, rect.top())` → change to `egui::pos2(x, rect.bottom() - black_key_height)` so black keys hang from the bottom edge
2. **Add tests first** for the layout calculations (extracted into testable helpers)
3. **Verify** glissando (mouse drag) still works across the new layout

### Test plan
```
- test_black_key_y_at_bottom: key rect min_y == panel_bottom - black_key_height
- test_white_key_spans_full_height: white key top == panel_top, bottom == panel_bottom
- test_hit_detection_black_over_white: pointer in black key area → black key wins
```

---

## Phase 3: Fix Visualizer

### 3a. Fix RMS/Peak meter ordering
**Problem:** RMS bar is drawn first, then peak bar is drawn on top. Since peak ≥ RMS always, RMS is invisible.  
**Fix:** Draw peak first (background), then RMS on top with distinct brighter color.

### 3b. Remove per-frame Vec allocation
**Problem:** `draw_channel()` collects all points into a `Vec<Pos2>` each frame.  
**Fix:** Use `windows(2)` directly on ring buffer indices without collecting.

### Test plan
```
- test_rms_bar_visible: RMS rect is distinguishable from peak rect
- (visual verification via standalone)
```

---

## Phase 4: Fix Audio & Clean Up Dead Code

### 4a. Remove `apply_zoom_to_style()` dead code
Since `zs()` is a no-op, `apply_zoom_to_style()` sets fonts/spacing to their default values every frame. Remove it entirely — `ctx.set_pixels_per_point()` handles zoom.

### 4b. Remove unused variables
- `_screen_size` in `draw_editor()`
- `_corner_id` in `draw_resize_corner()`
- `sample_rate` field in `SongWalkerPlugin` (already accessible via `audio_engine.sample_rate()`)

### 4c. Verify `midi_to_freq` / `velocity_to_float` usage
If truly unused, mark `#[allow(dead_code)]` with a comment explaining they're public API for downstream use, or remove them if they'll never be needed.

---

## Phase 5: Build, Run, Verify

1. `cargo test` — all new + existing tests pass
2. `cargo build` — clean compile, no warnings
3. Run standalone (`./run-standalone.sh`) with logs
4. Verify piano renders correctly (black keys at bottom)
5. Verify visualizer shows both peak and RMS bars
6. Test mouse drag across piano keys → audio output
7. Test preset loading → piano playback

---

## File Change Summary

| File | Changes |
|------|---------|
| `src/midi.rs` | Add `#[cfg(test)] mod tests` (6 tests) |
| `src/audio.rs` | Make `constant_power_pan` public, add tests (4 tests) |
| `src/editor/piano.rs` | Make `is_black_key`/`note_name` public, add tests (7 tests), fix black key Y position |
| `src/editor/visualizer.rs` | Add tests (5 tests), fix RMS/peak draw order, remove Vec alloc |
| `src/state.rs` | Add tests (3 tests) |
| `src/slots/mod.rs` | Add tests (6 tests) |
| `src/slots/preset_slot.rs` | Add tests (4 tests) |
| `src/editor/mod.rs` | Remove `apply_zoom_to_style()`, remove `_screen_size`/`_corner_id`, clean up `zs()` calls |
