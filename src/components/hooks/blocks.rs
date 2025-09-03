use dioxus::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use crate::utilities::types::{Filters, ViewMode};
use crate::settings::{SortSetting, SortBy};

// Group 1: Filters & extension related signals -------------------------------------------------
pub struct FiltersBlock {
    pub filters: Signal<Filters>,
    pub ext_filters: Signal<BTreeSet<String>>,
    pub ext_enabled: Signal<BTreeMap<String,bool>>,
    pub excluded_dirs: Signal<BTreeSet<PathBuf>>,
    pub search_text: Signal<String>,
}

/// Creates filter-related signals and provides them via context (mirrors previous inline setup).
pub fn use_filters_block() -> FiltersBlock {
    let filters = use_signal(Filters::default);
    let ext_filters = use_signal(|| BTreeSet::<String>::new());
    let ext_enabled = use_signal(|| BTreeMap::<String,bool>::new());
    let excluded_dirs = use_signal(|| BTreeSet::<PathBuf>::new());
    let search_text = use_signal(|| String::new());

    // Provide contexts (consumed by filtering & other hooks)
    provide_context(filters.clone());
    provide_context(ext_enabled.clone());
    provide_context(ext_filters.clone());
    provide_context(excluded_dirs.clone());
    provide_context(search_text.clone());

    FiltersBlock { filters, ext_filters, ext_enabled, excluded_dirs, search_text }
}

// Group 2: Layout & view/state signals ----------------------------------------------------------
pub struct LayoutBlock {
    pub qa_collapsed: Signal<bool>,
    pub drives_collapsed: Signal<bool>,
    pub preview_collapsed: Signal<bool>,
    pub preview_width: Signal<u32>,
    pub left_width: Signal<u32>,
    pub resizing_left: Signal<Option<(i32,u32)>>,
    pub resizing_preview: Signal<Option<(i32,u32)>>,
    pub view_mode: Signal<ViewMode>,
    pub group_by_category: Signal<bool>,
    pub sort: Signal<SortSetting>,
    pub detail_column_widths: Signal<[f32;6]>,
    pub category_col_width: Signal<f32>,
    pub resizing_col: Signal<Option<(usize,i32,f32)>>,
    pub progress_expanded: Signal<bool>,
}

/// Initialize layout-related signals from persisted UiSettings (ui signal must already exist).
pub fn use_layout_block(ui: Signal<crate::settings::UiSettings>) -> LayoutBlock {
    let qa_collapsed = use_signal(|| ui.read().qa_collapsed);
    let drives_collapsed = use_signal(|| ui.read().drives_collapsed);
    let preview_collapsed = use_signal(|| ui.read().preview_collapsed);
    let preview_width = use_signal(|| ui.read().preview_width.max(240).min(800));
    let sort = use_signal(|| ui.read().sort.clone().unwrap_or(SortSetting { by: SortBy::Name, asc: true }));
    let resizing_preview = use_signal(|| None::<(i32,u32)>);
    let left_width = use_signal(|| ui.read().left_width.max(180).min(480));
    let resizing_left = use_signal(|| None::<(i32,u32)>);
    let view_mode = use_signal(|| match ui.read().view_mode.as_deref() { Some("icons") => ViewMode::Icons, _ => ViewMode::Details });
    let group_by_category = use_signal(|| ui.read().group_by_category);
    let detail_column_widths = use_signal(|| ui.read().detail_column_widths.unwrap_or([1.2, 2.0, 0.7, 0.9, 0.9, 0.6]));
    let category_col_width = use_signal(|| ui.read().category_col_width.unwrap_or(0.9));
    let resizing_col = use_signal(|| None::<(usize,i32,f32)>);
    let progress_expanded = use_signal(|| false);

    LayoutBlock { qa_collapsed, drives_collapsed, preview_collapsed, preview_width, left_width, resizing_left, resizing_preview, view_mode, group_by_category, sort, detail_column_widths, category_col_width, resizing_col, progress_expanded }
}

// Group 3: Scan & navigation signals -----------------------------------------------------------
pub struct ScanBlock {
    pub results: Signal<crate::utilities::types::ScanResults>,
    pub error: Signal<Option<String>>,
    pub scanning: Signal<bool>,
    pub scan_generation: Signal<u64>,
    pub initialized: Signal<bool>,
    pub dir_items: Signal<Vec<crate::utilities::types::DirItem>>,
    pub progress: Signal<Option<(usize,usize)>>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_finished: Signal<Option<std::time::Instant>>,
    pub recursive_current: Signal<bool>,
    pub only_subdirs: Signal<bool>,
    pub nav_history: Signal<Vec<std::path::PathBuf>>,
    pub path_text: Signal<String>,
}

pub fn use_scan_block() -> ScanBlock {
    let results = use_signal(|| crate::utilities::types::ScanResults::default());
    let error = use_signal(|| None::<String>);
    let scanning = use_signal(|| false);
    let scan_generation = use_signal(|| 0u64);
    let initialized = use_signal(|| false);
    let dir_items = use_signal(|| Vec::<crate::utilities::types::DirItem>::new());
    let progress = use_signal(|| None::<(usize,usize)>);
    let scan_started = use_signal(|| None::<std::time::Instant>);
    let scan_finished = use_signal(|| None::<std::time::Instant>);
    let recursive_current = use_signal(|| false);
    let only_subdirs = use_signal(|| false);
    let nav_history = use_signal(|| Vec::<std::path::PathBuf>::new());
    let path_text = use_signal(|| String::new());

    // Provide commonly consumed contexts
    provide_context(results.clone());
    provide_context(progress.clone());
    provide_context(scanning.clone());
    provide_context(scan_generation.clone());
    provide_context(error.clone());
    provide_context(scan_started.clone());
    provide_context(scan_finished.clone());
    // Provide nav & path contexts if future hooks wish to consume them
    provide_context(path_text.clone());

    ScanBlock { results, error, scanning, scan_generation, initialized, dir_items, progress, scan_started, scan_finished, recursive_current, only_subdirs, nav_history, path_text }
}

// (Future) Group 4: AI / indexing block.
pub struct AiBlock {
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub ai_search_active: Signal<bool>,
    pub ai_search_results: Signal<Vec<crate::FileMetadata>>,
    pub ai_descriptions: Signal<std::collections::HashMap<String,String>>,
    pub ai_model_ready: Signal<bool>,
    pub ai_generating: Signal<bool>,
    pub selected_ai_meta: Signal<Option<crate::FileMetadata>>,
    pub index_queue_len: Signal<usize>,
    pub index_active: Signal<usize>,
    pub index_completed: Signal<usize>,
}

pub fn use_ai_block() -> AiBlock {
    let ai_search_engine = use_signal(|| None::<crate::ai::AISearchEngine>);
    let ai_search_active = use_signal(|| false);
    let ai_search_results = use_signal(|| Vec::<crate::FileMetadata>::new());
    let ai_descriptions = use_signal(|| std::collections::HashMap::<String,String>::new());
    let ai_model_ready = use_signal(|| false);
    let ai_generating = use_signal(|| false);
    let selected_ai_meta = use_signal(|| None::<crate::FileMetadata>);
    let index_queue_len = use_signal(|| 0usize);
    let index_active = use_signal(|| 0usize);
    let index_completed = use_signal(|| 0usize);

    // Provide contexts widely consumed by components
    provide_context(ai_search_engine.clone());
    provide_context(ai_search_active.clone());
    provide_context(ai_search_results.clone());
    provide_context(ai_descriptions.clone());
    provide_context(ai_model_ready.clone());
    provide_context(ai_generating.clone());
    provide_context(selected_ai_meta.clone());
    provide_context(index_queue_len.clone());
    provide_context(index_active.clone());
    provide_context(index_completed.clone());

    AiBlock { ai_search_engine, ai_search_active, ai_search_results, ai_descriptions, ai_model_ready, ai_generating, selected_ai_meta, index_queue_len, index_active, index_completed }
}
