//! Multi-timbral slot system (Kontakt-style rack).
//!
//! Each slot is independently a Preset slot or a Runner slot.
//! Slots can be added, removed, reordered, solo'd, and muted.

pub mod preset_slot;
pub mod runner_slot;
pub mod slot;

pub use slot::{Slot, SlotMode};

/// Maximum number of simultaneous slots.
pub const MAX_SLOTS: usize = 16;

/// Manages the collection of instrument slots.
pub struct SlotManager {
    slots: Vec<Slot>,
    sample_rate: f32,
}

impl SlotManager {
    pub fn new() -> Self {
        // Start with one empty preset slot
        Self {
            slots: vec![Slot::new(0, SlotMode::Preset)],
            sample_rate: 44100.0,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for slot in &mut self.slots {
            slot.initialize(sample_rate);
        }
    }

    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.reset();
        }
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    pub fn slots_mut(&mut self) -> &mut Vec<Slot> {
        &mut self.slots
    }

    /// Add a new slot. Returns the slot index, or None if max reached.
    pub fn add_slot(&mut self, mode: SlotMode) -> Option<usize> {
        if self.slots.len() >= MAX_SLOTS {
            return None;
        }
        let idx = self.slots.len();
        let mut slot = Slot::new(idx, mode);
        slot.initialize(self.sample_rate);
        self.slots.push(slot);
        Some(idx)
    }

    /// Remove a slot by index.
    pub fn remove_slot(&mut self, index: usize) -> bool {
        if index < self.slots.len() && self.slots.len() > 1 {
            self.slots.remove(index);
            // Re-index remaining slots
            for (i, slot) in self.slots.iter_mut().enumerate() {
                slot.set_index(i);
            }
            true
        } else {
            false
        }
    }

    /// Check if any slot has solo enabled.
    pub fn any_solo(&self) -> bool {
        self.slots.iter().any(|s| s.is_solo())
    }
}
