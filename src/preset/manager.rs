use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::cache::DiskCache;
use super::loader::PresetLoader;

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

/// Info about a library (sub-index entry from root index).
#[derive(Debug, Clone)]
pub struct LibraryInfo {
    pub name: String,
    /// Relative path to library's index.json.
    pub path: String,
    pub description: String,
    pub preset_count: usize,
    pub status: LibraryStatus,
    pub expanded: bool,
}

/// A preset entry loaded from a library index.
#[derive(Debug, Clone)]
pub struct PresetInfo {
    pub name: String,
    pub path: String,
    pub category: String,
    pub tags: Vec<String>,
    pub gm_program: Option<u8>,
    pub zone_count: u32,
}

/// Manages the in-memory registry of available libraries and loaded presets.
///
/// The editor UI reads from this via Arc<Mutex<>>. Background threads
/// update it after HTTP fetches complete.
pub struct PresetManager {
    /// Known libraries from the root index.
    pub libraries: Vec<LibraryInfo>,
    /// Presets per library (library name → entries).
    pub library_presets: HashMap<String, Vec<PresetInfo>>,
    /// Base URL for the library.
    pub base_url: String,
    /// Search query for filtering presets.
    pub search_query: String,
    /// Selected category filter (None = all).
    pub category_filter: Option<String>,
    /// Whether the background refresh has been triggered this session.
    pub refresh_started: bool,
    /// Status message for the UI.
    pub status_message: String,
}

impl PresetManager {
    pub fn new() -> Self {
        Self {
            libraries: Vec::new(),
            library_presets: HashMap::new(),
            base_url: super::loader::DEFAULT_LIBRARY_URL.to_string(),
            search_query: String::new(),
            category_filter: None,
            refresh_started: false,
            status_message: String::new(),
        }
    }

    /// Start the initial background refresh of library indexes.
    ///
    /// Called once at plugin initialization. Loads from cache immediately,
    /// then spawns a background HTTP fetch to refresh data.
    pub fn start_background_refresh(manager: Arc<Mutex<Self>>) {
        // Check if already started
        {
            let mut mgr = manager.lock().unwrap();
            if mgr.refresh_started {
                return;
            }
            mgr.refresh_started = true;
            mgr.status_message = "Loading index…".to_string();
        }

        // Load from cache first for instant display
        Self::load_from_cache(&manager);

        // Spawn background thread with tokio runtime for HTTP fetch
        let manager_clone = manager.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();

            match rt {
                Ok(rt) => {
                    rt.block_on(Self::fetch_root_index_async(manager_clone));
                }
                Err(e) => {
                    if let Ok(mut mgr) = manager_clone.lock() {
                        mgr.status_message = format!("Runtime error: {}", e);
                    }
                }
            }
        });
    }

    /// Load cached data for immediate display.
    fn load_from_cache(manager: &Arc<Mutex<Self>>) {
        let cache = DiskCache::new();
        let _ = cache.ensure_dirs();

        if let Some(cached) = cache.read_root_index() {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&cached) {
                let mut mgr = manager.lock().unwrap();
                mgr.parse_root_index(&root);

                // Load cached library indexes for any known libraries
                let lib_names: Vec<String> = mgr
                    .libraries
                    .iter()
                    .map(|l| l.name.clone())
                    .collect();
                drop(mgr);

                for name in &lib_names {
                    if let Some(cached_lib) = cache.read_library_index(name) {
                        if let Ok(lib_index) =
                            serde_json::from_str::<serde_json::Value>(&cached_lib)
                        {
                            let mut mgr = manager.lock().unwrap();
                            mgr.parse_library_index(name, &lib_index);
                            if let Some(lib) =
                                mgr.libraries.iter_mut().find(|l| &l.name == name)
                            {
                                lib.status = LibraryStatus::Loaded;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Async fetch of the root index from the network.
    async fn fetch_root_index_async(manager: Arc<Mutex<Self>>) {
        let base_url = {
            let mgr = manager.lock().unwrap();
            mgr.base_url.clone()
        };
        let loader = PresetLoader::new().with_base_url(base_url);

        match loader.fetch_root_index().await {
            Ok(root) => {
                let mut mgr = manager.lock().unwrap();
                mgr.parse_root_index(&root);
                mgr.status_message = format!("{} libraries loaded", mgr.libraries.len());
            }
            Err(e) => {
                let mut mgr = manager.lock().unwrap();
                if mgr.libraries.is_empty() {
                    mgr.status_message = format!("⚠ {}", e);
                }
                // If we have cached data, keep using it
            }
        }
    }

    /// Fetch a library index in the background (called when user expands a folder).
    pub fn fetch_library_index(manager: Arc<Mutex<Self>>, library_name: String) {
        // Mark as loading
        {
            let mut mgr = manager.lock().unwrap();
            if let Some(lib) = mgr.libraries.iter_mut().find(|l| l.name == library_name) {
                if lib.status == LibraryStatus::Loaded {
                    return; // Already loaded
                }
                lib.status = LibraryStatus::Loading;
            }
            mgr.status_message = format!("Loading {}…", library_name);
        }

        let manager_clone = manager.clone();
        let lib_name = library_name.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();

            if let Ok(rt) = rt {
                rt.block_on(async {
                    let (base_url, lib_path) = {
                        let mgr = manager_clone.lock().unwrap();
                        let path = mgr
                            .libraries
                            .iter()
                            .find(|l| l.name == lib_name)
                            .map(|l| l.path.clone());
                        (mgr.base_url.clone(), path)
                    };

                    if let Some(path) = lib_path {
                        let loader = PresetLoader::new().with_base_url(base_url);

                        match loader.fetch_library_index_by_path(&path, &lib_name).await {
                            Ok(lib_index) => {
                                let mut mgr = manager_clone.lock().unwrap();
                                mgr.parse_library_index(&lib_name, &lib_index);
                                if let Some(lib) =
                                    mgr.libraries.iter_mut().find(|l| l.name == lib_name)
                                {
                                    lib.status = LibraryStatus::Loaded;
                                }
                                let count = mgr
                                    .library_presets
                                    .get(&lib_name)
                                    .map(|p| p.len())
                                    .unwrap_or(0);
                                mgr.status_message =
                                    format!("{}: {} presets", lib_name, count);
                            }
                            Err(e) => {
                                let mut mgr = manager_clone.lock().unwrap();
                                if let Some(lib) =
                                    mgr.libraries.iter_mut().find(|l| l.name == lib_name)
                                {
                                    lib.status = LibraryStatus::Error(e.clone());
                                }
                                mgr.status_message = format!("⚠ {}", e);
                            }
                        }
                    }
                });
            }
        });
    }

    /// Parse the root index JSON and populate the library list.
    ///
    /// The root index format is:
    /// ```json
    /// {
    ///   "format": "songwalker-index",
    ///   "version": 1,
    ///   "name": "...",
    ///   "entries": [
    ///     { "type": "index", "name": "...", "path": "...", "presetCount": N },
    ///     ...
    ///   ]
    /// }
    /// ```
    fn parse_root_index(&mut self, root: &serde_json::Value) {
        let entries = match root.get("entries").and_then(|e| e.as_array()) {
            Some(arr) => arr,
            None => return,
        };

        self.libraries.clear();

        for entry in entries {
            let entry_type = entry
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("");

            if entry_type != "index" {
                continue;
            }

            let name = entry
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let path = entry
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            let description = entry
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let preset_count = entry
                .get("presetCount")
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as usize;

            // Preserve loaded status if library was already loaded
            let status = if self.library_presets.contains_key(&name) {
                LibraryStatus::Loaded
            } else {
                LibraryStatus::NotLoaded
            };

            self.libraries.push(LibraryInfo {
                name,
                path,
                description,
                preset_count,
                status,
                expanded: false,
            });
        }
    }

    /// Parse a library's index JSON and populate its preset list.
    ///
    /// Library index format matches root index but entries are type "preset":
    /// ```json
    /// {
    ///   "format": "songwalker-index",
    ///   "entries": [
    ///     { "type": "preset", "name": "...", "path": "...", "category": "sampler", ... },
    ///     ...
    ///   ]
    /// }
    /// ```
    fn parse_library_index(&mut self, library_name: &str, index: &serde_json::Value) {
        let entries = match index.get("entries").and_then(|e| e.as_array()) {
            Some(arr) => arr,
            None => return,
        };

        let mut presets = Vec::new();

        for entry in entries {
            let entry_type = entry
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("");

            if entry_type != "preset" {
                continue;
            }

            let name = entry
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let path = entry
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            let category = entry
                .get("category")
                .and_then(|c| c.as_str())
                .unwrap_or("sampler")
                .to_string();
            let tags: Vec<String> = entry
                .get("tags")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let gm_program = entry
                .get("gmProgram")
                .and_then(|n| n.as_u64())
                .map(|n| n as u8);
            let zone_count = entry
                .get("zoneCount")
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as u32;

            presets.push(PresetInfo {
                name,
                path,
                category,
                tags,
                gm_program,
                zone_count,
            });
        }

        self.library_presets
            .insert(library_name.to_string(), presets);
    }

    /// Get all unique categories across loaded library presets.
    pub fn available_categories(&self) -> Vec<String> {
        let mut cats = std::collections::HashSet::new();
        for presets in self.library_presets.values() {
            for preset in presets {
                cats.insert(preset.category.clone());
            }
        }
        let mut sorted: Vec<String> = cats.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Get presets for a given library, filtered by current search/category.
    pub fn filtered_presets_for_library(&self, library_name: &str) -> Vec<&PresetInfo> {
        let query = self.search_query.to_lowercase();

        self.library_presets
            .get(library_name)
            .map(|presets| {
                presets
                    .iter()
                    .filter(|p| {
                        // Category filter
                        if let Some(ref cat) = self.category_filter {
                            if &p.category != cat {
                                return false;
                            }
                        }
                        // Search filter
                        if !query.is_empty() {
                            let name_lower = p.name.to_lowercase();
                            if !name_lower.contains(&query) {
                                return false;
                            }
                        }
                        true
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
