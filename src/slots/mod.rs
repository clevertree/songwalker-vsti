//! Multi-timbral slot system (Kontakt-style rack).
//!
//! Each slot is a unified instrument that handles MIDI → preset playback
//! and optionally runs `.sw` source code. This matches the web editor
//! model where presets are loaded via `loadPreset()` in source code.

pub mod preset_slot;
pub mod runner_slot;
pub mod slot;

pub use slot::Slot;

/// Maximum number of simultaneous slots.
pub const MAX_SLOTS: usize = 16;

/// Manages the collection of instrument slots.
pub struct SlotManager {
    slots: Vec<Slot>,
    sample_rate: f32,
}

impl SlotManager {
    pub fn new_empty() -> Self {
        Self {
            slots: Vec::with_capacity(MAX_SLOTS),
            sample_rate: 44100.0,
        }
    }

    /// Pre-allocate all slots. Must be called from a thread that allows allocation (e.g., initialize()).
    pub fn allocate_all(&mut self) {
        if self.slots.is_empty() {
            for i in 0..MAX_SLOTS {
                let mut slot = Slot::new(i);
                slot.initialize(self.sample_rate);
                self.slots.push(slot);
            }
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
    pub fn add_slot(&mut self) -> Option<usize> {
        if self.slots.len() >= MAX_SLOTS {
            return None;
        }
        let idx = self.slots.len();
        let mut slot = Slot::new(idx);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_manager_new_empty() {
        let sm = SlotManager::new_empty();
        assert_eq!(sm.slot_count(), 0);
        assert!(!sm.any_solo());
    }

    #[test]
    fn test_slot_manager_allocate_all() {
        let mut sm = SlotManager::new_empty();
        sm.allocate_all();
        assert_eq!(sm.slot_count(), MAX_SLOTS);
    }

    #[test]
    fn test_slot_manager_allocate_all_idempotent() {
        let mut sm = SlotManager::new_empty();
        sm.allocate_all();
        sm.allocate_all(); // Should not add more
        assert_eq!(sm.slot_count(), MAX_SLOTS);
    }

    #[test]
    fn test_slot_manager_add_slot() {
        let mut sm = SlotManager::new_empty();
        let idx = sm.add_slot();
        assert_eq!(idx, Some(0));
        assert_eq!(sm.slot_count(), 1);

        let idx2 = sm.add_slot();
        assert_eq!(idx2, Some(1));
        assert_eq!(sm.slot_count(), 2);
    }

    #[test]
    fn test_slot_manager_max_slots() {
        let mut sm = SlotManager::new_empty();
        for _ in 0..MAX_SLOTS {
            assert!(sm.add_slot().is_some());
        }
        assert_eq!(sm.add_slot(), None);
        assert_eq!(sm.slot_count(), MAX_SLOTS);
    }

    #[test]
    fn test_slot_manager_remove_slot() {
        let mut sm = SlotManager::new_empty();
        sm.add_slot();
        sm.add_slot();
        sm.add_slot();
        assert_eq!(sm.slot_count(), 3);

        let removed = sm.remove_slot(1);
        assert!(removed);
        assert_eq!(sm.slot_count(), 2);
        // Verify re-indexing
        assert_eq!(sm.slots()[0].index(), 0);
        assert_eq!(sm.slots()[1].index(), 1);
    }

    #[test]
    fn test_slot_manager_remove_last_slot_rejected() {
        let mut sm = SlotManager::new_empty();
        sm.add_slot();
        // Cannot remove the last slot
        let removed = sm.remove_slot(0);
        assert!(!removed);
        assert_eq!(sm.slot_count(), 1);
    }

    #[test]
    fn test_slot_manager_remove_out_of_bounds() {
        let mut sm = SlotManager::new_empty();
        sm.add_slot();
        sm.add_slot();
        let removed = sm.remove_slot(10);
        assert!(!removed);
        assert_eq!(sm.slot_count(), 2);
    }

    #[test]
    fn test_slot_manager_initialize() {
        let mut sm = SlotManager::new_empty();
        sm.add_slot();
        sm.initialize(48000.0);
        // Just verify it doesn't panic
        assert_eq!(sm.slot_count(), 1);
    }

    #[test]
    fn test_slot_manager_reset() {
        let mut sm = SlotManager::new_empty();
        sm.add_slot();
        sm.reset();
        // Just verify it doesn't panic
        assert_eq!(sm.slot_count(), 1);
    }
}
