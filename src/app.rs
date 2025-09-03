use crate::components::hooks::{use_scan_channel, use_filters_block, use_layout_block, use_scan_block, use_ai_block, FiltersBlock, LayoutBlock, ScanBlock, AiBlock, ScanChannelState};
use crate::{database, utilities::{explorer::default_pictures_root, types::{DirItem, Filters, ScanResults}}, Thumbnail};
use std::{collections::{HashMap, HashSet}, time::Duration};
use std::{path::{Path, PathBuf}, rc::Rc, cell::Cell};
use dioxus::desktop::use_window;
use crate::get_settings; 
use dioxus::prelude::*;
use crate::components::{
    progress_overlay::ProgressOverlay,
    navbar::NewNavbar,
    results::ResultsView,
    preview::PreviewPane,
    debug_view::DebugView,
    sidebar::LeftSidebar,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppView { Explorer, DebugDb }

pub const DEFAULT_JOYCAPTION_PATH: &str = r#"C:\Users\Owner\Desktop\llama-joycaption-beta-one-hf-llava"#;
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const SKELETON_CSS: Asset = asset!("/assets/crimson.css");
pub const MAX_NEW_TOKENS: usize = 200;
pub const TEMPERATURE: f32 = 0.5;

pub fn app() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: SKELETON_CSS }
        document::Link { rel: "stylesheet", href: SKELETON_CSS }
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
    // Grouped filter-related signals (provides contexts internally)
    let FiltersBlock { filters, ext_filters, ext_enabled, excluded_dirs, search_text } = use_filters_block();
    // Scan & navigation grouped signals
    let ScanBlock { results, error, scanning, scan_generation, initialized, dir_items, progress, mut scan_started, scan_finished, mut recursive_current, mut only_subdirs, nav_history, path_text } = use_scan_block();

    // Bulk AI description generation progress
    let bulk_progress = use_signal(|| (0usize,0usize));
    let bulk_generating = use_signal(|| false);
    
    let mut ui = crate::components::hooks::use_settings();
    // Grouped layout/view signals (derived from settings)
    let LayoutBlock { qa_collapsed, drives_collapsed, preview_collapsed, mut preview_width, mut left_width, mut resizing_left, resizing_preview, view_mode, group_by_category, sort, mut detail_column_widths, category_col_width, mut resizing_col, progress_expanded } = use_layout_block(ui.clone());
    
    let selected_path = use_signal(|| None::<PathBuf>);
    let selected_paths = use_signal(|| HashSet::<PathBuf>::new());

    let AiBlock { ai_search_engine, ai_search_active, ai_search_results, ai_descriptions, mut ai_model_ready, ai_generating, selected_ai_meta, index_queue_len, index_active, index_completed } = use_ai_block();
    // Provide shared signals (after creation of signals they depend on)
    provide_context(bulk_progress.clone());
    provide_context(bulk_generating.clone());
    // ai contexts now provided by AiBlock
    provide_context(ui.clone());


    let indexed_paths = use_signal(|| HashSet::<String>::new());
    let app_view = use_signal(|| AppView::Explorer);
    let debug_thumb_rows = use_signal(|| Vec::<crate::Thumbnail>::new());
    let debug_doc_snips = use_signal(|| Vec::<crate::DebugDocumentSnippet>::new());
    let debug_loaded_at = use_signal(|| None::<std::time::Instant>);

    // Global cache of DB thumbnail rows keyed by absolute path (context provided)

    let all_cached = use_signal(|| HashMap::<String, Thumbnail>::new());
    provide_context(all_cached.clone());

    provide_context(all_cached.clone());

    let ai_init_started_flag = Rc::new(Cell::new(false));
    let _ai_pending_refreshed_flag = Rc::new(Cell::new(false));

    // Ingest global scan channel and actively use all exposed signals so reactivity is explicit.
    let ScanChannelState { results: scan_results_sig, progress: scan_progress_sig, scanning: scan_scanning_sig, generation: scan_generation_sig } = use_scan_channel();
    
    // Derive a memo summarizing scan status (forces dependency tracking on all fields)
    let scan_summary = {
        let r = scan_results_sig.clone();
        let p = scan_progress_sig.clone();
        let s = scan_scanning_sig.clone();
        let g = scan_generation_sig.clone();
        use_memo(move || {
            let len = r.read().items.len();
            let prog = p.read().clone();
            let scanning_now = *s.read();
            let scan_gen_val = *g.read();
            (scan_gen_val, scanning_now, len, prog)
        })
    };
    // Log whenever any part of the scan summary changes (ensures runtime usage of all fields)
    {
        let summary = scan_summary.clone();
        use_effect(move || {
            let (scan_gen_val, scanning_now, len, prog) = *summary.read();
            log::warn!("[scan-summary] gen={scan_gen_val} scanning={scanning_now} items={len} progress={prog:?}");
        });
    }

    // Ingest global scan channel and actively use all exposed signals so reactivity is explicit.
    let ScanChannelState { results: scan_results_sig, progress: scan_progress_sig, scanning: scan_scanning_sig, generation: scan_generation_sig } = use_scan_channel();
    
    // Derive a memo summarizing scan status (forces dependency tracking on all fields)
    let scan_summary = {
        let r = scan_results_sig.clone();
        let p = scan_progress_sig.clone();
        let s = scan_scanning_sig.clone();
        let g = scan_generation_sig.clone();
        use_memo(move || {
            let len = r.read().items.len();
            let prog = p.read().clone();
            let scanning_now = *s.read();
            let scan_gen_val = *g.read();
            (scan_gen_val, scanning_now, len, prog)
        })
    };
    // Log whenever any part of the scan summary changes (ensures runtime usage of all fields)
    {
        let summary = scan_summary.clone();
        use_effect(move || {
            let (scan_gen_val, scanning_now, len, prog) = *summary.read();
            log::warn!("[scan-summary] gen={scan_gen_val} scanning={scanning_now} items={len} progress={prog:?}");
        });
    }

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
                            // If initial preload failed earlier, attempt late pre-population now
                            if results_sig_pre.read().items.is_empty() && root_snapshot.exists() {
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
                                if !preload.is_empty() {
                                    log::info!("[startup] late pre-populated {} cached items after retry", preload.len());
                                    results_sig_pre.write().items.extend(preload);
                                }
                            }
                            // If initial preload failed earlier, attempt late pre-population now
                            if results_sig_pre.read().items.is_empty() && root_snapshot.exists() {
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
                                if !preload.is_empty() {
                                    log::info!("[startup] late pre-populated {} cached items after retry", preload.len());
                                    results_sig_pre.write().items.extend(preload);
                                }
                            }
                        } else {
                            log::error!("Still no database");
                        }
                    }
                });
            }


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
        });
    }

    {
        let mut filters_sig = filters.clone();
        let mut ui_sig = ui.clone();
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
                    }
                }
            }
        });
    }

    let file_records = use_memo(move || {
        let cache = all_cached.read().clone();
        let desc_map = ai_descriptions.read().clone();
        let base_items = results.read().items.clone();
        base_items.iter().map(|f| {
            let key = f.path.display().to_string();
            let cached = cache.get(&key);
            let ai_desc = desc_map.get(&key);
            crate::utilities::types::FileRecord::from_found(f, cached, ai_desc)
        }).collect::<Vec<_>>()
    });

    provide_context(file_records.clone());

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
            // debounced persistence via use_settings
            // debounced persistence via use_settings
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
            // debounced persistence via use_settings
            // debounced persistence via use_settings
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
            "data-theme": "crimson",
            class: "h-screen overflow-auto app-bg-gradient",
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
                }
                if resizing_col.read().is_some() {
                    let mut widths = detail_column_widths.read().clone();
                    normalize_detail_widths(&mut widths);
                    detail_column_widths.set(widths);
                    let mut s = ui.write();
                    s.detail_column_widths = Some(*detail_column_widths.read());
                    s.category_col_width = Some(*category_col_width.read());
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
                        results,
                        dir_items,
                    }
                    section {
                        class: "flex-1",
                        style: "overflow-y:auto; padding:10px;",
                        // If we already have items, short-circuit to ResultsView even if some earlier branch would have shown folders.
                        if results.read().items.len() > 0 && !*ai_search_active.read() {
                            ResultsView {
                                view_mode,
                                sort,
                                ui,
                                group_by_category,
                                all_cached,
                                selected_path,
                                selected_paths,
                                ai_descriptions,
                                ai_search_active,
                                ai_search_results,
                                detail_column_widths,
                                category_col_width,
                                resizing_col,
                                filters,
                            }
                        } else if *ai_search_active.read() && search_text.read().trim().is_empty() {
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
                                            class: "px-3 py-1 text-xs bg-gradient-to-r from-cyan-500 to-fuchsia-600 text-white rounded shadow hover:brightness-110 active:translate-y-px transition",
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
                                group_by_category,
                                all_cached,
                                selected_path,
                                selected_paths,
                                ai_descriptions,
                                ai_search_active,
                                ai_search_results,
                                detail_column_widths,
                                category_col_width,
                                resizing_col,
                                filters,
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
            // Global portal root for overlay elements (dropdowns, context menus, dialogs)
            // Positioned after main layout to avoid inheriting unintended stacking contexts.
            // Uses z-index token --z-portal-root defined in CSS theme.
            div { id: "portal-root", class: "pointer-events-none fixed inset-0", style: "z-index:var(--z-portal-root);" }
        }
    }
}

fn folder_entry(name: String, path: PathBuf,
    mut filters: Signal<Filters>,
    mut path_text: Signal<String>,
    mut recursive_current: Signal<bool>,
    mut only_subdirs: Signal<bool>,
    mut scan_started: Signal<Option<std::time::Instant>>,
    _scan_generation: Signal<u64>,
    _scanning: Signal<bool>,
    mut results: Signal<ScanResults>,
    dir_items: Signal<Vec<DirItem>>,
    _progress: Signal<Option<(usize,usize)>>,
    mut nav_history: Signal<Vec<PathBuf>>,
    mut ui: Signal<crate::settings::UiSettings>,
) -> Element {
    rsx! {
        div {
            key: "{name}",
            class: "flex items-center gap-3 panel px-3 py-2 button",
            "data-style": "outline",
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
                // Persist last_root (debounced persistence handled by settings hook)
                // Persist last_root (debounced persistence handled by settings hook)
                {
                    let mut s = ui.write();
                    s.last_root = Some(new_root.display().to_string());
                }
                path_text.set(new_root.display().to_string());
                recursive_current.set(false);
                if shallow_should_scan(&new_root) {
                    only_subdirs.set(false);
                    scan_started.set(Some(std::time::Instant::now()));
                    // persist last_root on navigation (debounced)
                    // persist last_root on navigation (debounced)
                    {
                        let mut s = ui.write();
                        s.last_root = Some(new_root.display().to_string());
                    }
                    // begin_scan removed: folder_entry navigation triggers reactive scan
                } else {
                    only_subdirs.set(true);
                    let mut dir_items_sig = dir_items.clone();
                    let nr_async = new_root.clone();
                    // persist last_root even when not scanning (only subdirs, debounced)
                    // persist last_root even when not scanning (only subdirs, debounced)
                    {
                        let mut s = ui.write();
                        s.last_root = Some(new_root.display().to_string());
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
