use std::collections::HashMap;
use std::sync::Arc;

use songwalker_core::preset::{CatalogEntry, LibraryIndex};

use super::cache::DiskCache;
use super::loader::PresetLoader;
use crate::slots::preset_slot::PresetInstance;

/// Status of a library in the manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryStatus {
    /// Index not yet fetched.
    NotLoaded,
    /// Index is being fetched.
    Loading,
    /// Index loaded and browseable.
    Loaded,
    /// Fully downloaded for offline use.
    Offline,
    /// Failed to load.
    Error(String),
}

/// Info about a library.
#[derive(Debug, Clone)]
pub struct LibraryInfo {
    pub name: String,
    pub entry_count: usize,
    pub status: LibraryStatus,
    pub enabled: bool,
}

/// Status of a preset being loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetLoadStatus {
    NotLoaded,
    Loading,
    Loaded,
    Error(String),
}

/// Manages the in-memory registry of available libraries and loaded presets.
///
/// This runs on a background thread for I/O. The audio thread reads
/// loaded presets via atomic pointer swaps.
pub struct PresetManager {
    /// Known libraries and their status.
    pub libraries: Vec<LibraryInfo>,
    /// Catalog entries per library (library name → entries).
    pub catalogs: HashMap<String, Vec<CatalogEntry>>,
    /// Base URL for the library.
    pub base_url: String,
    /// Search query for filtering presets.
    pub search_query: String,
    /// Selected category filter (None = all).
    pub category_filter: Option<String>,
    /// Whether the background refresh has been started this session.
    pub refresh_started: bool,
}

impl PresetManager {
    pub fn new() -> Self {
        Self {
            libraries: Vec::new(),
            catalogs: HashMap::new(),
            base_url: super::loader::DEFAULT_LIBRARY_URL.to_string(),
            search_query: String::new(),
            category_filter: None,
            refresh_started: false,
        }
    }

    /// Start the initial background refresh of library indexes.
    ///
    /// Called once at plugin initialization. Fetches the root index
    /// and populates the library list.
    pub fn start_background_refresh(&mut self) {
        if self.refresh_started {
            return;
        }
        self.refresh_started = true;

        let cache = DiskCache::new();
        let _ = cache.ensure_dirs();

        // Load cached root index if available
        if let Some(cached) = cache.read_root_index() {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&cached) {
                self.parse_root_index(&root);
            }
        }

        // Load cached library indexes
        for lib in &self.libraries {
            if let Some(cached) = cache.read_library_index(&lib.name) {
                if let Ok(index) = serde_json::from_str::<LibraryIndex>(&cached) {
                    self.catalogs
                        .insert(lib.name.clone(), index.presets);
                }
            }
        }
    }

    /// Parse the root index JSON and populate the library list.
    fn parse_root_index(&mut self, root: &serde_json::Value) {
        self.libraries.clear();

        if let Some(libs) = root.as_array() {
            for lib in libs {
                let name = lib
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let count = lib
                    .get("entries")
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0) as usize;

                let cache = DiskCache::new();
                let status = if cache.is_offline(&name) {
                    LibraryStatus::Offline
                } else if self.catalogs.contains_key(&name) {
                    LibraryStatus::Loaded
                } else {
                    LibraryStatus::NotLoaded
                };

                self.libraries.push(LibraryInfo {
                    name,
                    entry_count: count,
                    status,
                    enabled: true, // All enabled by default
                });
            }
        }
    }

    /// Get all catalog entries matching the current search/filter, across enabled libraries.
    pub fn filtered_entries(&self) -> Vec<(&str, &CatalogEntry)> {
        let mut results = Vec::new();
        let query = self.search_query.to_lowercase();

        for lib in &self.libraries {
            if !lib.enabled {
                continue;
            }
            if let Some(entries) = self.catalogs.get(&lib.name) {
                for entry in entries {
                    // Category filter
                    if let Some(ref cat) = self.category_filter {
                        let entry_cat = format!("{:?}", entry.category);
                        if &entry_cat != cat {
                            continue;
                        }
                    }

                    // Search filter
                    if !query.is_empty() {
                        let name_lower = entry.name.to_lowercase();
                        if !name_lower.contains(&query) {
                            continue;
                        }
                    }

                    results.push((lib.name.as_str(), entry));
                }
            }
        }

        results
    }

    /// Get all unique categories across loaded catalogs.
    pub fn available_categories(&self) -> Vec<String> {
        let mut cats = std::collections::HashSet::new();
        for entries in self.catalogs.values() {
            for entry in entries {
                cats.insert(format!("{:?}", entry.category));
            }
        }
        let mut sorted: Vec<String> = cats.into_iter().collect();
        sorted.sort();
        sorted
    }
}
