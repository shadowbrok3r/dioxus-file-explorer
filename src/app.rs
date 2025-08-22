#![allow(unused_imports)]
use crate::explorer::{default_pictures_root, drive_icon_for_root, list_drive_infos, quick_access, list_dir_items};
use crate::scan::{begin_scan, ScanMsg};
use crate::types::{DateField, DirItem, Filters, ScanResults, ViewMode};
use dioxus::prelude::*;
use dioxus::desktop::use_window;
use humansize::{format_size, DECIMAL};
use keyboard_types::Key;
use std::path::{Path, PathBuf};
use crossbeam::channel::Receiver;
use crate::settings::{load_settings, save_settings, SortBy, SortSetting};
use std::collections::{BTreeSet, BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::cell::Cell;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

pub fn app() -> Element {
    let _win = use_window();
    // Signals
    let mut filters = use_signal(Filters::default);
    let mut results = use_signal(|| ScanResults::default());
    let mut error = use_signal(|| None::<String>);
    let mut scanning = use_signal(|| false);
    let mut rx_state = use_signal(|| None::<Receiver<ScanMsg>>);
    let mut initialized = use_signal(|| false);
    let mut dir_items = use_signal(|| Vec::<DirItem>::new());
    let mut progress = use_signal(|| None::<(usize, usize)>);
    // persistent UI settings
    let mut ui = use_signal(load_settings);
    let mut qa_collapsed = use_signal(|| ui.read().qa_collapsed);
    let mut drives_collapsed = use_signal(|| ui.read().drives_collapsed);
    let mut preview_collapsed = use_signal(|| ui.read().preview_collapsed);
    let mut preview_width = use_signal(|| ui.read().preview_width.max(240).min(800));
    let mut sort = use_signal(|| ui.read().sort.clone().unwrap_or(SortSetting { by: SortBy::Name, asc: true }));
    let mut resizing_preview = use_signal(|| None::<(i32, u32)>);
    let mut left_width = use_signal(|| ui.read().left_width.max(180).min(480));
    let mut resizing_left = use_signal(|| None::<(i32, u32)>);
    let mut view_mode = use_signal(|| match ui.read().view_mode.as_deref() { Some("icons") => ViewMode::Icons, _ => ViewMode::Details });
    let mut selected_path = use_signal(|| None::<PathBuf>);
    let mut path_text = use_signal(|| String::new());
    let mut recursive_current = use_signal(|| false); // track if current scan is deep recursive
    let mut only_subdirs = use_signal(|| false); // when shallow and no immediate files present
    let mut scan_started = use_signal(|| None::<std::time::Instant>);
    let mut scan_finished = use_signal(|| None::<std::time::Instant>); // freeze elapsed when done
    // Dynamic extension & exclusion filtering
    let mut ext_filters = use_signal(|| BTreeSet::<String>::new());
    let mut ext_enabled = use_signal(|| BTreeMap::<String,bool>::new());
    let mut excluded_dirs = use_signal(|| BTreeSet::<PathBuf>::new());
    let mut search_text = use_signal(|| String::new());
    // AI Search state
    let mut ai_search_text = use_signal(|| String::new());
    let mut ai_search_results = use_signal(|| Vec::<crate::ai_search::FileMetadata>::new());
    let ai_search_engine = use_signal(|| None::<crate::ai_search::AISearchEngine>);
    let mut ai_search_active = use_signal(|| false);
    // AI image descriptions cache (path -> description)
    let ai_descriptions = use_signal(|| HashMap::<String, String>::new());
    // Track which file paths have been AI-indexed to avoid duplicate indexing on every render
    let indexed_paths = use_signal(|| HashSet::<String>::new());
    // UI state for AI model & description progress
    let mut ai_model_ready = use_signal(|| false);
    let mut ai_pending_desc = use_signal(|| 0usize);
    let mut ai_generating = use_signal(|| false);
    // One-time AI engine init guards
    let ai_init_started_flag = Rc::new(Cell::new(false));
    let ai_pending_refreshed_flag = Rc::new(Cell::new(false));
    // (Removed previous periodic tick re-render; streaming scan messages already drive UI updates.)

    // Drain scan messages
    if let Some(rx) = rx_state.read().as_ref() {
        for msg in rx.try_iter() {
            match msg {
                ScanMsg::Found(item) => {
                    if let Some(ext) = item.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                        if !ext_filters.read().contains(&ext) {
                            ext_filters.write().insert(ext.clone());
                            ext_enabled.write().entry(ext.clone()).or_insert(true);
                        }
                    }
                    results.write().items.push(item)
                },
                ScanMsg::UpdateThumb { path, thumb } => {
                    if let Some(it) = results.write().items.iter_mut().find(|f| f.path == path) {
                        it.thumb_data = Some(thumb);
                    }
                }
                ScanMsg::Progress { scanned, total } => progress.set(Some((scanned, total))),
                ScanMsg::Error(e) => { error.set(Some(e)); scanning.set(false); },
                ScanMsg::Done => { scanning.set(false); scan_finished.set(Some(std::time::Instant::now())); },
            }
        }
    }

    // Initialize default root
    if !initialized.read().clone() {
        if let Some(pics) = default_pictures_root() {
            { let mut f = filters.write(); path_text.set(pics.display().to_string()); f.root = pics; }
        }
        initialized.set(true);
        // initial shallow scan (will run even if only subfolders; can adjust if desired)
    scan_started.set(Some(std::time::Instant::now()));
    scan_finished.set(None);
        begin_scan(filters, rx_state, scanning, results, dir_items, progress, false);
    }

    // Collapsible widths
    // When both left navigation sections are collapsed, fully hide (0px) instead of leaving a sliver
    let left_w: String = if *qa_collapsed.read() && *drives_collapsed.read() { "0px".to_string() } else { format!("{}px", *left_width.read()) };
    // Preview target width; when collapsed we override style to 0 with no padding/border
    let right_w: String = if *preview_collapsed.read() { "0px".to_string() } else { format!("{}px", *preview_width.read()) };
    // Dynamic style for preview pane (remove padding & border when collapsed for true hide)
    let preview_style: String = if *preview_collapsed.read() {
        "width:0; overflow:hidden; transition: width .08s ease; position: relative; padding:0; border:none;".to_string()
    } else {
        format!("width: {right_w}; overflow: hidden; transition: width .08s ease; position: relative;")
    };

    // Memoized filtered item list (extensions, exclusions, search)
    let filtered_items = use_memo(move || {
        let enabled = ext_enabled.read().clone();
        let excluded = excluded_dirs.read().clone();
        let needle = search_text.read().to_ascii_lowercase();
        let only_with_thumb = filters.read().only_with_thumb; // new flag
        results
            .read()
            .items
            .iter()
            .filter(|it| {
                if only_with_thumb && it.thumb_data.is_none() { return false; }
                if let Some(ext) = it
                    .path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase())
                {
                    if let Some(flag) = enabled.get(&ext) {
                        if !*flag { return false; }
                    }
                }
                for ex in excluded.iter() {
                    if it.path.starts_with(ex) { return false; }
                }
                if !needle.is_empty() {
                    let name_lc = it.path.file_name().and_then(|f| f.to_str()).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
                    if !name_lc.contains(&needle) { return false; }
                }
                true
            })
            .cloned()
            .collect::<Vec<_>>()
    });
    
    // Sorted items for details view
    let sorted_items = use_memo(move || {
        let mut vec_items = filtered_items.read().clone();
        let sv = sort.read();
        use crate::settings::SortBy;
        vec_items.sort_by(|a,b| {
            let ord = match sv.by {
                SortBy::Name => a.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase()
                                    .cmp(&b.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase()),
                SortBy::Modified => a.modified.cmp(&b.modified),
                SortBy::Created => a.created.cmp(&b.created),
                SortBy::Size => a.size.cmp(&b.size),
                SortBy::Type => a.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
                                    .cmp(&b.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()),
            }; if sv.asc { ord } else { ord.reverse() }
        });
        vec_items
    });
    
    // Persist dynamic extension/enabled & exclusion sets (effect runs when either changes)
    {
        let mut ui_sig = ui.clone();
        let ext_enabled_sig = ext_enabled.clone();
        let excluded_dirs_sig = excluded_dirs.clone();
        use_effect(move || {
            // Update ui settings then persist
            let mut settings = ui_sig.write();
            settings.ext_enabled = Some(ext_enabled_sig.read().iter().map(|(k,v)| (k.clone(), *v)).collect());
            settings.excluded_dirs = Some(excluded_dirs_sig.read().iter().map(|p| p.display().to_string()).collect());
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
            spawn(async move {
                match crate::ai_search::AISearchEngine::new().await {
                    Ok(engine) => {
                        let loaded = engine.load_cached().await;
                        log::info!("AI Search Engine initialized (cached {} rows)", loaded);
                        let paths = engine.list_indexed_paths().await;
                        for p in paths.iter() { indexed_sig.write().insert(p.clone()); }
                        if let Ok(files) = engine.get_all_files().await {
                            for f in files.iter() { if let Some(desc) = &f.description { ai_desc_sig.write().insert(f.path.clone(), desc.clone()); } }
                        }
                        let engine_clone = engine.clone();
                        spawn(async move {
                            if let Err(e) = engine_clone.ensure_vision_model().await { log::warn!("Vision model warm-up failed: {}", e); } else { ai_model_ready.set(true); }
                        });
                        ai_engine_sig.set(Some(engine));
                    }
                    Err(e) => log::error!("Failed to initialize AI Search Engine: {}", e),
                }
            });
        });
    }
    // After model ready, enqueue one-time enrichment & pending refresh
    {
        let ai_engine_sig = ai_search_engine.clone();
        use_effect(move || {
            if !*ai_model_ready.read() || ai_pending_refreshed_flag.get() { return; }
            if let Some(engine) = ai_engine_sig.read().as_ref() {
                ai_pending_refreshed_flag.set(true);
                let engine_clone = engine.clone();
                spawn(async move {
                    ai_generating.set(true);
                    ai_pending_desc.set(engine_clone.count_missing_descriptions().await);
                    let produced = engine_clone.enrich_missing_descriptions().await;
                    ai_pending_desc.set(engine_clone.count_missing_descriptions().await);
                    ai_generating.set(false);
                    if produced > 0 { log::info!("Enriched {} missing AI descriptions post warm-up", produced); }
                });
            }
        });
    }
    
    // Index newly discovered files in AI search engine and populate description cache (idempotent)
    {
        let results_sig = results.clone();
        let ai_engine_sig = ai_search_engine.clone();
        let ai_desc_sig = ai_descriptions.clone();
        let indexed_sig = indexed_paths.clone();
        use_effect(move || {
            // Determine which items are new (not yet indexed)
            let snapshot = results_sig.read().items.clone();
            let already = indexed_sig.read().clone();
            let new_items: Vec<_> = snapshot
                .into_iter()
                .filter(|f| !already.contains(&f.path.display().to_string()))
                .collect();
            if new_items.is_empty() { return; }
            if ai_engine_sig.read().is_none() { return; }
            let engine_opt = ai_engine_sig.read().clone();
            let mut ai_desc_sig = ai_desc_sig.clone();
            let mut indexed_sig = indexed_sig.clone();
            log::info!("AI indexing effect: {} new files to index", new_items.len());
            spawn(async move {
                if let Some(engine) = engine_opt {
                    for item in new_items.iter() {
                        let path_str = item.path.display().to_string();
                        // Double-check path still not indexed (race guard)
                        if indexed_sig.read().contains(&path_str) { continue; }
                        let metadata = crate::ai_search::found_file_to_metadata(item);
                        match engine.index_file(metadata.clone()).await {
                            Ok(_) => {
                                indexed_sig.write().insert(path_str.clone());
                                if let Some(meta) = engine.get_file_metadata(&metadata.path).await {
                                    if let Some(desc) = meta.description {
                                        ai_desc_sig.write().insert(path_str.clone(), desc);
                                    } else if meta.file_type == "image" {
                                        // Schedule single description generation in background
                                        let engine_clone = engine.clone();
                                        let path_clone = metadata.path.clone();
                                        let mut ai_desc_sig2 = ai_desc_sig.clone();
                                        spawn(async move {
                                            if let Ok(Some(desc)) = engine_clone.generate_description_for_path(&path_clone, false).await {
                                                ai_desc_sig2.write().insert(path_clone.clone(), desc);
                                            }
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to index file {}: {}", path_str, e);
                            }
                        }
                    }
                }
            });
        });
    }
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { href: "https://fonts.googleapis.com/icon?family=Material+Icons", rel: "stylesheet" }

        div { 
            class: "h-screen overflow-hidden",
            // Global mouse event handlers for resizing
            onmousemove: move |evt| {
                if let Some((start_x, start_width)) = resizing_left.read().clone() {
                    let delta = evt.client_coordinates().x as i32 - start_x;
                    let new_width = (start_width as i32 + delta).max(180).min(480) as u32;
                    left_width.set(new_width);
                }
                if let Some((start_x, start_width)) = resizing_preview.read().clone() {
                    let delta = start_x - evt.client_coordinates().x as i32; // Reversed for right-side resize
                    let new_width = (start_width as i32 + delta).max(240).min(800) as u32;
                    preview_width.set(new_width);
                }
            },
            onmouseup: move |_| {
                if resizing_left.read().is_some() {
                    resizing_left.set(None);
                    let mut s = ui.write();
                    s.left_width = *left_width.read();
                    save_settings(&s);
                }
                if resizing_preview.read().is_some() {
                    resizing_preview.set(None);
                    let mut s = ui.write();
                    s.preview_width = *preview_width.read();
                    save_settings(&s);
                }
            },

        // Top fixed header (reordered: path input far left, preview toggle far right)
    header { class: "flex items-center gap-2 px-3 py-2 bg-panel border-b border-stroke",
            // Path input (primary flex element on left)
            input {
                class: "flex-1 bg-muted text-var-text border border-stroke rounded-md px-2 py-1",
                value: "{path_text.read().clone()}",
                oninput: move |evt| { path_text.set(evt.value()); },
                onkeydown: move |evt| {
                    if evt.key() == Key::Enter {
                        let p = PathBuf::from(path_text.read().clone());
                        if p.exists() { let new_root = p.clone(); { let mut f = filters.write(); f.root = new_root.clone(); } recursive_current.set(false); if shallow_should_scan(&new_root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); dir_items.set(list_dir_items(new_root).unwrap_or_default()); results.set(Default::default()); } }
                    }
                }
            }
            // Up directory
            button { class: "btn", disabled: filters.read().root.parent().is_none(), onclick: move |_| {
                    let mut new_root = filters.read().root.clone();
                    if new_root.pop() {
                        { let mut f = filters.write(); path_text.set(new_root.display().to_string()); f.root = new_root; }
                        recursive_current.set(false);
                        if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); dir_items.set(list_dir_items(filters.read().root.clone()).unwrap_or_default()); results.set(Default::default()); }
                    }
                }, i { class: "material-icons", "arrow_upward" } }
            // Deep scan trigger
            button { class: "btn", title: "Deep recursive scan all subfolders", onclick: move |_| {
                    if scanning.read().clone() { rx_state.set(None); scanning.set(false); }
                    recursive_current.set(true);
                    scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, rx_state, scanning, results, dir_items, progress, true);
                }, i { class: "material-icons", "travel_explore" } }
            // Cancel current scan
            button { class: "btn", disabled: !scanning.read().clone(), onclick: move |_| { rx_state.set(None); scanning.set(false); scan_finished.set(Some(std::time::Instant::now())); }, i { class: "material-icons", "close" } }
            // View mode toggles
            button { class: "btn", title: "Icons view", onclick: move |_| { view_mode.set(ViewMode::Icons); let mut s = ui.write(); s.view_mode = Some("icons".into()); save_settings(&s); }, i { class: "material-icons", "grid_view" } }
            button { class: "btn", title: "Details view", onclick: move |_| { view_mode.set(ViewMode::Details); let mut s = ui.write(); s.view_mode = Some("details".into()); save_settings(&s); }, i { class: "material-icons", "view_list" } }
            // Export CSV
            button { class: "btn", disabled: results.read().items.is_empty(), onclick: move |_| {
                    if results.read().items.is_empty() { return; }
                    if let Err(e) = app_export_csv(&results.read().items) { error.set(Some(e)); } else { error.set(None); }
                }, i { class: "material-icons", "download" } }
            // Toggle left navigation (QA + Drives)
            button { class: "btn", title: if *qa_collapsed.read() && *drives_collapsed.read() { "Show left navigation" } else { "Hide left navigation" }, onclick: move |_| {
                    let hide = !(*qa_collapsed.read() && *drives_collapsed.read());
                    qa_collapsed.set(hide); drives_collapsed.set(hide);
                    let mut s = ui.write(); s.qa_collapsed = hide; s.drives_collapsed = hide; save_settings(&s);
                }, i { class: "material-icons", { if *qa_collapsed.read() && *drives_collapsed.read() { "chevron_right" } else { "chevron_left" } } }
            }
            // Search box for client-side filtering
            input { class: "w-56 bg-muted text-var-text border border-stroke rounded-md px-2 py-1 text-sm", placeholder: "Search...", value: "{search_text.read().clone()}", oninput: move |e| search_text.set(e.value()) }
            
            // AI Global Search
            div { class: "flex items-center gap-2",
                button {
                    class: if *ai_search_active.read() { "btn bg-accent text-white" } else { "btn" },
                    title: "Toggle AI Smart Search",
                    onclick: move |_| {
                        let new_state = !*ai_search_active.read();
                        log::info!("AI search toggle clicked -> {}", new_state);
                        ai_search_active.set(new_state);
                        if new_state {
                            ai_search_text.set(String::new());
                            ai_search_results.set(Vec::new());
                        }
                    },
                    i { class: "material-icons", "psychology" }
                }
                if *ai_search_active.read() {
                    // Inline status chips
                    if !*ai_model_ready.read() { span { class: "text-10px px-2 py-0.5 rounded bg-muted border border-stroke text-weak", "Loading vision model..." } }
                    else if *ai_generating.read() { span { class: "text-10px px-2 py-0.5 rounded bg-accent/10 border border-accent text-accent", "Generating descriptions ({ai_pending_desc.read()})" } }
                    else if *ai_pending_desc.read() > 0 { span { class: "text-10px px-2 py-0.5 rounded bg-muted border border-stroke text-weak", "{ai_pending_desc.read()} missing" } }
                }
                if *ai_search_active.read() && ai_search_engine.read().is_some() {
                    // Manual global enrichment button
                    button { class: "btn", title: "Generate all missing image descriptions", onclick: move |_| {
                        if let Some(engine) = ai_search_engine.read().clone() {
                            let mut ai_generating2 = ai_generating.clone();
                            let mut ai_pending2 = ai_pending_desc.clone();
                            let mut desc_map = ai_descriptions.clone();
                            spawn(async move {
                                ai_generating2.set(true);
                                ai_pending2.set(engine.count_missing_descriptions().await);
                                let produced = engine.enrich_missing_descriptions().await;
                                // Refresh description cache
                                if produced > 0 {
                                    if let Ok(all) = engine.get_all_files().await { for f in all { if let Some(d) = f.description { desc_map.write().insert(f.path.clone(), d); } } }
                                }
                                ai_pending2.set(engine.count_missing_descriptions().await);
                                ai_generating2.set(false);
                            });
                        }
                    }, i { class: "material-icons", "auto_fix_high" } }
                }
                // Manual force reindex for currently selected file (if AI engine ready)
                if selected_path.read().is_some() && ai_search_engine.read().is_some() {
                    button {
                        class: "btn",
                        title: "Force AI reindex selected file (regenerate description & tags)",
                        onclick: move |_| {
                            if let (Some(p), Some(engine)) = (selected_path.read().clone(), ai_search_engine.read().clone()) {
                                let path_str = p.display().to_string();
                                let mut ai_desc_sig = ai_descriptions.clone();
                                spawn(async move {
                                    match engine.force_reindex_path(&path_str).await {
                                        Ok(_) => {
                                            if let Some(meta) = engine.get_file_metadata(&path_str).await { if let Some(desc) = meta.description { ai_desc_sig.write().insert(path_str.clone(), desc); } }
                                            log::info!("Manual force reindex complete: {}", path_str);
                                        }
                                        Err(e) => log::warn!("Force reindex failed for {}: {}", path_str, e),
                                    }
                                });
                            }
                        },
                        i { class: "material-icons", "refresh" }
                    }
                }
                
                if *ai_search_active.read() {
                    input {
                        class: "w-64 bg-muted text-var-text border border-accent rounded-md px-2 py-1 text-sm",
                        placeholder: "AI Smart Search (describe what you're looking for)...",
                        value: "{ai_search_text.read().clone()}",
                        oninput: move |e| {
                            let query = e.value();
                            ai_search_text.set(query.clone());
                            if query.trim().is_empty() { log::debug!("AI search query cleared"); ai_search_results.set(Vec::new()); return; }
                            if ai_search_engine.read().is_none() { log::warn!("AI search invoked before engine ready"); return; }
                            let engine = ai_search_engine.read().clone();
                            let mut results_sig = ai_search_results.clone();
                            let query = query.clone();
                            log::info!("Dispatching AI semantic search: '{}'", query);
                            spawn(async move {
                                if let Some(engine) = engine {
                                    match engine.search(&query).await {
                                        Ok(results) => { log::info!("AI search '{}' -> {} results", query, results.len()); results_sig.set(results); }
                                        Err(e) => { log::error!("AI search error for '{}': {}", query, e); }
                                    }
                                }
                            });
                        }
                    }
                }
            }
            
            // Spacer pushes preview toggle to far right
            span { class: "flex-1" }
            // Preview pane toggle (far right)
            button { class: "btn", title: if *preview_collapsed.read() { "Show preview pane" } else { "Hide preview pane" }, onclick: move |_| { let curr = *preview_collapsed.read(); preview_collapsed.set(!curr); let mut s = ui.write(); s.preview_collapsed = !curr; s.preview_width = *preview_width.read(); save_settings(&s); },
                i { class: "material-icons", { if *preview_collapsed.read() { "visibility" } else { "visibility_off" } } }
            }
        }

        // Progress bar + status details (persists briefly after completion to show final numbers)
        if progress.read().is_some() || scanning.read().clone() {
            { let prog = progress.read().clone(); let started_opt = scan_started.read().clone(); let finished_opt = scan_finished.read().clone(); let elapsed = match (started_opt, finished_opt) { (Some(st), Some(fin)) => fin.duration_since(st), (Some(st), None) => st.elapsed(), _ => std::time::Duration::default() }; let secs = elapsed.as_secs_f32(); let done = !scanning.read().clone(); let found_count = results.read().items.len();
                rsx!{ div { class: "w-full bg-muted overflow-hidden flex flex-col", style: "position:relative;",
                    // bar (show completed state filled when done & total known)
                    if let Some((scanned,total)) = prog {
                        if total > 0 { { let pct = (scanned as f32 / total.max(1) as f32 * 100.0).min(100.0); rsx!{ div { class: "h-1", class: if done { "bg-green-500" } else { "bg-accent" }, style: "width:{pct}%; transition:width .12s linear;" } } } }
                        else { div { class: "h-1 bg-accent animate-pulse", style: "width:40%; position:absolute; left:0; animation: scan-indeterminate 1.2s linear infinite;" } }
                    } else { div { class: "h-1 bg-accent animate-pulse", style: "width:30%;" } }
                    // info line
                    div { class: "flex flex-wrap gap-3 px-2 py-1 text-11px text-weak items-center", style: "user-select:none;",
                        // Status chip indicating scan mode or Done
                        span { class: "px-1.5 py-0.5 rounded-full text-10px tracking-wide uppercase font-medium border",
                            class: if done { "bg-green-600/15 border-green-500 text-green-400" } else { "bg-accent/10 border-accent text-accent" },
                            { if done { "Done" } else if *recursive_current.read() { "Deep" } else { "Shallow" } }
                        }
                        if let Some((scanned,total)) = prog {
                            if total > 0 {
                                {{
                                    let pct = scanned as f32 * 100.0 / total.max(1) as f32;
                                    let pct_rounded = pct.round() as i32;
                                    let rate = if secs > 0.15 { scanned as f32 / secs } else { 0.0 };
                                    rsx! {
                                        span { "{scanned} / {total} ({pct_rounded}%)" }
                                        span { "found {found_count}" }
                                        if done { span { "in {secs:.1}s" } }
                                        if !done && rate > 0.1 { span { "{rate:.1} items/s" } }
                                    }
                                }}
                            } else {
                                {{
                                    let rate = if secs > 0.15 { scanned as f32 / secs } else { 0.0 };
                                    rsx! {
                                        span { "{scanned} items" }
                                        span { "found {found_count}" }
                                        if !done && rate > 0.1 { span { "{rate:.1} items/s" } }
                                        { let txt = if done { format!("in {:.1}s", secs) } else { format!("elapsed {:.1}s", secs) }; rsx!{ span { "{txt}" } } }
                                    }
                                }}
                            }
                        } else { span { if done { "No items" } else { "Starting scan..." } } }
                    }
                } }
            }
        }

        // Sticky filters bar below header
    section { class: "filters", style: "position: sticky; top: 56px; z-index: 5;",
            div { class: "filter-group",
                label { "Root:" }
                code { class: "path", {filters.read().root.display().to_string()} }
            }
            div { class: "filter-group",
                label { "Types:" }
                label { class: "chk",
                    input { r#type: "checkbox", checked: filters.read().include_images, oninput: move |_| {
                        // Toggle include_images and clone root while mutable borrow active
                        let root = {
                            let mut flt = filters.write();
                            flt.include_images = !flt.include_images;
                            flt.root.clone()
                        };
                        scan_started.set(Some(std::time::Instant::now()));
                        scan_finished.set(None);
                        let rec = *recursive_current.read();
                        if shallow_should_scan(&root) || rec {
                            only_subdirs.set(false);
                            begin_scan(filters, rx_state, scanning, results, dir_items, progress, rec);
                        }
                    } }
                    span { " Images" }
                }
                label { class: "chk",
                    input { r#type: "checkbox", checked: filters.read().include_videos, oninput: move |_| {
                        let root = {
                            let mut flt = filters.write();
                            flt.include_videos = !flt.include_videos;
                            flt.root.clone()
                        };
                        scan_started.set(Some(std::time::Instant::now()));
                        scan_finished.set(None);
                        let rec = *recursive_current.read();
                        if shallow_should_scan(&root) || rec {
                            only_subdirs.set(false);
                            begin_scan(filters, rx_state, scanning, results, dir_items, progress, rec);
                        }
                    } }
                    span { " Videos" }
                }
                div { class: "filter-group",
                    label { class: "chk",
                        input { r#type: "checkbox", checked: filters.read().only_with_thumb, oninput: move |_| {
                            let current_val = filters.read().only_with_thumb;
                            filters.write().only_with_thumb = !current_val;
                        }}
                        span { " Thumbs only" }
                    }
                }
            }
            div { class: "filter-group",
                button { class: "btn", onclick: move |_| {
                        { let mut f = filters.write(); f.date_field = match f.date_field { DateField::Modified => DateField::Created, DateField::Created => DateField::Modified }; }
                        recursive_current.set(false);
                        if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); }
                    }, span { "Date: " } strong { match filters.read().date_field { DateField::Modified => "Modified", DateField::Created => "Created" } } }
                label { " After:" }
                input { r#type: "date", value: filters.read().modified_after.clone().unwrap_or_default(), oninput: move |evt| { { let mut f = filters.write(); f.modified_after = Some(evt.value()); } recursive_current.set(false); if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); } } }
                label { " Before:" }
                input { r#type: "date", value: filters.read().modified_before.clone().unwrap_or_default(), oninput: move |evt| { { let mut f = filters.write(); f.modified_before = Some(evt.value()); } recursive_current.set(false); if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); } } }
            }
            // Dynamic extensions filter row
            if !ext_filters.read().is_empty() {
                div { class: "filter-group", style: "display:flex; flex-wrap:wrap; gap:6px; align-items:center; max-width:760px;",
                    label { class: "font-semibold", "Ext:" }
                    for ext in ext_filters.read().iter() {
                        { let ext_name = ext.clone();
                            {{
                                let active = *ext_enabled.read().get(&ext_name).unwrap_or(&true);
                                let style_str = if active {
                                    // Accent background & border when enabled
                                    "user-select:none; background:var(--accent-weak); border-color:var(--accent);"
                                } else {
                                    // Dim when disabled
                                    "user-select:none; opacity:.45;"
                                };
                                rsx!{
                                    label { key: "ext-{ext_name}", class: "flex items-center gap-1 text-11px px-1.5 py-0.5 rounded-md border cursor-pointer", style: "{style_str}",
                                        input { r#type: "checkbox", checked: active, oninput: move |_| {
                                            let mut map = ext_enabled.write();
                                            let cur = map.get(&ext_name).cloned().unwrap_or(true);
                                            map.insert(ext_name.clone(), !cur);
                                        } }
                                        span { ".{ext_name}" }
                                    }
                                }
                            }}
                        }
                    }
                }
            }
            if !excluded_dirs.read().is_empty() {
                div { class: "filter-group flex items-center gap-2", 
                    span { class: "text-11px", "Excluded: {excluded_dirs.read().len()} dirs" }
                    button { class: "btn text-10px px-2 py-0.5", onclick: move |_| { excluded_dirs.write().clear(); }, "Clear" }
                }
            }
        }

        if let Some(err) = error.read().as_ref() { div { class: "error", code { "{err}" } } }

        // Body: three columns grid; sidebars fixed width; center scrolls
        div { class: "flex", style: "height: calc(100vh - 56px - 48px);", // header + filters approx
            // Left sidebar
            aside { class: "bg-panel border-r border-stroke", style: "width: {left_w}; overflow: hidden; transition: width .08s ease; position: relative;",
                div { class: "flex items-center justify-between px-3 py-2 border-b border-stroke",
                    h3 { class: "text-18px font-semibold", "Quick Access" }
                    button { class: "btn", onclick: move |_| { let curr = *qa_collapsed.read(); qa_collapsed.set(!curr); let mut s = ui.write(); s.qa_collapsed = !curr; save_settings(&s); }, i { class: "material-icons", { if *qa_collapsed.read() { "chevron_right" } else { "expand_more" } } } }
                }
                if !*qa_collapsed.read() {
                    ul { class: "qa-list", style: "padding: 8px 10px; display: grid; gap: 6px;",
                        for qa in quick_access().into_iter() {
                            { let label = qa.label.clone(); let path = qa.path.clone();
                                rsx!{ li { key: "{label}", class: "btn", onclick: move |_| { let new_root = path.clone(); { let mut f = filters.write(); f.root = new_root.clone(); } path_text.set(new_root.display().to_string()); recursive_current.set(false); if shallow_should_scan(&new_root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); dir_items.set(list_dir_items(new_root).unwrap_or_default()); results.set(Default::default()); } },
                                    i { class: "material-icons", "star" } span { "{label}" }
                                } }
                            }
                        }
                    }
                }
                div { class: "flex items-center justify-between px-3 py-2 border-t border-b border-stroke",
                    h3 { class: "text-18px font-semibold", "Drives" }
                    button { class: "btn", onclick: move |_| { let curr = *drives_collapsed.read(); drives_collapsed.set(!curr); let mut s = ui.write(); s.drives_collapsed = !curr; save_settings(&s); }, i { class: "material-icons", { if *drives_collapsed.read() { "chevron_right" } else { "expand_more" } } } }
                }
                if !*drives_collapsed.read() {
                    ul { class: "qa-list", style: "padding: 8px 10px; display: grid; gap: 6px;",
                        for info in list_drive_infos().into_iter() {
                            { let root = info.root.clone(); let display = if info.label.is_empty() { info.root.clone() } else { format!("{} ({})", info.root, info.label) }; let path = PathBuf::from(root.clone());
                                let free = format_size(info.free, DECIMAL); let total = format_size(info.total, DECIMAL);
                                rsx!{ li { key: "{display}", class: "btn", style: "display:grid; grid-template-columns: 24px 1fr; align-items:center; gap:6px; padding:6px 8px;", onclick: move |_| { let new_root = path.clone(); { let mut f = filters.write(); f.root = new_root.clone(); } path_text.set(new_root.display().to_string()); recursive_current.set(false); if shallow_should_scan(&new_root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); dir_items.set(list_dir_items(new_root).unwrap_or_default()); results.set(Default::default()); } },
                                    i { class: "material-icons", style: "font-size:20px;", { drive_icon_for_root(&root) } }
                                    div { class: "flex flex-col min-w-0", 
                                        span { title: "{display}", style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:13px;", "{display}" }
                                        span { class: "text-weak text-11px", style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{free} free of {total}" }
                                    }
                                } }
                            }
                        }
                    }
                }
                // If both collapsed, show a small expand button so the panel can be reopened
                if *qa_collapsed.read() && *drives_collapsed.read() {
                    // Make the collapsed gutter clickable to expand
                    div { style: "position:absolute; inset:0; z-index: 10; cursor: pointer;", onclick: move |_| {
                            qa_collapsed.set(false); drives_collapsed.set(false);
                            let mut s = ui.write(); s.qa_collapsed = false; s.drives_collapsed = false; save_settings(&s);
                        }
                    }
                    div { style: "position:absolute; top:8px; left:2px; width:20px; height:20px; z-index: 11;",
                        button { class: "btn", title: "Expand side panel", onclick: move |_| { qa_collapsed.set(false); let mut s = ui.write(); s.qa_collapsed = false; save_settings(&s); }, i { class: "material-icons", "chevron_right" } }
                    }
                }
                // Left resize handle (on the right edge of sidebar)
                div { class: "resize-handle", style: "position:absolute; top:0; right:-3px; width:6px; height:100%; cursor: ew-resize;",
                    onmousedown: move |evt| { resizing_left.set(Some((evt.client_coordinates().x as i32, *left_width.read()))); }
                }
            }

            // Center content (scroll only this column)
            section { class: "flex-1", style: "overflow-y: auto; padding: 10px;",
                
                // AI Search Results
                if *ai_search_active.read() && !ai_search_results.read().is_empty() {
                    div { class: "mb-6",
                        h3 { class: "text-lg font-semibold mb-3 flex items-center gap-2",
                            i { class: "material-icons text-accent", "psychology" }
                            "AI Smart Search Results"
                            span { class: "text-sm font-normal text-weak", "({ai_search_results.read().len()} found)" }
                        }
                        
                        div { class: "grid gap-3",
                            for result in ai_search_results.read().iter() {
                                { let result_clone = result.clone();
                                  let path_str = result_clone.path.clone();
                                  let filename = result_clone.filename.clone();
                                  let file_type = result_clone.file_type.clone();
                                  let description = result_clone.description.clone().unwrap_or_default();
                                  let tags = result_clone.tags.clone();
                                  let thumb_opt = result_clone.thumb_b64.clone().or(result_clone.thumbnail_path.clone());
                                  
                                  rsx! { 
                                    div { 
                                        key: "{path_str}",
                                        class: "bg-panel border border-stroke rounded-lg p-4 hover:border-accent transition-colors cursor-pointer",
                                        onclick: {
                                            let path_for_single = path_str.clone();
                                            move |_| {
                                            // Single click: select for preview pane
                                            selected_path.set(Some(PathBuf::from(path_for_single.clone())));
                                            if *preview_collapsed.read() {
                                                preview_collapsed.set(false);
                                                let mut s = ui.write();
                                                s.preview_collapsed = false;
                                                s.preview_width = *preview_width.read();
                                                save_settings(&s);
                                            }
                                        }},
                                        ondoubleclick: {
                                            let path_for_double = path_str.clone();
                                            move |_| {
                                                // Double click: open with native handler
                                                let _ = open::that(&path_for_double);
                                            }
                                        },
                                        
                                        div { class: "flex items-start gap-3",
                                            // Thumbnail (if available) else icon
                                            if let Some(t) = thumb_opt.clone() { img { class: "h-16 w-16 rounded-md object-cover border border-stroke flex-shrink-0", src: "{t}" } } else {
                                                i { 
                                                    class: "material-icons text-2xl flex-shrink-0",
                                                    match file_type.as_str() {
                                                        "image" => "photo",
                                                        "video" => "smart_display", 
                                                        _ => "insert_drive_file"
                                                    }
                                                }
                                            }
                                            
                                            // File details
                                            div { class: "flex-1 min-w-0",
                                                h4 { class: "font-medium text-base mb-1 truncate", "{filename}" }
                                                p { class: "text-sm text-weak mb-2 truncate", "{path_str}" }
                                                
                                                if !description.is_empty() {
                                                    p { class: "text-sm text-primary mb-2", "{description}" }
                                                } else if result_clone.file_type == "image" && *ai_model_ready.read() {
                                                    p { class: "text-xs text-weak italic", "(Generating description...)" }
                                                }
                                                
                                                if !tags.is_empty() {
                                                    div { class: "flex flex-wrap gap-1",
                                                        for tag in tags.iter() {
                                                            span { 
                                                                key: "{tag}",
                                                                class: "px-2 py-1 bg-accent-weak text-accent text-xs rounded-full",
                                                                "{tag}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                  }
                                }
                            }
                        }
                    }
                } else if *ai_search_active.read() && ai_search_text.read().trim().is_empty() {
                    div { class: "text-center py-12 text-weak",
                        i { class: "material-icons text-6xl mb-4 opacity-50", "psychology" }
                        h3 { class: "text-lg mb-2", "AI Smart Search" }
                        p { "Describe what you're looking for and let AI help you find it" }
                        p { class: "text-sm mt-2", "Try: \"photos of dogs\", \"documents about project planning\", \"videos from last vacation\"" }
                    }
                } else if *ai_search_active.read() && !ai_search_text.read().trim().is_empty() && ai_search_results.read().is_empty() {
                    div { class: "text-center py-12 text-weak",
                        i { class: "material-icons text-6xl mb-4 opacity-50", "search_off" }
                        h3 { class: "text-lg mb-2", "No AI Results Found" }
                        p { "Try a different description or check if files are indexed" }
                    }
                }
                
                if !*ai_search_active.read() && results.read().items.is_empty() {
                    // Folder grid
                    section { class: "folder-list", style: "display:flex; flex-direction:column; gap:4px;",
                        for d in dir_items.read().iter() {
                            { let name = d.path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string(); let path = d.path.clone();
                                rsx!{ div { class: "flex items-center gap-3 bg-panel border border-stroke rounded-md px-3 py-2 btn", key: "{name}", style: "min-height:40px;", onclick: move |_| { let new_root = path.clone(); { let mut f = filters.write(); f.root = new_root.clone(); } path_text.set(new_root.display().to_string()); recursive_current.set(false); if shallow_should_scan(&new_root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); dir_items.set(list_dir_items(new_root).unwrap_or_default()); results.set(Default::default()); } },
                                    i { class: "material-icons", "folder" }
                                    span { class: "ellipsis", style: "flex:1; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{name}" }
                                } }
                            }
                        }
                    }
                    p { class: "empty",
                        if scanning.read().clone() {
                            match progress.read().clone() {
                                Some((s,t)) => if t > 0 { format!("{}... {} / {}", if *recursive_current.read() { "Deep scanning" } else { "Scanning" }, s, t) } else { format!("{}... {}", if *recursive_current.read() { "Deep scanning" } else { "Scanning" }, s) },
                                None => if *recursive_current.read() { "Deep scanning...".to_string() } else { "Scanning...".to_string() },
                            }
                        } else if *only_subdirs.read() {
                            "" // placeholder suppressed; actual block rendered below
                        } else { "No results - adjust filters." }
                    }
                    if *only_subdirs.read() {
                        div { class: "mt-8 flex flex-col items-center gap-3 text-slate-400 text-sm",
                            span { "Folder contains only subfolders." }
                            div { class: "flex gap-2",
                                button { class: "btn px-3 py-1 text-xs bg-gradient-to-r from-cyan-500 to-fuchsia-600 text-white rounded shadow hover:brightness-110 active:translate-y-px transition",
                                    onclick: move |_| {
                                        // Force shallow scan anyway
                                        only_subdirs.set(false);
                                        scan_started.set(Some(std::time::Instant::now()));
                                        recursive_current.set(false);
                                        begin_scan(filters, rx_state, scanning, results, dir_items, progress, false);
                                    },
                                    i { class: "material-icons mr-1 align-middle text-base", "play_arrow" }
                                    span { "Scan Anyway" }
                                }
                                // button { class: "btn px-3 py-1 text-xs bg-slate-700 hover:bg-slate-600 text-slate-200 rounded border border-slate-600",
                                //     // Removed duplicate deep scan button here; users can trigger deep scan from header.
                                //     disabled: true,
                                //     title: "Use Deep Scan button in header",
                                //     onclick: move |_| {},
                                //     span { class: "opacity-60", "Deep Scan (see header)" }
                                // }
                            }
                        }
                    }
                } else if !*ai_search_active.read() && filtered_items.read().is_empty() {
                    p { class: "text-sm text-weak", "No matching results - adjust filters/search." }
                } else if !*ai_search_active.read() {
                    if *view_mode.read() == ViewMode::Icons {
                        ul { class: "results",
                            for item in filtered_items.read().iter() {
                                { let item_path = item.path.clone(); let item_path_click = item_path.clone(); let item_path_open = item_path.clone(); let path_str = item_path.display().to_string(); let mtime = item.modified.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "-".to_string()); let size = item.size.map(|s| format_size(s, DECIMAL)).unwrap_or_else(|| "-".to_string()); let desc_opt = ai_descriptions.read().get(&path_str).cloned();
                                    rsx! { li { key: "{path_str}", onclick: move |_| { selected_path.set(Some(item_path_click.clone())); if *preview_collapsed.read() { preview_collapsed.set(false); let mut s = ui.write(); s.preview_collapsed = false; save_settings(&s); } }, ondoubleclick: move |_| { let _ = open::that(&item_path_open); }, oncontextmenu: move |evt| { evt.prevent_default(); if let Some(parent) = item_path.parent() { excluded_dirs.write().insert(parent.to_path_buf()); } },
                                        if let Some(img) = &item.thumb_data { img { class: "h-16 w-16 rounded-md object-contain bg-111216 border border-stroke", src: "{img}" } } else { i { class: "material-icons text-22px file-icon", {item.icon_name()} } }
                                        div { class: "meta",
                                            h3 { class: "name", {Path::new(&path_str).file_name().and_then(|s| s.to_str()).unwrap_or(&path_str)} }
                                            if let Some(desc) = desc_opt { p { class: "text-11px text-weak line-clamp-2", style: "max-width:220px;", "{desc}" } }
                                            div { class: "sub", span { class: "mtime", "{mtime}" } span { class: "size", "{size}" } code { class: "path", {path_str} } }
                                        }
                                    } }
                                }
                            }
                        }
                    } else {
                        // Details view (thumbnail, name, path, size, modified, created, type)
                        div {
                            div { class: "results-header", style: "display:grid; grid-template-columns:56px 1.2fr 2fr .7fr .9fr .9fr .6fr; gap:10px; align-items:center; padding:6px 10px; color:var(--text-weak); border-bottom:1px solid var(--stroke); font-size:12px;",
                                span { "" }
                                span { class: "cursor-pointer select-none", onclick: move |_| {
                                        use crate::settings::SortBy; let mut s_sig = sort.write(); let mut new = s_sig.clone(); if new.by == SortBy::Name { new.asc = !new.asc; } else { new.by = SortBy::Name; new.asc = true; } *s_sig = new.clone(); let mut uiw = ui.write(); uiw.sort = Some(new); save_settings(&uiw);
                                    }, { let sv2 = sort.read(); if sv2.by == crate::settings::SortBy::Name { if sv2.asc { "▲ " } else { "▼ " } } else { "" } } "Name" }
                                span { "Path" }
                                span { class: "cursor-pointer select-none", onclick: move |_| {
                                        use crate::settings::SortBy; let mut s_sig = sort.write(); let mut new = s_sig.clone(); if new.by == SortBy::Size { new.asc = !new.asc; } else { new.by = SortBy::Size; new.asc = true; } *s_sig = new.clone(); let mut uiw = ui.write(); uiw.sort = Some(new); save_settings(&uiw);
                                    }, { let sv2 = sort.read(); if sv2.by == crate::settings::SortBy::Size { if sv2.asc { "▲ " } else { "▼ " } } else { "" } } "Size" }
                                span { class: "cursor-pointer select-none", onclick: move |_| {
                                        use crate::settings::SortBy; let mut s_sig = sort.write(); let mut new = s_sig.clone(); if new.by == SortBy::Modified { new.asc = !new.asc; } else { new.by = SortBy::Modified; new.asc = true; } *s_sig = new.clone(); let mut uiw = ui.write(); uiw.sort = Some(new); save_settings(&uiw);
                                    }, { let sv2 = sort.read(); if sv2.by == crate::settings::SortBy::Modified { if sv2.asc { "▲ " } else { "▼ " } } else { "" } } "Modified" }
                                span { class: "cursor-pointer select-none", onclick: move |_| {
                                        use crate::settings::SortBy; let mut s_sig = sort.write(); let mut new = s_sig.clone(); if new.by == SortBy::Created { new.asc = !new.asc; } else { new.by = SortBy::Created; new.asc = true; } *s_sig = new.clone(); let mut uiw = ui.write(); uiw.sort = Some(new); save_settings(&uiw);
                                    }, { let sv2 = sort.read(); if sv2.by == crate::settings::SortBy::Created { if sv2.asc { "▲ " } else { "▼ " } } else { "" } } "Created" }
                                span { class: "cursor-pointer select-none", onclick: move |_| {
                                        use crate::settings::SortBy; let mut s_sig = sort.write(); let mut new = s_sig.clone(); if new.by == SortBy::Type { new.asc = !new.asc; } else { new.by = SortBy::Type; new.asc = true; } *s_sig = new.clone(); let mut uiw = ui.write(); uiw.sort = Some(new); save_settings(&uiw);
                                    }, { let sv2 = sort.read(); if sv2.by == crate::settings::SortBy::Type { if sv2.asc { "▲ " } else { "▼ " } } else { "" } } "Type" }
                            }
                            div { class: "detail-rows", style: "display:flex; flex-direction:column; gap:4px; padding:4px 6px;",
                                for item in sorted_items.read().iter() {
                                    { let item_clone = item.clone();
                                      let path_disp = item_clone.path.display().to_string();
                                      let name = item_clone.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
                                      let size_txt = item_clone.size.map(|s| format_size(s, DECIMAL)).unwrap_or("-".into());
                                      let modified_txt = item_clone.modified.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or("-".into());
                                      let created_txt = item_clone.created.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or("-".into());
                                      let ext_txt = item_clone.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                                      let selected = selected_path.read().as_ref().map(|p| p == &item_clone.path).unwrap_or(false);
                                      let row_style = if selected { "display:grid; grid-template-columns:56px 1.2fr 2fr .7fr .9fr .9fr .6fr; gap:10px; align-items:center; background:var(--accent-weak); border:1px solid var(--accent); border-radius:6px; padding:6px 10px; cursor:pointer;" } else { "display:grid; grid-template-columns:56px 1.2fr 2fr .7fr .9fr .9fr .6fr; gap:10px; align-items:center; background:var(--panel); border:1px solid var(--stroke); border-radius:6px; padding:6px 10px; cursor:pointer;" };
                                      let desc_opt = ai_descriptions.read().get(&path_disp).cloned();
                                      rsx! { div { key: "det-{path_disp}", class: "detail-row", style: "{row_style}", onclick: move |_| { selected_path.set(Some(item_clone.path.clone())); },
                                            div { style: "width:48px; height:48px; display:flex; align-items:center; justify-content:center; overflow:hidden; border-radius:4px; background:var(--muted);",
                                                if let Some(img) = &item_clone.thumb_data { img { src: "{img}", style: "max-width:100%; max-height:100%; object-fit:cover;" } } else { i { class: "material-icons file-icon", style: "font-size:28px;", { item_clone.icon_name() } } }
                                            }
                                            div { class: "ellipsis", title: "{name}", style: "font-size:13px; font-weight:600; display:flex; flex-direction:column; gap:2px;",
                                                span { "{name}" }
                                                if let Some(desc) = desc_opt { span { class: "text-11px text-weak truncate", style: "max-width:240px;", "{desc}" } }
                                            }
                                            code { class: "ellipsis path", title: "{path_disp}", style: "font-size:11px; opacity:.75;", "{path_disp}" }
                                            span { style: "font-size:12px;", "{size_txt}" }
                                            span { style: "font-size:12px;", "{modified_txt}" }
                                            span { style: "font-size:12px;", "{created_txt}" }
                                            span { style: "font-size:12px;", "{ext_txt}" }
                                        } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Right preview pane
            aside { 
                class: "bg-panel border-l border-stroke", 
                style: "{preview_style}",
                
                if !*preview_collapsed.read() {
                    div { class: "h-full flex flex-col",
                        // Preview header
                        div { class: "flex items-center justify-between px-3 py-2 border-b border-stroke",
                            h3 { class: "text-lg font-semibold", "Preview" }
                            button { 
                                class: "btn", 
                                title: "Close preview",
                                onclick: move |_| { 
                                    preview_collapsed.set(true); 
                                    let mut s = ui.write(); 
                                    s.preview_collapsed = true; 
                                    save_settings(&s); 
                                },
                                i { class: "material-icons", "close" }
                            }
                        }
                        
                        // Preview content
                        div { class: "flex-1 p-4 overflow-y-auto",
                            if let Some(selected) = selected_path.read().clone() {
                                div { class: "space-y-4",
                                    // File name
                                    h4 { 
                                        class: "text-lg font-medium truncate",
                                        title: "{selected.file_name().and_then(|f| f.to_str()).unwrap_or(\"\")}",
{selected.file_name().and_then(|f| f.to_str()).unwrap_or("")}
                                    }
                                    
                                    // File path
                                    p { 
                                        class: "text-sm text-weak break-all",
                                        "{selected.display()}"
                                    }
                                    
                                    // File thumbnail or icon
                                    div { class: "flex justify-center py-4",
                                                    if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                            if let Some(thumb) = &item.thumb_data {
                                                img { 
                                                    class: "max-w-full max-h-48 rounded-lg border border-stroke",
                                                    src: "{thumb}",
                                                    alt: "Preview"
                                                }
                                            } else {
                                                i { 
                                                    class: "material-icons text-6xl text-weak",
                                                    "{item.icon_name()}"
                                                }
                                            }
                                                    } else if *ai_search_active.read() {
                                                        // Fallback: look in AI search results for thumbnail & description
                                                        if let Some(ai_item) = ai_search_results.read().iter().find(|f| f.path == selected.display().to_string()) {
                                                            if let Some(t) = ai_item.thumb_b64.clone().or(ai_item.thumbnail_path.clone()) {
                                                                img { class: "max-w-full max-h-48 rounded-lg border border-stroke", src: "{t}", alt: "Preview" }
                                                            } else {
                                                                i { class: "material-icons text-6xl text-weak", match ai_item.file_type.as_str() { "image" => "photo", "video" => "smart_display", _ => "insert_drive_file" } }
                                                            }
                                                        } else { i { class: "material-icons text-6xl text-weak", "insert_drive_file" } }
                                        } else {
                                            i { class: "material-icons text-6xl text-weak", "insert_drive_file" }
                                        }
                                    }
                                    
                                    // File metadata (with AI description if available)
                                    if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                        div { class: "space-y-2 text-sm",
                                            if let Some(desc) = ai_descriptions.read().get(&selected.display().to_string()) { div { class: "p-2 rounded-md bg-muted border border-stroke text-11px leading-snug",
                                                span { class: "font-semibold text-accent", "AI Description:" }
                                                p { class: "mt-1", "{desc}" }
                                            } }
                                            if ai_descriptions.read().get(&selected.display().to_string()).is_none() && ai_search_engine.read().is_some() && *ai_model_ready.read() {
                                                { let path_for_gen = selected.display().to_string();
                                                    rsx!(div { class: "p-2 rounded-md bg-muted border border-stroke text-11px leading-snug flex flex-col gap-2",
                                                        span { class: "font-semibold text-accent", "AI Description:" }
                                                        span { class: "text-weak", "No description yet." }
                                                        button { class: "btn text-10px w-min", onclick: move |_| {
                                                            if let Some(engine) = ai_search_engine.read().clone() {
                                                                let path_target = path_for_gen.clone();
                                                                let mut desc_map2 = ai_descriptions.clone();
                                                                spawn(async move {
                                                                    if let Ok(Some(desc)) = engine.generate_description_for_path(&path_target, true).await { desc_map2.write().insert(path_target.clone(), desc); }
                                                                });
                                                            }
                                                        }, i { class: "material-icons text-sm", "bolt" } span { " Generate" } }
                                                    })
                                                }
                                            }
                                            if let Some(size) = item.size {
                                                div { class: "flex justify-between",
                                                    span { class: "text-weak", "Size:" }
                                                    span { "{format_size(size, DECIMAL)}" }
                                                }
                                            }
                                            
                                            if let Some(modified) = item.modified {
                                                { let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();
                                                    rsx! {
                                                        div { class: "flex justify-between",
                                                            span { class: "text-weak", "Modified:" }
                                                            span { "{modified_str}" }
                                                        }
                                                    }
                                                }
                                            }
                                            
                                            if let Some(created) = item.created {
                                                { let created_str = created.format("%Y-%m-%d %H:%M").to_string();
                                                    rsx! {
                                                        div { class: "flex justify-between",
                                                            span { class: "text-weak", "Created:" }
                                                            span { "{created_str}" }
                                                        }
                                                    }
                                                }
                                            }
                                            
                                            if let Some(ext) = selected.extension() {
                                                div { class: "flex justify-between",
                                                    span { class: "text-weak", "Type:" }
                                                    span { "{ext.to_str().unwrap_or(\"\")}" }
                                                }
                                            }
                                        }
                                    } else if *ai_search_active.read() {
                                        // Selected item was from AI results only
                                        if let Some(ai_item) = ai_search_results.read().iter().find(|f| f.path == selected.display().to_string()) {
                                            div { class: "space-y-2 text-sm",
                                                if let Some(desc) = ai_item.description.clone() { div { class: "p-2 rounded-md bg-muted border border-stroke text-11px leading-snug", span { class: "font-semibold text-accent", "AI Description:" } p { class: "mt-1", "{desc}" } } }
                                                div { class: "flex justify-between", span { class: "text-weak", "Type:" } span { "{ai_item.file_type}" } }
                                                div { class: "flex justify-between", span { class: "text-weak", "Path:" } span { class: "break-all", "{selected.display()}" } }
                                            }
                                        }
                                    }
                                    
                                    // Action buttons
                                    div { class: "pt-4 space-y-2",
                                        { let selected_for_open = selected.clone();
                                          let selected_for_folder = selected.clone();
                                          rsx! {
                                            button { 
                                                class: "w-full btn bg-accent text-white hover:bg-accent-dark",
                                                onclick: move |_| { let _ = open::that(&selected_for_open); },
                                                i { class: "material-icons mr-2", "open_in_new" }
                                                "Open File"
                                            }
                                            
                                            button { 
                                                class: "w-full btn bg-muted hover:bg-stroke",
                                                onclick: move |_| { 
                                                    if let Some(parent) = selected_for_folder.parent() {
                                                        let _ = open::that(parent);
                                                    }
                                                },
                                                i { class: "material-icons mr-2", "folder_open" }
                                                "Show in Folder"
                                            }
                                          }
                                        }
                                    }
                                }
                            } else {
                                div { class: "text-center py-8 text-weak",
                                    i { class: "material-icons text-4xl mb-2 opacity-50", "preview" }
                                    p { "Select a file to preview" }
                                }
                            }
                        }
                    }
                }
                
                // Preview resize handle (on the left edge of preview pane)
                if !*preview_collapsed.read() {
                    div { 
                        class: "resize-handle", 
                        style: "position:absolute; top:0; left:-3px; width:6px; height:100%; cursor: ew-resize;",
                        onmousedown: move |evt| { 
                            resizing_preview.set(Some((evt.client_coordinates().x as i32, *preview_width.read()))); 
                        }
                    }
                }
            }
        }
    }
    }
}

fn shallow_should_scan(root: &std::path::Path) -> bool {
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if let Ok(ft) = e.file_type() { if ft.is_file() { return true; } }
        }
    }
    false
}

// small wrapper to keep export in this file
pub fn app_export_csv(items: &[crate::types::FoundFile]) -> Result<(), String> {
    let file_path = match rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("media_results.csv").save_file() { Some(p) => p, None => return Ok(()) };
    let mut wtr = csv::Writer::from_path(&file_path).map_err(|e| e.to_string())?;
    wtr.write_record(["path","kind","modified","created","size_bytes"]).map_err(|e| e.to_string())?;
    for it in items {
        let kind = match it.kind { crate::types::MediaKind::Image => "image", crate::types::MediaKind::Video => "video", crate::types::MediaKind::Other => "other" };
        let modified = it.modified.map(|d| d.to_rfc3339()).unwrap_or_default();
        let created = it.created.map(|d| d.to_rfc3339()).unwrap_or_default();
        let size = it.size.unwrap_or(0).to_string();
        wtr.write_record([it.path.display().to_string(), kind.to_string(), modified, created, size]).map_err(|e| e.to_string())?;
    }
    wtr.flush().map_err(|e| e.to_string())?;
    Ok(())
}
