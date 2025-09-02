#![allow(unused_imports)]
use crate::{database, utilities::{explorer::{default_pictures_root, list_dir_items}, scan::ScanMsg, types::{DirItem, Filters, ScanResults, ViewMode}}, Thumbnail};
use crate::settings::{load_settings, save_settings, SortBy, SortSetting};
use std::{collections::{BTreeMap, BTreeSet, HashMap, HashSet}, time::Duration};
use std::{path::{Path, PathBuf}, rc::Rc, cell::Cell};
use dioxus::{core::spawn_forever, desktop::use_window};
use crate::{DB, get_settings}; 
use dioxus::prelude::*;
use crate::components::{
    progress_overlay::ProgressOverlay,
    progress_overlay::ProgressOverlayProps,
    navbar::NewNavbar,
    results::ResultsView,
    preview::PreviewPane,
    debug_view::DebugView,
    sidebar::LeftSidebar,
};
// use dioxus::logger::tracing::{debug, error, info};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppView { Explorer, DebugDb }

pub const DEFAULT_JOYCAPTION_PATH: &str = r#"G:\Users\Owner\Desktop\llama-joycaption-beta-one-hf-llava"#;
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
pub const MAX_NEW_TOKENS: usize = 200;
pub const TEMPERATURE: f32 = 0.5;

// Helper: convert a FoundFile (basic scan result) into a minimal Thumbnail row for persistence
fn file_to_thumbnail(f: &crate::utilities::types::FoundFile) -> Option<crate::Thumbnail> {
    use chrono::Utc;
    let file_type = f.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
    let ft_string = if let Some(ext) = file_type.clone() {
        if crate::utilities::types::IMAGE_EXTS.iter().any(|e| *e == ext) { "image".to_string() }
        else if crate::utilities::types::VIDEO_EXTS.iter().any(|e| *e == ext) { "video".to_string() }
        else { ext }
    } else { "other".into() };
    let md = std::fs::metadata(&f.path).ok();
    let modified = md.as_ref().and_then(|m| m.modified().ok()).map(|st| chrono::DateTime::<chrono::Utc>::from(st));
    Some(crate::Thumbnail {
        db_created: Utc::now().into(),
        path: f.path.display().to_string(),
        filename: f.path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
        file_type: ft_string,
        size: f.size.unwrap_or(0),
        description: None,
        caption: None,
        tags: Vec::new(),
        category: None,
        embedding: None,
        thumbnail_b64: f.thumb_data.clone(), // may be None; raw data URL string stored earlier version
        modified: if let Some(date) = modified { Some(date.into()) } else { Some(Utc::now().into()) },
        hash: None,
    })
}

pub fn app() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link {
            href: "https://fonts.googleapis.com/icon?family=Material+Icons",
            rel: "stylesheet",
        }
        App {}
    }
}

#[component]
fn App() -> Element {
    let _win = use_window();
    use_future(move || async move {
        match database::new().await {
            Ok(_) => log::info!("Initialized database"),
            Err(e) => log::error!("Error initializing database: {e:?}"),
        };
    });
    let mut filters = use_signal(Filters::default);
    let results = use_signal(|| ScanResults::default());
    let error = use_signal(|| None::<String>);
    let scanning = use_signal(|| false);
    // Track current scan generation (global channel lives in scan.rs)
    let scan_generation = use_signal(|| 0u64);
    let mut initialized = use_signal(|| false);
    let dir_items = use_signal(|| Vec::<DirItem>::new());
    let progress = use_signal(|| None::<(usize, usize)>);
    // Bulk AI description generation progress
    let bulk_progress = use_signal(|| (0usize,0usize));
    let bulk_generating = use_signal(|| false);
    let mut ui = use_signal(load_settings);
    let qa_collapsed = use_signal(|| ui.read().qa_collapsed);
    let drives_collapsed = use_signal(|| ui.read().drives_collapsed);
    let preview_collapsed = use_signal(|| ui.read().preview_collapsed);
    let mut preview_width = use_signal(|| ui.read().preview_width.max(240).min(800));
    let sort = use_signal(|| ui.read().sort.clone().unwrap_or(SortSetting { by: SortBy::Name, asc: true }));
    let resizing_preview = use_signal(|| None::<(i32,u32)>);
    let mut left_width = use_signal(|| ui.read().left_width.max(180).min(480));
    let mut resizing_left = use_signal(|| None::<(i32,u32)>);
    let view_mode = use_signal(|| match ui.read().view_mode.as_deref() { Some("icons") => ViewMode::Icons, _ => ViewMode::Details });
    let selected_path = use_signal(|| None::<PathBuf>);
    let selected_paths = use_signal(|| HashSet::<PathBuf>::new());
    let mut path_text = use_signal(|| String::new());
    let mut recursive_current = use_signal(|| false);
    let mut only_subdirs = use_signal(|| false);
    let mut scan_started = use_signal(|| None::<std::time::Instant>);
    let mut scan_finished = use_signal(|| None::<std::time::Instant>);
    let ext_filters = use_signal(|| BTreeSet::<String>::new());
    let ext_enabled = use_signal(|| BTreeMap::<String,bool>::new());
    let excluded_dirs = use_signal(|| BTreeSet::<PathBuf>::new());
    let search_text = use_signal(|| String::new());
    let ai_search_results = use_signal(|| Vec::<crate::FileMetadata>::new());
    let ai_search_engine = use_signal(|| None::<crate::ai::AISearchEngine>);
    // Provide shared signals (after creation of signals they depend on)
    provide_context(bulk_progress.clone());
    provide_context(bulk_generating.clone());
    provide_context(ai_search_engine.clone());
    // let mut ui = use_signal(load_settings);
    provide_context(ui.clone());
    let ai_search_active = use_signal(|| false);
    let ai_descriptions = use_signal(|| HashMap::<String,String>::new());
    let indexed_paths = use_signal(|| HashSet::<String>::new());
    let mut ai_model_ready = use_signal(|| false);
    // (ai_pending_desc reserved for future streaming status)
    let ai_generating = use_signal(|| false);
    let selected_ai_meta = use_signal(|| None::<crate::FileMetadata>);
    let app_view = use_signal(|| AppView::Explorer);
    let debug_thumb_rows = use_signal(|| Vec::<crate::Thumbnail>::new());
    let debug_doc_snips = use_signal(|| Vec::<crate::DebugDocumentSnippet>::new());
    let debug_loaded_at = use_signal(|| None::<std::time::Instant>);
    let group_by_category = use_signal(|| ui.read().group_by_category);
    // Global cache of DB thumbnail rows keyed by absolute path
    let all_cached = use_signal(|| HashMap::<String, Thumbnail>::new());
    // Navigation history (stack of previous roots for Back button)
    let nav_history = use_signal(|| Vec::<PathBuf>::new());
    // Detail view column widths: [Name, Path, Size, Modified, Created, Type] (+ separate Category column width)
    let mut detail_column_widths = use_signal(|| ui.read().detail_column_widths.unwrap_or([1.2, 2.0, 0.7, 0.9, 0.9, 0.6]));
    let category_col_width = use_signal(|| ui.read().category_col_width.unwrap_or(0.9));
    // Active column resize state: (col_index, start_x, start_width)
    let mut resizing_col = use_signal(|| None::<(usize, i32, f32)>);
    let ai_init_started_flag = Rc::new(Cell::new(false));
    // Progress overlay expansion state
    let progress_expanded = use_signal(|| false);
    let _ai_pending_refreshed_flag = Rc::new(Cell::new(false));
    // Indexing progress signals (populated from engine atomics)
    let index_queue_len = use_signal(|| 0usize);
    let index_active = use_signal(|| 0usize);
    let index_completed = use_signal(|| 0usize);
    // Dedicated long-lived task draining scan channel using use_future (lifetime tied to component, avoids scope warnings)
    {
    let mut results_sig = results.clone();
    let all_cached_sig_for_scan = all_cached.clone();
        let mut progress_sig = progress.clone();
        let mut error_sig = error.clone();
        let mut scanning_sig = scanning.clone();
        let mut scan_finished_sig = scan_finished.clone();
        let mut ext_filters_sig = ext_filters.clone();
        let mut ext_enabled_sig = ext_enabled.clone();
        let scan_generation_sig = scan_generation.clone();
        let _scan_drain_task = use_future(move || async move {
            use tokio::time::{sleep, Duration};
            // Batching buffer for recursive scan persistence (thumbnails every 100 files)
            let mut pending_thumb_rows: Vec<crate::Thumbnail> = Vec::with_capacity(120);
            const BATCH_THRESHOLD: usize = 100;
            loop {
                // drain scan channel and update progress each loop
                let active_gen = *scan_generation_sig.read();
                let rx = crate::utilities::scan::global_scan_receiver();
                let mut processed = 0usize;
                let mut latest_progress: Option<(usize,usize)> = None;
                {
                    let mut new_items: Vec<crate::utilities::types::FoundFile> = Vec::new();
                    let mut thumb_updates: Vec<(std::path::PathBuf,String)> = Vec::new();
                    while let Ok(env) = rx.try_recv() {
                        if env.scan_id != active_gen { continue; }
                        match env.msg {
                            ScanMsg::Found(item) => {
                                if let Some(ext) = item.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                                    if !ext_filters_sig.read().contains(&ext) { ext_filters_sig.write().insert(ext.clone()); ext_enabled_sig.write().entry(ext.clone()).or_insert(true); }
                                }
                                let path_str = item.path.display().to_string();
                                let cached = all_cached_sig_for_scan.read().contains_key(&path_str);
                                if !cached {
                                    if let Some(row) = file_to_thumbnail(&item) { pending_thumb_rows.push(row); }
                                    new_items.push(item);
                                }
                            }
                            ScanMsg::FoundBatch(batch) => {
                                for item in batch.into_iter() {
                                    if let Some(ext) = item.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                                        if !ext_filters_sig.read().contains(&ext) { ext_filters_sig.write().insert(ext.clone()); ext_enabled_sig.write().entry(ext.clone()).or_insert(true); }
                                    }
                                    let path_str = item.path.display().to_string();
                                    let cached = all_cached_sig_for_scan.read().contains_key(&path_str);
                                    if !cached {
                                        if let Some(row) = file_to_thumbnail(&item) { pending_thumb_rows.push(row); }
                                        new_items.push(item);
                                    }
                                    if pending_thumb_rows.len() >= BATCH_THRESHOLD {
                                        let batch = std::mem::take(&mut pending_thumb_rows);
                                        spawn(async move {
                                            if let Err(e) = crate::database::save_thumbnail_batch(batch).await { log::warn!("scan batch persistence failed: {e}"); }
                                        });
                                    }
                                }
                            }
                            ScanMsg::UpdateThumb { path, thumb } => { thumb_updates.push((path, thumb)); }
                            ScanMsg::Progress { scanned, total } => { latest_progress = Some((scanned,total)); }
                            ScanMsg::Error(e) => { error_sig.set(Some(e)); scanning_sig.set(false); }
                            ScanMsg::Done => {
                                // Guard: only mark finished if this Done corresponds to current generation
                                if env.scan_id == active_gen { scanning_sig.set(false); scan_finished_sig.set(Some(std::time::Instant::now())); }
                                // Flush any remaining pending rows (< threshold)
                                if !pending_thumb_rows.is_empty() {
                                    let batch = std::mem::take(&mut pending_thumb_rows);
                                    spawn(async move {
                                        if let Err(e) = crate::database::save_thumbnail_batch(batch).await { log::warn!("final scan batch persistence failed: {e}"); }
                                    });
                                }
                            }
                        }
                        processed += 1; if processed > 1200 { break; }
                    }
                    if !new_items.is_empty() {
                        let mut w = results_sig.write();
                        w.items.extend(new_items.into_iter());
                    }
                    if !thumb_updates.is_empty() {
                        let mut w = results_sig.write();
                        for (p,t) in thumb_updates { if let Some(f) = w.items.iter_mut().find(|f| f.path == p) { f.thumb_data = Some(t); } }
                    }
                }
                if let Some(p) = latest_progress.take() { progress_sig.set(Some(p)); }
                sleep(Duration::from_millis(if processed > 0 { 20 } else { 80 })).await;
            }
        });
        // keep handle alive
    // keep handle captured in closure scope; no need to call value() (method not present in current dioxus)
    }

    // First-time init moved into an unconditional use_effect to avoid conditional hook usage
    {
        let mut initialized_sig = initialized.clone();
        let mut filters_sig = filters.clone();
        let mut path_text_sig = path_text.clone();
        let ui_sig = ui.clone();
        let all_cached_sig = all_cached.clone();
        let mut scan_started_sig = scan_started.clone();
        let mut scan_finished_sig = scan_finished.clone();
        use_effect(move || {
            if *initialized_sig.read() { return; }
            // Root default
            if let Some(pics) = default_pictures_root() {
                let mut f = filters_sig.write();
                path_text_sig.set(pics.display().to_string());
                f.root = pics;
            }
            // Asynchronous preload of thumbnail/metadata cache (spawn so effect returns immediately)
            {
                let mut all_cached_sig2 = all_cached_sig.clone();
                let mut results_sig_pre = results.clone();
                let root_snapshot = filters_sig.read().root.clone();
                spawn(async move {
                    if let Ok(map) = crate::database::load_thumb_lookup().await {
                        let count = map.len();
                        all_cached_sig2.set(map);
                        log::info!("[startup] loaded {count} cached thumbnail rows into all_cached");
                        // Pre-populate results list with cached entries under the current root
                        if root_snapshot.exists() {
                            let root_str = root_snapshot.display().to_string();
                            let cache = all_cached_sig2.read().clone();
                            let mut preload: Vec<crate::utilities::types::FoundFile> = Vec::new();
                            for row in cache.values() {
                                if row.path.starts_with(&root_str) {
                                    let pb = std::path::PathBuf::from(&row.path);
                                    if pb.is_file() {
                                        preload.push(crate::utilities::types::FoundFile {
                                            path: pb,
                                            size: Some(row.size),
                                            modified: None,
                                            created: None,
                                            kind: match row.file_type.as_str() { "image" => crate::utilities::types::MediaKind::Image, "video" => crate::utilities::types::MediaKind::Video, _ => crate::utilities::types::MediaKind::Other },
                                            thumb_data: row.thumbnail_b64.clone(),
                                        });
                                    }
                                }
                            }
                            if !preload.is_empty() { log::info!("[startup] pre-populated {} cached items", preload.len()); results_sig_pre.write().items.extend(preload); }
                        }
                    } else {
                        log::warn!("[startup] failed to load cached thumbnails");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        if let Ok(map) = crate::database::load_thumb_lookup().await {
                            let count = map.len();
                            all_cached_sig2.set(map);
                            log::info!("[startup] loaded {count} cached thumbnail rows into all_cached");
                        } else {
                            log::error!("Still no database");
                        }
                    }
                });
            }
            // Hydrate filters from persisted settings snapshot
            {
                let s = ui_sig.read().clone();
                let mut f = filters_sig.write();
                if let Some(a) = s.filter_modified_after.clone() { f.modified_after = Some(a); }
                if let Some(b) = s.filter_modified_before.clone() { f.modified_before = Some(b); }
                if let Some(cats) = s.filter_category_multi.clone() { f.category_filters = cats.into_iter().collect(); }
                f.only_with_thumb = s.filter_only_with_thumb;
                f.only_with_description = s.filter_only_with_description;
            }
            initialized_sig.set(true);
            scan_started_sig.set(Some(std::time::Instant::now()));
            scan_finished_sig.set(None);
            // initial scan trigger handled reactively elsewhere
        });
    }

    // Async hydration of last_root after DB settings load (non-blocking, may rescan if different)
    {
        let mut filters_sig = filters.clone();
        let mut ui_sig = ui.clone();
        let scan_generation_sig = scan_generation.clone();
        let scanning_sig = scanning.clone();
        let results_sig = results.clone();
        let dir_items_sig = dir_items.clone();
        let progress_sig = progress.clone();
        let mut scan_started_sig = scan_started.clone();
        let mut scan_finished_sig = scan_finished.clone();
        use_future(move || async move {
            if let Ok(settings) = get_settings().await {
                let last_root = settings.last_root.clone();
                ui_sig.set(settings.clone());
                if let Some(root_str) = last_root {
                    let new_root = std::path::PathBuf::from(&root_str);
                    if new_root.is_dir() && new_root != filters_sig.read().root {
                        {
                            let mut f = filters_sig.write();
                            f.root = new_root.clone();
                        }
                        scan_started_sig.set(Some(std::time::Instant::now()));
                        scan_finished_sig.set(None);
                        // legacy begin_scan removed: changing filters/root triggers reactive scan resource
                    }
                }
            }
        });
    }

    // Derived filtered items (extensions, search, exclusions, thumbs-only, description-only, category filters)
    let filtered_items = use_memo(move || {
        let t0 = std::time::Instant::now();
        let enabled = ext_enabled.read().clone();
        let excluded = excluded_dirs.read().clone();
        let needle = search_text.read().to_ascii_lowercase();
        let only_with_thumb = filters.read().only_with_thumb;
        let only_with_desc = filters.read().only_with_description;
        let category_filter_single = filters.read().category_filter.clone(); // legacy single-select (to be removed)
        let category_filters_multi = filters.read().category_filters.clone();
        let desc_map = ai_descriptions.read().clone();
    // categories stored on cached Thumbnail rows
    let categories_cache = all_cached.read().clone();
        let total_before = results.read().items.len();
        let mut kept = 0usize;
        let mut rejected_thumb = 0usize;
        let mut rejected_ext = 0usize;
        let mut rejected_excluded_dir = 0usize;
        let mut rejected_search = 0usize;
        let mut rejected_desc = 0usize;
        let mut rejected_category = 0usize;
        let out: Vec<_> = results.read().items.iter().filter(|it| {
            if only_with_thumb && it.thumb_data.is_none() { return false; }
            if let Some(ext) = it.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                if let Some(flag) = enabled.get(&ext) { if !*flag { return false; } }
            }
            for ex in excluded.iter() { if it.path.starts_with(ex) { return false; } }
            if !needle.is_empty() {
                let name_lc = it.path.file_name().and_then(|f| f.to_str()).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
                if !name_lc.contains(&needle) { return false; }
            }
            if only_with_desc {
                let p = it.path.display().to_string();
                if !desc_map.contains_key(&p) { return false; }
            }
            // Multi-category union filter: if set non-empty require membership. Fallback to single filter while migrating.
            let p = it.path.display().to_string();
            let cat_opt = categories_cache.get(&p).and_then(|t| t.category.clone());
            if !category_filters_multi.is_empty() {
                // If user selected categories, require cat present and in set
                if let Some(cat_val) = cat_opt.as_ref() {
                    if !category_filters_multi.contains(cat_val) { return false; }
                } else { return false; }
            } else if let Some(ref legacy_needed) = category_filter_single { // legacy pathway
                if cat_opt.as_ref() != Some(legacy_needed) { return false; }
            }
            true
        }).cloned().inspect(|it| { kept += 1; }).collect();
        // recount rejections by running simple passes (cheap relative to scan)
        for it in results.read().items.iter() {
            if only_with_thumb && it.thumb_data.is_none() { rejected_thumb += 1; continue; }
            if let Some(ext) = it.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) { if let Some(flag) = enabled.get(&ext) { if !*flag { rejected_ext += 1; continue; } } }
            if excluded.iter().any(|ex| it.path.starts_with(ex)) { rejected_excluded_dir += 1; continue; }
            if !needle.is_empty() {
                let name_lc = it.path.file_name().and_then(|f| f.to_str()).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
                if !name_lc.contains(&needle) { rejected_search += 1; continue; }
            }
            if only_with_desc { let p = it.path.display().to_string(); if !desc_map.contains_key(&p) { rejected_desc += 1; continue; } }
            let p = it.path.display().to_string();
            let cat_opt = categories_cache.get(&p).and_then(|t| t.category.clone());
            if !category_filters_multi.is_empty() {
                if cat_opt.as_ref().map(|c| category_filters_multi.contains(c)).unwrap_or(false) { } else { rejected_category += 1; continue; }
            } else if let Some(ref legacy_needed) = category_filter_single { if cat_opt.as_ref() != Some(legacy_needed) { rejected_category += 1; continue; } }
        }
        log::info!("[filter] total_before={total_before} kept={kept} reject_thumb={rejected_thumb} reject_ext_flag={rejected_ext} reject_excluded_dir={rejected_excluded_dir} reject_search={rejected_search} reject_desc={rejected_desc} reject_category={rejected_category} elapsed_ms={}", t0.elapsed().as_millis());
        out
    });

    // Categories present (derive from cached metadata categories in all_cached)
    // Categories available limited to those present in CURRENT filtered_items (not entire cache)
    let categories_available = use_memo(move || {
        let mut set = std::collections::BTreeSet::<String>::new();
        let cache = all_cached.read().clone();
        for it in filtered_items.read().iter() {
            let p = it.path.display().to_string();
            if let Some(row) = cache.get(&p) { if let Some(c) = &row.category { if !c.is_empty() { set.insert(c.clone()); } } }
        }
        set
    });

    // Grouped items map (category -> Vec<FoundFile>) if grouping enabled
    let grouped_items = use_memo(move || {
        if !*group_by_category.read() { None } else {
            let mut map: std::collections::BTreeMap<String, Vec<crate::utilities::types::FoundFile>> = std::collections::BTreeMap::new();
            for it in filtered_items.read().iter() {
                let p = it.path.display().to_string();
                let cat = all_cached.read().get(&p).and_then(|t| t.category.clone()).unwrap_or_else(|| "Uncategorized".into());
                map.entry(cat).or_default().push(it.clone());
            }
            Some(map)
        }
    });

    // Persist settings when certain signals change
    {
        let mut ui_sig = ui.clone();
        let ext_enabled_sig = ext_enabled.clone();
        let excluded_dirs_sig = excluded_dirs.clone();
        let filters_sig = filters.clone();
        use_effect(move || {
            let mut settings = ui_sig.write();
            settings.ext_enabled = Some(ext_enabled_sig.read().iter().map(|(k,v)| (k.clone(), *v)).collect());
            settings.excluded_dirs = Some(excluded_dirs_sig.read().iter().map(|p| p.display().to_string()).collect());
            // Write filter persistence fields
            let f = filters_sig.read();
            settings.filter_modified_after = f.modified_after.clone();
            settings.filter_modified_before = f.modified_before.clone();
            settings.filter_category_multi = if f.category_filters.is_empty() { None } else { Some(f.category_filters.iter().cloned().collect()) };
            settings.filter_only_with_thumb = f.only_with_thumb;
            settings.filter_only_with_description = f.only_with_description;
            save_settings(&settings);
        });
    }

    // Initialize AI search engine once
    {
        let ai_engine_sig = ai_search_engine.clone();
        let indexed_sig = indexed_paths.clone();
        let ai_desc_sig = ai_descriptions.clone();
        use_effect(move || {
            if ai_init_started_flag.get() || ai_engine_sig.read().is_some() { return; }
            ai_init_started_flag.set(true);
            let mut ai_engine_sig = ai_engine_sig.clone();
            let mut indexed_sig = indexed_sig.clone();
            let mut ai_desc_sig = ai_desc_sig.clone();
            // bring in all_cached signal to pre-populate categories (and future hash/thumb cache) from DB
            let mut all_cached_sig = all_cached.clone();
            spawn(async move {
                let engine = crate::ai::AISearchEngine::new();
                engine.ensure_index_worker().await; // start indexing queue worker
                let loaded = engine.load_cached().await;
                log::info!("AI Search Engine initialized (cached {} rows)", loaded);
                for p in engine.list_indexed_paths().await.iter() { indexed_sig.write().insert(p.clone()); }
                if let Ok(files) = engine.get_all_files().await {
                    use chrono::Utc;
                    for f in files.iter() {
                        if let Some(desc) = &f.description { ai_desc_sig.write().insert(f.path.clone(), desc.clone()); }
                        let mut cache = all_cached_sig.write();
                        if let Some(row) = cache.get_mut(&f.path) {
                            if row.hash.is_none() { row.hash = f.hash.clone(); }
                            if row.thumbnail_b64.is_none() { row.thumbnail_b64 = f.thumb_b64.clone().or(f.thumbnail_path.clone()); }
                            if row.category.is_none() { row.category = f.category.clone(); }
                            if row.description.is_none() { row.description = f.description.clone(); }
                            if row.caption.is_none() { row.caption = f.caption.clone(); }
                            if row.tags.is_empty() && !f.tags.is_empty() { row.tags = f.tags.clone(); }
                        } else {
                            cache.insert(f.path.clone(), Thumbnail {
                                db_created: Utc::now().into(),
                                path: f.path.clone(),
                                filename: f.filename.clone(),
                                file_type: f.file_type.clone(),
                                size: f.size,
                                description: f.description.clone(),
                                caption: f.caption.clone(),
                                tags: f.tags.clone(),
                                category: f.category.clone(),
                                embedding: f.embedding.clone(),
                                thumbnail_b64: f.thumb_b64.clone().or(f.thumbnail_path.clone()),
                                modified: Some(Utc::now().into()),
                                hash: f.hash.clone(),
                            });
                        }
                    }
                }
                let engine_clone = engine.clone();
                spawn(async move { if let Err(e) = engine_clone.ensure_vision_model().await { log::warn!("Vision model warm-up failed: {}", e); } else { ai_model_ready.set(true); } });
                ai_engine_sig.set(Some(engine));
            });
        });
    }

    // (Removed automatic enrichment effect – manual only)

    // Auto indexing preference (persisted in settings; default false)
    let auto_indexing = use_signal(|| ui.read().auto_indexing);
    {
        let mut ui_sig = ui.clone();
        let auto_idx = auto_indexing.clone();
        use_effect(move || {
            let mut settings = ui_sig.write();
            settings.auto_indexing = *auto_idx.read();
            save_settings(&settings);
        });
    }
    // (Removed automatic indexing effect – manual only)

    // Poll engine indexing atomics periodically (lifetime bound to component) using use_future
    {
        let ai_engine_sig = ai_search_engine.clone();
        let mut q_sig = index_queue_len.clone();
        let mut a_sig = index_active.clone();
        let mut c_sig = index_completed.clone();
        let _index_poll = use_future(move || async move {
            use tokio::time::{sleep, Duration};
            loop {
                if let Some(engine) = ai_engine_sig.read().as_ref() {
                    q_sig.set(engine.index_queue_len.load(std::sync::atomic::Ordering::Relaxed));
                    a_sig.set(engine.index_active.load(std::sync::atomic::Ordering::Relaxed));
                    c_sig.set(engine.index_completed.load(std::sync::atomic::Ordering::Relaxed));
                }
                sleep(Duration::from_millis(100)).await;
            }
        });
    // keep polling task alive for component lifetime
    }

    // Load metadata for selected file
    {
        let ai_engine_sig = ai_search_engine.clone();
        let selected_sig = selected_path.clone();
        let mut sel_meta_sig = selected_ai_meta.clone();
        use_effect(move || {
            if let (Some(engine), Some(p)) = (ai_engine_sig.read().as_ref(), selected_sig.read().clone()) {
                let path_str = p.display().to_string();
                let engine_clone = engine.clone();
                let mut sel_meta_sig2 = sel_meta_sig.clone();
                spawn(async move { let m = engine_clone.get_file_metadata(&path_str).await; sel_meta_sig2.set(m); });
            } else { sel_meta_sig.set(None); }
        });
    }

    rsx! {
        div {
            class: "h-screen overflow-auto",
            style: if resizing_col.read().is_some() { "cursor:col-resize;position:relative" } else { "" },
            onmousemove: move |evt| {
                if let Some((start_x, start_width)) = resizing_left.read().clone() {
                    let delta = evt.client_coordinates().x as i32 - start_x;
                    let new_w = (start_width as i32 + delta).max(180).min(480) as u32;
                    left_width.set(new_w);
                }
                if let Some((start_x, start_width)) = resizing_preview.read().clone() {
                    let delta = start_x - evt.client_coordinates().x as i32;
                    let new_w = (start_width as i32 + delta).max(240).min(800) as u32;
                    preview_width.set(new_w);
                }
                if let Some((col_idx, start_x, start_w)) = resizing_col.read().clone() {
                    let x_now = evt.client_coordinates().x as i32;
                    let dx = x_now - start_x;
                    let mut widths = detail_column_widths.read().clone();
                    let new_w = (start_w + (dx as f32 * 0.15)).clamp(0.25, 18.0);
                    if col_idx < widths.len() {
                        widths[col_idx] = new_w;
                        detail_column_widths.set(widths);
                    }
                    log::debug!(
                        "[resize-global] col={col_idx} start={start_w:.2} dx={dx} -> {new_w:.2} widths={:?}",
                        widths
                    );
                }
            },
            onmouseup: move |_| {
                if resizing_left.read().is_some() {
                    resizing_left.set(None);
                    let mut s = ui.write();
                    s.left_width = *left_width.read();
                    save_settings(&s);
                }
                if resizing_col.read().is_some() {
                    let mut widths = detail_column_widths.read().clone();
                    normalize_detail_widths(&mut widths);
                    detail_column_widths.set(widths);
                    let mut s = ui.write();
                    s.detail_column_widths = Some(*detail_column_widths.read());
                    s.category_col_width = Some(*category_col_width.read());
                    save_settings(&s);
                    resizing_col.set(None);
                }
            },
            NewNavbar {
                ui,
                view_mode,
                preview_collapsed,
                qa_collapsed,
                drives_collapsed,
                results,
                app_view,
                error,
                filters,
                scan_generation,
                scanning,
                dir_items,
                progress,
                ai_search_engine,
                selected_paths,
                auto_indexing,
                search_text,
                ai_search_results,
                ai_search_active,
                group_by_category,
                selected_path,
                nav_history,
                recursive_current,
                only_subdirs,
                scan_started,
                scan_finished,
                ext_filters,
                ext_enabled,
                excluded_dirs,
                bulk_progress,
                bulk_generating,
                categories_available,
            }
            if ui.read().show_progress_overlay && (progress.read().is_some() || scanning.read().clone() || *bulk_generating.read()
                || bulk_progress.read().1 > 0)
            {
                {
                    let results_sig = results.clone();
                    let mut selected_paths_sig = selected_paths.clone();
                    let mut filters_sig = filters.clone();
                    let mut sort_sig = sort.clone();
                    let mut ui_sig = ui.clone();
                    rsx! {
                        ProgressOverlay {
                            progress,
                            scanning,
                            recursive_current,
                            scan_started,
                            scan_finished,
                            results,
                            bulk_progress,
                            bulk_generating,
                            show_expanded: progress_expanded,
                            on_select_all: EventHandler::new(move |_| {
                                let all: Vec<_> = results_sig
                                    .read()
                                    .items
                                    .iter()
                                    .map(|f| f.path.clone())
                                    .collect();
                                let mut cur = selected_paths_sig.read().clone();
                                for p in all {
                                    cur.insert(p);
                                }
                                selected_paths_sig.set(cur);
                            }),
                            on_filter_images: EventHandler::new(move |_| {
                                let mut f = filters_sig.read().clone();
                                f.only_with_thumb = false;
                                filters_sig.set(f);
                            }),
                            on_filter_videos: EventHandler::new(move |_| {
                                let mut f = filters_sig.read().clone();
                                f.only_with_thumb = false;
                                filters_sig.set(f);
                            }),
                            on_filter_all: EventHandler::new(move |_| {
                                let mut f = filters_sig.read().clone();
                                f.only_with_thumb = false;
                                filters_sig.set(f);
                            }),
                            on_sort_name: EventHandler::new(move |_| {
                                let setting = crate::settings::SortSetting {
                                    by: crate::settings::SortBy::Name,
                                    asc: true,
                                };
                                sort_sig.set(setting.clone());
                                let mut s = ui_sig.read().clone();
                                s.sort = Some(setting);
                                save_settings(&s);
                                ui_sig.set(s);
                            }),
                            on_sort_date: EventHandler::new(move |_| {
                                let setting = crate::settings::SortSetting {
                                    by: crate::settings::SortBy::Modified,
                                    asc: false,
                                };
                                sort_sig.set(setting.clone());
                                let mut s = ui_sig.read().clone();
                                s.sort = Some(setting);
                                save_settings(&s);
                                ui_sig.set(s);
                            }),
                            on_sort_size: EventHandler::new(move |_| {
                                let setting = crate::settings::SortSetting {
                                    by: crate::settings::SortBy::Size,
                                    asc: false,
                                };
                                sort_sig.set(setting.clone());
                                let mut s = ui_sig.read().clone();
                                s.sort = Some(setting);
                                save_settings(&s);
                                ui_sig.set(s);
                            }),
                        }
                    }
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "error",
                    code { "{err}" }
                }
            }
            if *app_view.read() == AppView::DebugDb {
                DebugView {
                    ai_search_engine,
                    ai_descriptions,
                    debug_thumb_rows,
                    debug_doc_snips,
                    debug_loaded_at,
                    selected_path,
                }
            } else {
                div {
                    class: "flex",
                    style: "height: calc(100vh - 56px - 48px);",
                    LeftSidebar {
                        filters,
                        qa_collapsed,
                        drives_collapsed,
                        ui,
                        left_width,
                        resizing_left,
                        path_text,
                        recursive_current,
                        only_subdirs,
                        scan_started,
                        scan_generation,
                        scanning,
                        results,
                        dir_items,
                        progress,
                    }
                    section {
                        class: "flex-1",
                        style: "overflow-y:auto; padding:10px;",
                        if *ai_search_active.read() && search_text.read().trim().is_empty() {
                            div { class: "text-center py-12 text-weak",
                                i { class: "material-icons text-6xl mb-4 opacity-50",
                                    "psychology"
                                }
                                h3 { class: "text-lg mb-2", "AI Smart Search" }
                                p { "Describe what you're looking for and let AI help you find it" }
                                p { class: "text-sm mt-2",
                                    "Try: \"photos of dogs\", \"documents about project planning\", \"videos from last vacation\""
                                }
                            }
                        } else if *ai_search_active.read() && !search_text.read().trim().is_empty()
                            && ai_search_results.read().is_empty()
                        {
                            div { class: "text-center py-12 text-weak",
                                i { class: "material-icons text-6xl mb-4 opacity-50",
                                    "search_off"
                                }
                                h3 { class: "text-lg mb-2", "No AI Results Found" }
                                p { "Try a different description or check if files are indexed" }
                            }
                        } else if !*ai_search_active.read() && results.read().items.is_empty() {
                            section {
                                class: "folder-list",
                                style: "display:flex; flex-direction:column; gap:4px;",
                                for d in dir_items.read().iter() {
                                    {
                                        let name = d.path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                                        let path = d.path.clone();
                                        folder_entry(
                                            name,
                                            path,
                                            filters,
                                            path_text,
                                            recursive_current,
                                            only_subdirs,
                                            scan_started,
                                            scan_generation,
                                            scanning,
                                            results,
                                            dir_items,
                                            progress,
                                            nav_history,
                                            ui,
                                        )
                                    }
                                }
                            }
                            p { class: "empty",
                                {
                                    if scanning.read().clone() {
                                        match progress.read().clone() {
                                            Some((s, t)) => {
                                                if t > 0 {
                                                    format!(
                                                        "{}... {} / {}",
                                                        if *recursive_current.read() {
                                                            "Deep scanning"
                                                        } else {
                                                            "Scanning"
                                                        },
                                                        s,
                                                        t,
                                                    )
                                                } else {
                                                    format!(
                                                        "{}... {}",
                                                        if *recursive_current.read() {
                                                            "Deep scanning"
                                                        } else {
                                                            "Scanning"
                                                        },
                                                        s,
                                                    )
                                                }
                                            }
                                            None => {
                                                if *recursive_current.read() {
                                                    "Deep scanning...".into()
                                                } else {
                                                    "Scanning...".into()
                                                }
                                            }
                                        }
                                    } else if *only_subdirs.read() {
                                        "".into()
                                    } else {
                                        "No results - adjust filters.".into()
                                    }
                                }
                            }
                            if *only_subdirs.read() {
                                div { class: "mt-8 flex flex-col items-center gap-3 text-weak",
                                    span { class: "text-sm", "Folder contains only subfolders." }
                                    div { class: "flex gap-2",
                                        button {
                                            class: "btn px-3 py-1 text-xs bg-gradient-to-r from-cyan-500 to-fuchsia-600 text-white rounded shadow hover:brightness-110 active:translate-y-px transition",
                                            onclick: move |_| {
                                                only_subdirs.set(false);
                                                scan_started.set(Some(std::time::Instant::now()));
                                                recursive_current.set(false);
                                                // begin_scan removed: toggling only_subdirs triggers resource via filters/root
                                            },
                                            i { class: "material-icons mr-1 align-middle text-base",
                                                "play_arrow"
                                            }
                                            span { "Scan Anyway" }
                                        }
                                    }
                                }
                            }
                        } else {
                            ResultsView {
                                view_mode,
                                sort,
                                ui,
                                filtered_items: filtered_items.read().clone(),
                                group_by_category,
                                all_cached,
                                selected_path,
                                selected_paths,
                                ai_descriptions,
                                grouped_items: grouped_items.read().clone(),
                                ai_search_active,
                                ai_search_results,
                                detail_column_widths,
                                category_col_width,
                                resizing_col,
                                filters,
                                categories_available,
                                ext_filters,
                                ext_enabled,
                            }
                        }
                    }
                    PreviewPane {
                        ui,
                        preview_collapsed,
                        preview_width,
                        resizing_preview,
                        selected_path,
                        results,
                        ai_search_active,
                        ai_search_results,
                        ai_descriptions,
                        selected_ai_meta,
                        ai_search_engine,
                        ai_model_ready,
                        ai_generating,
                    }
                }
            }
        }
    }
}

fn folder_entry(name: String, path: PathBuf,
    mut filters: Signal<Filters>,
    mut path_text: Signal<String>,
    mut recursive_current: Signal<bool>,
    mut only_subdirs: Signal<bool>,
    mut scan_started: Signal<Option<std::time::Instant>>,
    scan_generation: Signal<u64>,
    scanning: Signal<bool>,
    mut results: Signal<ScanResults>,
    dir_items: Signal<Vec<DirItem>>,
    progress: Signal<Option<(usize,usize)>>,
    mut nav_history: Signal<Vec<PathBuf>>,
    mut ui: Signal<crate::settings::UiSettings>,
) -> Element {
    rsx! {
        div {
            key: "{name}",
            class: "flex items-center gap-3 bg-panel border border-stroke rounded-md px-3 py-2 btn",
            style: "min-height:40px;",
            onclick: move |_| {
                let current_root = filters.read().root.clone();
                if current_root != path {
                    nav_history.write().push(current_root);
                }
                let new_root = path.clone();
                {
                    let mut f = filters.write();
                    f.root = new_root.clone();
                }
                // Persist last_root
                {
                    let mut s = ui.write();
                    s.last_root = Some(new_root.display().to_string());
                    save_settings(&s);
                }
                path_text.set(new_root.display().to_string());
                recursive_current.set(false);
                if shallow_should_scan(&new_root) {
                    only_subdirs.set(false);
                    scan_started.set(Some(std::time::Instant::now()));
                    // persist last_root on navigation
                    {
                        let mut s = ui.write();
                        s.last_root = Some(new_root.display().to_string());
                        save_settings(&s);
                    }
                    // begin_scan removed: folder_entry navigation triggers reactive scan
                } else {
                    only_subdirs.set(true);
                    let mut dir_items_sig = dir_items.clone();
                    let nr_async = new_root.clone();
                    // persist last_root even when not scanning (only subdirs)
                    {
                        let mut s = ui.write();
                        s.last_root = Some(new_root.display().to_string());
                        save_settings(&s);
                    }
                    spawn(async move {
                        if let Ok(Some(items)) = tokio::spawn(async move {
                            crate::utilities::explorer::list_dir_items(nr_async).await.ok()
                        }).await {
                            dir_items_sig.set(items)
                        }
                    });
                    results.set(Default::default());
                }
            },
            i { class: "material-icons", "folder" }
            span {
                class: "ellipsis",
                style: "flex:1; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                "{name}"
            }
        }
    }
}


pub fn shallow_should_scan(root: &Path) -> bool {
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if let Ok(ft) = e.file_type() { if ft.is_file() { return true; } }
        }
    }
    false
}

pub fn app_export_csv(items: &[crate::utilities::types::FoundFile]) -> Result<(), String> {
    let file_path = match rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name("media_results.csv")
        .save_file() {
        Some(p) => p,
        None => return Ok(()),
    };
    let mut wtr = csv::Writer::from_path(&file_path).map_err(|e| e.to_string())?;
    wtr.write_record(["path", "kind", "modified", "created", "size_bytes"]).map_err(|e| e.to_string())?;
    for it in items {
        let kind = match it.kind { crate::utilities::types::MediaKind::Image => "image", crate::utilities::types::MediaKind::Video => "video", crate::utilities::types::MediaKind::Other => "other" };
        let modified = it.modified.map(|d| d.to_rfc3339()).unwrap_or_default();
        let created = it.created.map(|d| d.to_rfc3339()).unwrap_or_default();
        let size = it.size.unwrap_or(0).to_string();
        wtr.write_record([ it.path.display().to_string(), kind.to_string(), modified, created, size ])
            .map_err(|e| e.to_string())?;
    }
    wtr.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Normalize detail column fractional widths toward a target sum while clamping individual bounds.
/// This keeps the overall grid stable after interactive resizing or auto-fit operations.
pub fn normalize_detail_widths(widths: &mut [f32;6]) {
    let target = 7.2_f32;
    let sum: f32 = widths.iter().sum();
    if sum <= 0.0 { return; }
    let scale = (target / sum).clamp(0.6, 1.4);
    for w in widths.iter_mut() { *w = (*w * scale).clamp(0.35, 6.0); }
}
