use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem};
use dioxus_primitives::calendar::{Calendar, CalendarGrid, CalendarHeader, CalendarNavigation, CalendarPreviousMonthButton, CalendarNextMonthButton, CalendarSelectMonth, CalendarSelectYear};
use crate::utilities::scan::{cancel_scan, next_scan_id, global_scan_sender, spawn_scan};
use crate::{settings::{UiSettings, save_settings}, utilities::types::ViewMode};
use dioxus_primitives::switch::{Switch, SwitchThumb};
use dioxus_primitives::separator::Separator;
use std::collections::{BTreeSet, BTreeMap};
use time::{Date, OffsetDateTime};
use dioxus::prelude::*;
use std::path::PathBuf;
use chrono::Utc;

#[derive(Props, PartialEq, Clone)]
pub struct NewNavbarProps {
    pub ui: Signal<UiSettings>,
    pub view_mode: Signal<ViewMode>,
    pub preview_collapsed: Signal<bool>,
    pub qa_collapsed: Signal<bool>,
    pub drives_collapsed: Signal<bool>,
    pub results: Signal<crate::utilities::types::ScanResults>,
    pub app_view: Signal<crate::app::AppView>,
    pub error: Signal<Option<String>>,
    pub filters: Signal<crate::utilities::types::Filters>,
    pub scan_generation: Signal<u64>,
    pub scanning: Signal<bool>,
    pub dir_items: Signal<Vec<crate::utilities::types::DirItem>>,
    pub progress: Signal<Option<(usize, usize)>>,
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>,
    pub auto_indexing: Signal<bool>,
    pub search_text: Signal<String>,
    pub ai_search_results: Signal<Vec<crate::database::FileMetadata>>,
    pub ai_search_active: Signal<bool>,
    pub group_by_category: Signal<bool>,
    pub selected_path: Signal<Option<std::path::PathBuf>>,
    pub nav_history: Signal<Vec<std::path::PathBuf>>,
    // Additional signals migrated from FiltersBar
    pub recursive_current: Signal<bool>,
    pub only_subdirs: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_finished: Signal<Option<std::time::Instant>>,
    pub ext_filters: Signal<BTreeSet<String>>,
    pub ext_enabled: Signal<BTreeMap<String,bool>>,
    pub excluded_dirs: Signal<BTreeSet<PathBuf>>,
    // bulk generation progress (lifted to App and passed down)
    pub bulk_progress: Signal<(usize,usize)>,
    pub bulk_generating: Signal<bool>,
}

#[allow(non_snake_case)]
#[component]
pub fn NewNavbar(props: NewNavbarProps) -> Element {
    let NewNavbarProps { mut ui, mut view_mode, mut preview_collapsed, mut qa_collapsed, mut drives_collapsed, mut results, app_view: mut app_view, error, mut filters, scan_generation, mut scanning, mut dir_items, progress, ai_search_engine, selected_paths, mut auto_indexing, mut search_text, mut ai_search_results, mut ai_search_active, mut group_by_category, selected_path: _selected_path, mut nav_history, mut recursive_current, mut only_subdirs, mut scan_started, mut scan_finished, ext_filters, mut ext_enabled, mut excluded_dirs, bulk_progress, bulk_generating } = props;

    let mut exporting = use_signal(|| false);
    let mut prefs_open = use_signal(|| false);
    let mut date_menu_after_open = use_signal(|| false);
    let mut date_menu_before_open = use_signal(|| false);
    let mut calendar_after_view = use_signal(|| OffsetDateTime::now_utc().date());
    let mut calendar_before_view = use_signal(|| OffsetDateTime::now_utc().date());
    let mut recursive_settings_open = use_signal(|| false);
    let mut excluded_dirs_input = use_signal(|| String::new());
    let mut excluded_exts_input = use_signal(|| String::new());
    let mut rec_after_input = use_signal(|| String::new());
    let mut rec_before_input = use_signal(|| String::new());
    // categories_available now handled in column header dropdowns; leaving unused here intentionally

    // Files struct for standalone scanning prototype (will eventually replace older scan pipeline)
    let mut files = use_signal(crate::utilities::files::Files::new);

    // Resource: directory items (shallow listing) reacts to root changes when NOT in recursive mode
    let mut last_root_for_list = use_signal(|| std::path::PathBuf::new());
    let dir_items_resource = use_resource({
        let filters = filters.clone();
        move || {
            let filters = filters.read().clone();
            async move {
                if filters.root != std::path::PathBuf::new() && filters.root != last_root_for_list.read().clone() {
                    last_root_for_list.set(filters.root.clone());
                }
                crate::utilities::explorer::list_dir_items(filters.root.clone()).await.ok()
            }
        }
    });

    if let Some(Some(items)) = dir_items_resource.read().as_ref() { // Option<Result>
        if dir_items.read().len() != items.len() { dir_items.set(items.clone()); }
    }

    // Legacy scan resource (original reactive scanner) kept for comparison.
    let filters_sub = filters.clone();
    let recursive_sub = recursive_current.clone();
    let mut scan_generation_sub = scan_generation.clone();
    let mut scanning_sub = scanning.clone();
    let mut results_sub = results.clone();
    let mut progress_sub = progress.clone();
    let mut scan_started_sub = scan_started.clone();
    let mut scan_finished_sub = scan_finished.clone();
    let _scan_resource = use_resource(move || {
        let filters_snapshot = filters_sub.read().clone();
        let recursive_flag = *recursive_sub.read();
        async move {
            cancel_scan();
            let scan_id = next_scan_id();
            scan_generation_sub.set(scan_id);
            scanning_sub.set(true);
            progress_sub.set(None);
            results_sub.set(crate::utilities::types::ScanResults::default());
            scan_started_sub.set(Some(std::time::Instant::now()));
            scan_finished_sub.set(None);
            let tx = global_scan_sender();
            spawn(async move { let _ = spawn_scan(filters_snapshot, tx, recursive_flag, scan_id).await; });
            Ok::<(), ()>(())
        }
    });

    rsx! {
        nav { class: "app-nav flex items-center gap-3 px-2 bg-panel border-b border-stroke h-12",
            // Back / Up navigation controls
            div { class: "flex items-center gap-1 pr-1",
                button {
                    class: "btn px-2 py-1",
                    onclick: move |_| {
                        files.write().go_up();
                    },
                    i { class: "material-icons text-[18px] opacity-80", "logout" }
                }
                button {
                    class: "btn px-2 py-1",
                    disabled: nav_history.read().is_empty(),
                    onclick: move |_| {
                        if nav_history.read().is_empty() {
                            return;
                        }
                        let mut stack = nav_history.read().clone();
                        if let Some(prev) = stack.pop() {
                            nav_history.set(stack);
                            {
                                let mut f = filters.write();
                                f.root = prev.clone();
                            }
                            {
                                let mut s = ui.write();
                                s.last_root = Some(prev.display().to_string());
                                save_settings(&s);
                            }
                            only_subdirs.set(false);
                            recursive_current.set(false);
                            scan_started.set(Some(std::time::Instant::now()));
                            scan_finished.set(None);
                        }
                    },
                    i { class: "material-icons", "arrow_back" }
                }
                button {
                    class: "btn px-2 py-1",
                    disabled: filters.read().root.parent().is_none(),
                    onclick: move |_| {
                        let current = filters.read().root.clone();
                        if let Some(parent) = current.parent().map(|p| p.to_path_buf()) {
                            let mut stack = nav_history.read().clone();
                            stack.push(current.clone());
                            nav_history.set(stack);
                            {
                                let mut f = filters.write();
                                f.root = parent.clone();
                            }
                            {
                                let mut s = ui.write();
                                s.last_root = Some(parent.display().to_string());
                                save_settings(&s);
                            }
                            only_subdirs.set(false);
                            recursive_current.set(false);
                            scan_started.set(Some(std::time::Instant::now()));
                            scan_finished.set(None);
                        }
                    },
                    i { class: "material-icons text-[18px] opacity-80", "arrow_upward" }
                }
            }
            // Top menus rebuilt with DropdownMenuItem + on_select
            div { class: "flex gap-1 flex-1 items-center",
                // File
                DropdownMenu { class: "menubar",
                    DropdownMenuTrigger { class: "menubar-trigger", "File" }
                    DropdownMenuContent { class: "menubar-content",
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "export",
                            index: 0usize,
                            disabled: results.read().items.is_empty() || *exporting.read(),
                            on_select: move |_| {
                                if results.read().items.is_empty() || *exporting.read() {
                                    return;
                                }
                                exporting.set(true);
                                let rows = results.read().items.clone();
                                let mut exporting_flag = exporting.clone();
                                let mut err_sig = error.clone();
                                spawn(async move {
                                    let res = crate::app::app_export_csv(&rows);
                                    if let Err(e) = res {
                                        err_sig.set(Some(e));
                                    }
                                    exporting_flag.set(false);
                                });
                            },
                            span { class: "inline-flex items-center gap-1",
                                i { class: "material-icons text-[14px] opacity-70",
                                    "download"
                                }
                                span { "Export CSV" }
                            }
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "prefs",
                            index: 1usize,
                            on_select: move |_| prefs_open.set(true),
                            span { class: "inline-flex items-center gap-1",
                                i { class: "material-icons text-[14px] opacity-70",
                                    "settings"
                                }
                                span { "Preferences" }
                            }
                        }
                    }
                }
                // View
                DropdownMenu { class: "menubar",
                    DropdownMenuTrigger { class: "menubar-trigger", "View" }
                    DropdownMenuContent { class: "menubar-content flex flex-col p-1 min-w-[190px] gap-0.5",
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "icons",
                            index: 0usize,
                            on_select: move |_| {
                                view_mode.set(ViewMode::Icons);
                                let mut s = ui.write();
                                s.view_mode = Some("icons".into());
                                save_settings(&s);
                            },
                            "Icons"
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "details",
                            index: 1usize,
                            on_select: move |_| {
                                view_mode.set(ViewMode::Details);
                                let mut s = ui.write();
                                s.view_mode = Some("details".into());
                                save_settings(&s);
                            },
                            "Details"
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "toggle_preview",
                            index: 2usize,
                            on_select: move |_| {
                                let new_val = !*preview_collapsed.read();
                                preview_collapsed.set(new_val);
                                let mut s = ui.write();
                                s.preview_collapsed = new_val;
                                save_settings(&s);
                            },
                            {if *preview_collapsed.read() { "Show Preview" } else { "Hide Preview" }}
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "toggle_left",
                            index: 3usize,
                            on_select: move |_| {
                                let hide = !(*qa_collapsed.read() && *drives_collapsed.read());
                                qa_collapsed.set(hide);
                                drives_collapsed.set(hide);
                                let mut s = ui.write();
                                s.qa_collapsed = hide;
                                s.drives_collapsed = hide;
                                save_settings(&s);
                            },
                            {
                                if *qa_collapsed.read() && *drives_collapsed.read() {
                                    "Show Left"
                                } else {
                                    "Hide Left"
                                }
                            }
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "group_cat",
                            index: 4usize,
                            on_select: move |_| {
                                let cur = *group_by_category.read();
                                group_by_category.set(!cur);
                            },
                            {
                                if *group_by_category.read() {
                                    "Ungroup Categories"
                                } else {
                                    "Group by Category"
                                }
                            }
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "debug_toggle",
                            index: 5usize,
                            on_select: move |_| {
                                let next = match *app_view.read() { crate::app::AppView::Explorer => crate::app::AppView::DebugDb, crate::app::AppView::DebugDb => crate::app::AppView::Explorer };
                                app_view.set(next);
                            },
                            {
                                if *app_view.read() == crate::app::AppView::DebugDb { "Back to Explorer" } else { "Debug View" }
                            }
                        }
                    }
                }
                // Scan
                DropdownMenu { class: "menubar",
                    DropdownMenuTrigger { class: "menubar-trigger", "Scan" }
                    DropdownMenuContent { class: "menubar-content flex flex-col p-1 min-w-[180px] gap-0.5",
                        // Manual trigger for new fast scan (Files-based) inside menu
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "fast_scan_new",
                            index: 99usize,
                            disabled: files.read().scanning,
                            on_select: move |_| {
                                if files.read().scanning {
                                    return;
                                }
                                let root = filters.read().root.clone();
                                let include_images = filters.read().include_images;
                                let include_videos = filters.read().include_videos;
                                let preloaded: Vec<crate::utilities::types::FoundFile> = Vec::new();
                                let skip: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
                                if let Some(engine) = ai_search_engine.read().clone() {
                                    let engine_clone = engine.clone();
                                    let mut files_sig = files.clone();
                                    let mut results_sig2 = results.clone();
                                    let include_images2 = include_images;
                                    let include_videos2 = include_videos;
                                    spawn(async move {
                                        let cache_guard = engine_clone.files.lock().await;
                                        let mut pre: Vec<crate::utilities::types::FoundFile> = Vec::new();
                                        let mut skip_set: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
                                        for meta in cache_guard.iter() {
                                            if meta.path.starts_with(&root.display().to_string()) {
                                                let pb = std::path::PathBuf::from(&meta.path);
                                                skip_set.insert(pb.clone());
                                                pre.push(crate::utilities::types::FoundFile {
                                                    path: pb,
                                                    modified: meta.modified,
                                                    created: meta.created,
                                                    size: Some(meta.size),
                                                    kind: match meta.file_type.as_str() {
                                                        "image" => crate::utilities::types::MediaKind::Image,
                                                        "video" => crate::utilities::types::MediaKind::Video,
                                                        _ => crate::utilities::types::MediaKind::Other,
                                                    },
                                                    thumb_data: meta
                                                        .thumb_b64
                                                        .clone()
                                                        .or(meta.thumbnail_path.clone()),
                                                });
                                            }
                                        }
                                        results_sig2
                                            .set(crate::utilities::types::ScanResults {
                                                items: pre.clone(),
                                            });
                                        files_sig
                                            .write()
                                            .begin_scan_with_skip(
                                                root,
                                                include_images2,
                                                include_videos2,
                                                pre,
                                                skip_set,
                                            );
                                    });
                                } else {
                                    results
                                        .set(crate::utilities::types::ScanResults {
                                            items: preloaded.clone(),
                                        });
                                    files
                                        .write()
                                        .begin_scan_with_skip(
                                            root,
                                            include_images,
                                            include_videos,
                                            preloaded,
                                            skip,
                                        );
                                }
                                let mut poll_signal = files.clone();
                                let mut results_sig = results.clone();
                                let mut scanning_flag = scanning.clone();
                                spawn(async move {
                                    use tokio::time::{sleep, Duration};
                                    loop {
                                        sleep(Duration::from_millis(60)).await;
                                        let mut w = poll_signal.write();
                                        if let Some(delta) = w.poll_scan() {
                                            let dlen = delta.len();
                                            if dlen > 0 {
                                                log::info!(
                                                    "[ui-fast-scan-menu] appending delta_len={dlen} total_now={}",
                                                    w.scan_results.len()
                                                );
                                                let mut cur = results_sig.read().items.clone();
                                                cur.extend(delta);
                                                results_sig
                                                    .set(crate::utilities::types::ScanResults {
                                                        items: cur,
                                                    });
                                            } else {
                                                log::info!("[ui-fast-scan-menu] empty delta returned");
                                            }
                                        }
                                        if !w.scanning {
                                            break;
                                        }
                                    }
                                    log::info!(
                                        "[ui-fast-scan-menu] polling loop exit scanning=false final_len={} elapsed_ms={}",
                                        poll_signal.read().scan_results.len(), poll_signal.read().started_at
                                        .elapsed().as_millis()
                                    );
                                    let reader = poll_signal.read();
                                    let rows: Vec<crate::Thumbnail> = reader
                                        .scan_results
                                        .iter()
                                        .skip(reader.last_ui_len)
                                        .map(|f| {
                                            let file_type = f
                                                .path
                                                .extension()
                                                .and_then(|e| e.to_str())
                                                .map(|s| s.to_ascii_lowercase());
                                            let ft_string = if let Some(ext) = file_type.clone() {
                                                if crate::utilities::types::IMAGE_EXTS.iter().any(|e| *e == ext)
                                                {
                                                    "image".to_string()
                                                } else if crate::utilities::types::VIDEO_EXTS
                                                    .iter()
                                                    .any(|e| *e == ext)
                                                {
                                                    "video".to_string()
                                                } else {
                                                    ext
                                                }
                                            } else {
                                                "other".into()
                                            };
                                            crate::Thumbnail {
                                                db_created: Utc::now().into(),
                                                path: f.path.display().to_string(),
                                                filename: f
                                                    .path
                                                    .file_name()
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or("")
                                                    .to_string(),
                                                file_type: ft_string,
                                                size: f.size.unwrap_or(0),
                                                description: None,
                                                caption: None,
                                                tags: Vec::new(),
                                                category: None,
                                                embedding: None,
                                                thumbnail_b64: f.thumb_data.clone(),
                                                modified: if let Some(date) = f.modified {
                                                    Some(date.to_utc().into())
                                                } else {
                                                    Some(Utc::now().into())
                                                },
                                                hash: None,
                                            }
                                        })
                                        .collect();
                                    if !rows.is_empty() {
                                        log::info!(
                                            "[ui-fast-scan-menu] saving batch of {} thumbnails", rows.len()
                                        );
                                        if let Err(e) = crate::database::save_thumbnail_batch(rows).await {
                                            log::warn!("Failed saving scan batch: {e}");
                                        }
                                    }
                                    scanning_flag.set(false);
                                });
                            },
                            span { class: "inline-flex items-center gap-1",
                                i { class: "material-icons text-[14px] opacity-70",
                                    "bolt"
                                }
                                span { "Fast Scan (New)" }
                            }
                        }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "recursive",
                            index: 0usize,
                            disabled: *scanning.read(),
                            on_select: {
                                let filters_sig = filters.clone();
                                let mut recursive_current_sig = recursive_current.clone();
                                let mut scan_started_sig = scan_started.clone();
                                let mut scan_finished_sig = scan_finished.clone();
                                move |_| {
                                    if *scanning.read() {
                                        return;
                                    }
                                    recursive_current_sig.set(true);
                                    scan_started_sig.set(Some(std::time::Instant::now()));
                                    scan_finished_sig.set(None);
                                    let _root = filters_sig.read().root.clone();
                                }
                            },
                            "Recursive Scan"
                        }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "recursive_settings",
                            index: 3usize,
                            on_select: move |_| {
                                let f = filters.read().clone();
                                excluded_dirs_input
                                    .set(
                                        f
                                            .recursive_excluded_dirs
                                            .iter()
                                            .map(|p| p.display().to_string())
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    );
                                excluded_exts_input
                                    .set(
                                        f.recursive_excluded_exts.iter().cloned().collect::<Vec<_>>().join(","),
                                    );
                                rec_after_input.set(f.recursive_modified_after.clone().unwrap_or_default());
                                rec_before_input.set(f.recursive_modified_before.clone().unwrap_or_default());
                                recursive_settings_open.set(true);
                            },
                            "Recursive Settings..."
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "bulk",
                            index: 2usize,
                            disabled: *bulk_generating.read() || ai_search_engine.read().is_none(),
                            on_select: move |_| {
                                crate::ai::bulk::spawn_bulk_generate(
                                    ai_search_engine.read().clone(),
                                    results.read().items.clone(),
                                    ui.read().ai_prompt_template.clone(),
                                    bulk_progress.clone(),
                                    bulk_generating.clone(),
                                    error.clone(),
                                    ui.read().overwrite_descriptions,
                                );
                            },
                            {if *bulk_generating.read() { "Generating..." } else { "Bulk Generate" }}
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "cancel",
                            index: 1usize,
                            disabled: !*scanning.read(),
                            on_select: move |_| {
                                if !*scanning.read() {
                                    return;
                                }
                                cancel_scan();
                                scanning.set(false);
                            },
                            "Cancel Scan"
                        }
                    }
                }
                // AI
                DropdownMenu { class: "menubar",
                    DropdownMenuTrigger { class: "menubar-trigger", "AI" }
                    DropdownMenuContent { class: "menubar-content flex flex-col p-1 min-w-[200px] gap-0.5",
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "toggle_ai",
                            index: 0usize,
                            on_select: move |_| {
                                let active_now = *ai_search_active.read();
                                let new_state = !active_now;
                                ai_search_active.set(new_state);
                                if !new_state {
                                    ai_search_results.set(Vec::new());
                                }
                            },
                            {if *ai_search_active.read() { "Disable AI Search" } else { "Enable AI Search" }}
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "index_selected",
                            index: 1usize,
                            disabled: ai_search_engine.read().is_none(),
                            on_select: move |_| {
                                if let Some(engine) = ai_search_engine.read().as_ref() {
                                    engine
                                        .auto_descriptions_enabled
                                        .store(*auto_indexing.read(), std::sync::atomic::Ordering::Relaxed);
                                    let selected = selected_paths.read().clone();
                                    let engine2 = engine.clone();
                                    let mut err_sig = error.clone();
                                    spawn(async move {
                                        let mut queued = 0usize;
                                        for p in selected.iter() {
                                            if let Ok(md) = std::fs::metadata(p) {
                                                let ext = p
                                                    .extension()
                                                    .and_then(|e| e.to_str())
                                                    .unwrap_or("")
                                                    .to_ascii_lowercase();
                                                let kind = if crate::utilities::types::IMAGE_EXTS
                                                    .iter()
                                                    .any(|e| *e == ext)
                                                {
                                                    crate::utilities::types::MediaKind::Image
                                                } else if crate::utilities::types::VIDEO_EXTS
                                                    .iter()
                                                    .any(|e| *e == ext)
                                                {
                                                    crate::utilities::types::MediaKind::Video
                                                } else {
                                                    crate::utilities::types::MediaKind::Other
                                                };
                                                let ff = crate::utilities::types::FoundFile {
                                                    path: p.clone(),
                                                    modified: None,
                                                    created: None,
                                                    size: Some(md.len()),
                                                    kind,
                                                    thumb_data: None,
                                                };
                                                if engine2
                                                    .enqueue_index(crate::ai::found_file_to_metadata(&ff))
                                                    .await
                                                {
                                                    queued += 1;
                                                }
                                            }
                                        }
                                        if queued == 0 {
                                            err_sig
                                                .set(
                                                    Some(
                                                        "No files indexed (selection empty or unsupported)".into(),
                                                    ),
                                                );
                                        }
                                    });
                                }
                            },
                            "Index Selected"
                        }
                        Separator { class: "separator", horizontal: true }
                        DropdownMenuItem::<&'static str> {
                            class: "menubar-item",
                            value: "toggle_auto_index",
                            index: 2usize,
                            on_select: move |_| {
                                let new = !*auto_indexing.read();
                                auto_indexing.set(new);
                                if let Some(engine) = ai_search_engine.read().as_ref() {
                                    engine
                                        .auto_descriptions_enabled
                                        .store(new, std::sync::atomic::Ordering::Relaxed);
                                }
                            },
                            {if *auto_indexing.read() { "Disable Auto Index" } else { "Enable Auto Index" }}
                        }
                    }
                }
                // Filters (panel)
                DropdownMenu { class: "menubar",
                    DropdownMenuTrigger { class: "menubar-trigger", "Filters" }
                    DropdownMenuContent { class: "menubar-content flex flex-col gap-3 w-[360px] max-h-[520px] overflow-auto p-2",
                        // Media toggles
                        div { class: "grid grid-cols-2 gap-2 text-10px",
                            div { class: "flex justify-between gap-1",
                                span { class: "text-8px text-weak", "Img" }
                                Switch {
                                    class: "switch",
                                    checked: filters.read().include_images,
                                    on_checked_change: move |v: bool| {
                                        let root = {
                                            let mut f = filters.write();
                                            f.include_images = v;
                                            f.root.clone()
                                        };
                                        scan_started.set(Some(std::time::Instant::now()));
                                        scan_finished.set(None);
                                        let rec = *recursive_current.read();
                                        if crate::app::shallow_should_scan(&root) || rec {
                                            only_subdirs.set(false);
                                        }
                                    },
                                    SwitchThumb { class: "switch-thumb" }
                                }
                                span { class: "text-8px text-weak", "Vid" }
                                Switch {
                                    class: "switch",
                                    checked: filters.read().include_videos,
                                    on_checked_change: move |v: bool| {
                                        let root = {
                                            let mut f = filters.write();
                                            f.include_videos = v;
                                            f.root.clone()
                                        };
                                        scan_started.set(Some(std::time::Instant::now()));
                                        scan_finished.set(None);
                                        let rec = *recursive_current.read();
                                        if crate::app::shallow_should_scan(&root) || rec {
                                            only_subdirs.set(false);
                                        }
                                    },
                                    SwitchThumb { class: "switch-thumb" }
                                }
                            }
                            Separator { class: "separator", horizontal: true }
                            div { class: "flex items-center gap-1",
                                span { class: "text-8px text-weak", "Thumbs" }
                                Switch {
                                    class: "switch",
                                    checked: filters.read().only_with_thumb,
                                    on_checked_change: move |v: bool| {
                                        filters.write().only_with_thumb = v;
                                    },
                                    SwitchThumb { class: "switch-thumb" }
                                }
                            }
                        }
                        // Description-only toggle
                        div { class: "flex items-center gap-2",
                            span { class: "text-8px text-weak", "Has Desc" }
                            Switch {
                                class: "switch",
                                checked: filters.read().only_with_description,
                                on_checked_change: move |v: bool| {
                                    let root = {
                                        let mut f = filters.write();
                                        f.only_with_description = v;
                                        f.root.clone()
                                    };
                                    let rec = *recursive_current.read();
                                    if crate::app::shallow_should_scan(&root) || rec {}
                                },
                                SwitchThumb { class: "switch-thumb" }
                            }
                        }
                        // Category single-select removed temporarily (to be re-added)
                        {
                            rsx! {
                                Fragment {}
                            }
                        }
                        // Extensions
                        if !ext_filters.read().is_empty() {
                            div { class: "flex flex-wrap gap-2",
                                for ext in ext_filters.read().iter() {
                                    {
                                        let ext_name = ext.clone();
                                        let active = *ext_enabled.read().get(&ext_name).unwrap_or(&true);
                                        let inactive_cls = if active { "" } else { " inactive" };
                                        rsx! {
                                            div {
                                                key: "ext-{ext_name}",
                                                class: "ext-chip flex items-center gap-1 text-11px px-1.5 py-0.5 rounded-md border cursor-pointer bg-muted/40{inactive_cls}",
                                                span { ".{ext_name}" }
                                                Switch {
                                                    class: "switch",
                                                    checked: active,
                                                    on_checked_change: move |v: bool| {
                                                        let mut map = ext_enabled.write();
                                                        map.insert(ext_name.clone(), v);
                                                    },
                                                    SwitchThumb { class: "switch-thumb" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !excluded_dirs.read().is_empty() {
                            div { class: "flex items-center gap-2 mt-1",
                                span { class: "text-11px", "Excluded: {excluded_dirs.read().len()} dirs" }
                                button {
                                    class: "btn text-10px px-2 py-0.5",
                                    onclick: move |_| excluded_dirs.write().clear(),
                                    "Clear"
                                }
                            }
                        }
                        // Modified After
                        DropdownMenu { class: "menubar w-full", default_open: false,
                            DropdownMenuTrigger { class: "menubar-trigger w-full flex justify-between",
                                button {
                                    class: "flex-1 text-left ",
                                    style: "background: none; color: white; border: none",
                                    onclick: move |_| date_menu_after_open.set(!date_menu_after_open()),
                                    {filters.read().modified_after.clone().unwrap_or_else(|| "After: (any)".into())}
                                }
                            }
                            if date_menu_after_open() {
                                DropdownMenuContent { class: "menubar-content p-2",
                                    Calendar {
                                        selected_date: filters.read().modified_after.as_ref().and_then(|s| parse_date_str(s)),
                                        on_date_change: move |d: Option<Date>| {
                                            let new_val = d.map(|dd| dd.to_string());
                                            {
                                                let mut f = filters.write();
                                                f.modified_after = new_val;
                                            }
                                            date_menu_after_open.set(false);
                                        },
                                        view_date: calendar_after_view(),
                                        on_view_change: move |new_view: Date| calendar_after_view.set(new_view),
                                        min_date: Date::from_calendar_date(1995, time::Month::July, 21).unwrap(),
                                        max_date: Date::from_calendar_date(2035, time::Month::September, 11).unwrap(),
                                        CalendarHeader {
                                            CalendarNavigation {
                                                CalendarPreviousMonthButton {}
                                                CalendarSelectMonth {}
                                                CalendarSelectYear {}
                                                CalendarNextMonthButton {}
                                            }
                                        }
                                        CalendarGrid {}
                                    }
                                    DropdownMenuItem::<&'static str> {
                                        class: "menubar-item",
                                        value: "clear_after",
                                        index: 0usize,
                                        on_select: move |_| {
                                            {
                                                let mut f = filters.write();
                                                f.modified_after = None;
                                            }
                                            date_menu_after_open.set(false);
                                        },
                                        "Clear"
                                    }
                                }
                            }
                        }
                        // Modified Before
                        DropdownMenu { class: "menubar w-full", default_open: false,
                            DropdownMenuTrigger { class: "menubar-trigger w-full flex justify-between",
                                button {
                                    class: "flex-1 text-left",
                                    onclick: move |_| date_menu_before_open.set(!date_menu_before_open()),
                                    {filters.read().modified_before.clone().unwrap_or_else(|| "Before: (any)".into())}
                                }
                            }
                            if date_menu_before_open() {
                                DropdownMenuContent { class: "menubar-content p-2",
                                    Calendar {
                                        selected_date: filters.read().modified_before.as_ref().and_then(|s| parse_date_str(s)),
                                        on_date_change: move |d: Option<Date>| {
                                            let new_val = d.map(|dd| dd.to_string());
                                            {
                                                let mut f = filters.write();
                                                f.modified_before = new_val;
                                            }
                                            date_menu_before_open.set(false);
                                        },
                                        view_date: calendar_before_view(),
                                        on_view_change: move |new_view: Date| calendar_before_view.set(new_view),
                                        min_date: Date::from_calendar_date(1995, time::Month::July, 21).unwrap(),
                                        max_date: Date::from_calendar_date(2035, time::Month::September, 11).unwrap(),
                                        CalendarHeader {
                                            CalendarNavigation {
                                                CalendarPreviousMonthButton {}
                                                CalendarSelectMonth {}
                                                CalendarSelectYear {}
                                                CalendarNextMonthButton {}
                                            }
                                        }
                                        CalendarGrid {}
                                    }
                                    DropdownMenuItem::<&'static str> {
                                        class: "menubar-item",
                                        value: "clear_before",
                                        index: 0usize,
                                        on_select: move |_| {
                                            {
                                                let mut f = filters.write();
                                                f.modified_before = None;
                                            }
                                            date_menu_before_open.set(false);
                                        },
                                        "Clear"
                                    }
                                }
                            }
                        }
                                        // Category chips removed (now provided via column header dropdown)
                    }
                }
                button {
                    class: "btn px-2 py-1 bg-accent/20",
                    title: "Fast Scan (New)",
                    onclick: move |_| {
                        if !files.read().scanning {
                            let root = filters.read().root.clone();
                            let include_images = filters.read().include_images;
                            let include_videos = filters.read().include_videos;
                            let engine_opt = ai_search_engine.read().clone();
                            let mut results_sig = results.clone();
                            let mut files_sig = files.clone();
                            spawn(async move {
                                let mut pre: Vec<crate::utilities::types::FoundFile> = Vec::new();
                                let mut skip_set: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
                                if let Some(engine) = engine_opt {
                                    let guard = engine.files.lock().await;
                                    for meta in guard.iter() {
                                        if meta.path.starts_with(&root.display().to_string()) {
                                            let pb = std::path::PathBuf::from(&meta.path);
                                            skip_set.insert(pb.clone());
                                            pre.push(crate::utilities::types::FoundFile {
                                                path: pb,
                                                modified: meta.modified,
                                                created: meta.created,
                                                size: Some(meta.size),
                                                kind: match meta.file_type.as_str() {
                                                    "image" => crate::utilities::types::MediaKind::Image,
                                                    "video" => crate::utilities::types::MediaKind::Video,
                                                    _ => crate::utilities::types::MediaKind::Other,
                                                },
                                                thumb_data: meta
                                                    .thumb_b64
                                                    .clone()
                                                    .or(meta.thumbnail_path.clone()),
                                            });
                                        }
                                    }
                                }
                                results_sig
                                    .set(crate::utilities::types::ScanResults {
                                        items: pre.clone(),
                                    });
                                files_sig
                                    .write()
                                    .begin_scan_with_skip(
                                        root,
                                        include_images,
                                        include_videos,
                                        pre,
                                        skip_set,
                                    );
                            });
                            let mut poll_signal = files.clone();
                            let mut results_sig = results.clone();
                            let mut scanning_flag = scanning.clone();
                            spawn(async move {
                                use tokio::time::{sleep, Duration};
                                loop {
                                    sleep(Duration::from_millis(100)).await;
                                    let mut w = poll_signal.write();
                                    if let Some(delta) = w.poll_scan() {
                                        let dlen = delta.len();
                                        if dlen > 0 {
                                            log::info!(
                                                "[ui-fast-scan] appending delta_len={dlen} total_now={}", w
                                                .scan_results.len()
                                            );
                                            let mut cur = results_sig.read().items.clone();
                                            cur.extend(delta);
                                            results_sig
                                                .set(crate::utilities::types::ScanResults {
                                                    items: cur,
                                                });
                                        } else {
                                            log::info!("[ui-fast-scan] empty delta returned");
                                        }
                                    }
                                    if !w.scanning {
                                        break;
                                    }
                                }
                                log::info!(
                                    "[ui-fast-scan] polling loop exit scanning=false final_len={} elapsed_ms={}",
                                    poll_signal.read().scan_results.len(), poll_signal.read().started_at
                                    .elapsed().as_millis()
                                );
                                let reader = poll_signal.read();
                                let rows: Vec<crate::Thumbnail> = reader
                                    .scan_results
                                    .iter()
                                    .skip(reader.last_ui_len)
                                    .map(|f| {
                                        let file_type = f
                                            .path
                                            .extension()
                                            .and_then(|e| e.to_str())
                                            .map(|s| s.to_ascii_lowercase());
                                        let ft_string = if let Some(ext) = file_type.clone() {
                                            if crate::utilities::types::IMAGE_EXTS
                                                .iter()
                                                .any(|e| *e == ext)
                                            {
                                                "image".to_string()
                                            } else if crate::utilities::types::VIDEO_EXTS
                                                .iter()
                                                .any(|e| *e == ext)
                                            {
                                                "video".to_string()
                                            } else {
                                                ext
                                            }
                                        } else {
                                            "other".into()
                                        };
                                        crate::Thumbnail {
                                            db_created: Utc::now().into(),
                                            path: f.path.display().to_string(),
                                            filename: f
                                                .path
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            file_type: ft_string,
                                            size: f.size.unwrap_or(0),
                                            description: None,
                                            caption: None,
                                            tags: Vec::new(),
                                            category: None,
                                            embedding: None,
                                            thumbnail_b64: f.thumb_data.clone(),
                                            modified: if let Some(date) = f.modified {
                                                Some(date.to_utc().into())
                                            } else {
                                                Some(Utc::now().into())
                                            },
                                            hash: None,
                                        }
                                    })
                                    .collect();
                                if !rows.is_empty() {
                                    log::info!(
                                        "[ui-fast-scan] saving batch of {} thumbnails", rows.len()
                                    );
                                    if let Err(e) = crate::database::save_thumbnail_batch(rows).await {
                                        log::warn!("Failed saving scan batch: {e}");
                                    }
                                }
                                scanning_flag.set(false);
                            });
                        }
                    },
                    i { class: "material-icons text-[18px] opacity-80", "bolt" }
                }

                div { class: "flex-1 px-2",
                    {
                        let mut draft_path = use_signal(|| filters.read().root.display().to_string());
                        rsx! {
                            input {
                                class: "w-full bg-muted text-var-text border border-stroke rounded-md px-2 py-1 text-11px font-mono",
                                value: draft_path(),
                                oninput: move |e| {
                                    draft_path.set(e.value().to_string());
                                },
                                onkeydown: move |e| {
                                    if e.key() == dioxus::prelude::Key::Enter {
                                        let new_path = draft_path.read().trim().to_string();
                                        if new_path.is_empty() {
                                            return;
                                        }
                                        let new_buf = std::path::PathBuf::from(&new_path);
                                        if new_buf.is_dir() {
                                            {
                                                let current = filters.read().root.clone();
                                                let mut stack = nav_history.read().clone();
                                                if stack.last().map(|p| p != &current).unwrap_or(true) {
                                                    stack.push(current.clone());
                                                    nav_history.set(stack);
                                                }
                                            }
                                            {
                                                let mut f = filters.write();
                                                f.root = new_buf.clone();
                                            }
                                            {
                                                let mut s = ui.write();
                                                s.last_root = Some(new_buf.display().to_string());
                                                save_settings(&s);
                                            }
                                            only_subdirs.set(false);
                                            recursive_current.set(false);
                                            scan_started.set(Some(std::time::Instant::now()));
                                            scan_finished.set(None);
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }
            // Right side
            div { class: "flex items-center gap-2",
                input {
                    class: "w-60 bg-muted text-var-text border border-stroke rounded-md px-2 py-1 text-sm",
                    placeholder: if *ai_search_active.read() { "Describe & Enter" } else { "Search..." },
                    value: "{search_text.read().clone()}",
                    oninput: move |e| {
                        let query = e.value();
                        search_text.set(query.clone());
                        if *ai_search_active.read() {
                            if query.trim().is_empty() {
                                ai_search_results.set(Vec::new());
                                return;
                            }
                            if ai_search_engine.read().is_none() {
                                return;
                            }
                            #[cfg(feature = "surreal")]
                            {
                                let engine = ai_search_engine.read().clone();
                                let mut res_sig = ai_search_results.clone();
                                let q2 = query.clone();
                                spawn(async move {
                                    if let Some(engine) = engine {
                                        if let Ok(r) = engine.search(&q2).await {
                                            res_sig.set(r);
                                        }
                                    }
                                });
                            }
                        }
                    },
                }
                if *ai_search_active.read() {
                    span { class: "text-10px px-2 py-0.5 rounded bg-accent/20 text-accent",
                        "AI"
                    }
                }
                if *bulk_generating.read() {
                    span { class: "text-10px text-weak",
                        {
                            let (d, t) = *bulk_progress.read();
                            format!("Bulk {d}/{t}")
                        }
                    }
                }
            }
        }
        dioxus_primitives::dialog::DialogRoot {
            class: "dialog-backdrop",
            open: prefs_open(),
            on_open_change: move |v| prefs_open.set(v),
            dioxus_primitives::dialog::DialogContent { class: "dialog",
                button {
                    class: "dialog-close",
                    aria_label: "Close",
                    tabindex: if prefs_open() { "0" } else { "-1" },
                    onclick: move |_| prefs_open.set(false),
                    "×"
                }
                dioxus_primitives::dialog::DialogTitle { class: "dialog-title", "Preferences" }
                dioxus_primitives::dialog::DialogDescription { class: "dialog-description", "Configure AI indexing and prompts." }
                div { class: "flex flex-col gap-3 mt-2",
                    // Prompt template editor
                    div { class: "flex flex-col gap-1",
                        label { class: "text-10px text-weak", "Vision Prompt Template" }
                        textarea {
                            class: "textarea w-full h-48 bg-muted border border-stroke rounded p-1 font-mono text-10px",
                            value: ui.read().ai_prompt_template.clone(),
                            oninput: move |evt| {
                                let mut s = ui.write();
                                s.ai_prompt_template = evt.value().clone();
                                save_settings(&s);
                            },
                        }
                    }
                    // Show progress overlay toggle
                    div { class: "flex items-center gap-3",
                        div { class: "flex flex-col flex-1",
                            span { class: "text-10px text-weak", "Show Progress Overlay" }
                            span { class: "text-9px text-weak/80",
                                "Toggle visibility of bottom progress panel."
                            }
                        }
                        Switch {
                            class: "switch",
                            checked: ui.read().show_progress_overlay,
                            aria_label: "Show Progress Overlay",
                            on_checked_change: move |v: bool| {
                                let mut s = ui.write();
                                s.show_progress_overlay = v;
                                save_settings(&s);
                            },
                            SwitchThumb { class: "switch-thumb" }
                        }
                    }
                    // Overwrite existing descriptions toggle (slider style simulated)
                    div { class: "flex items-center gap-3",
                        div { class: "flex flex-col flex-1",
                            span { class: "text-10px text-weak", "Overwrite Existing Descriptions" }
                            span { class: "text-9px text-weak/80",
                                "If enabled, regenerating metadata will replace descriptions already stored."
                            }
                        }
                        // simple switch reusing Switch primitive for consistency
                        Switch {
                            class: "switch",
                            checked: ui.read().overwrite_descriptions,
                            aria_label: "Overwrite Descriptions",
                            on_checked_change: move |v: bool| {
                                let mut s = ui.write();
                                s.overwrite_descriptions = v;
                                save_settings(&s);
                            },
                            SwitchThumb { class: "switch-thumb" }
                        }
                    }
                }
            }
        }
        dioxus_primitives::dialog::DialogRoot {
            class: "dialog-backdrop",
            open: recursive_settings_open(),
            on_open_change: move |v| recursive_settings_open.set(v),
            dioxus_primitives::dialog::DialogContent { class: "dialog",
                button {
                    class: "dialog-close",
                    aria_label: "Close",
                    tabindex: if recursive_settings_open() { "0" } else { "-1" },
                    onclick: move |_| recursive_settings_open.set(false),
                    "×"
                }
                dioxus_primitives::dialog::DialogTitle { class: "dialog-title", "Recursive Scan Settings" }
                dioxus_primitives::dialog::DialogDescription { class: "dialog-description",
                    "Customize deep scan exclusions and date range overrides."
                }
                div { class: "flex flex-col gap-3 mt-2 w-[520px] max-w-[90vw]",
                    div { class: "flex flex-col gap-1",
                        label { class: "text-10px text-weak", "Excluded Directories (one per line)" }
                        textarea {
                            class: "textarea h-32 bg-muted border border-stroke rounded p-1 text-10px font-mono",
                            value: excluded_dirs_input(),
                            oninput: move |e| excluded_dirs_input.set(e.value().to_string()),
                        }
                        span { class: "text-8px text-weak",
                            "These directory paths (prefix match) will be skipped during recursive scans."
                        }
                    }
                    div { class: "flex flex-col gap-1",
                        label { class: "text-10px text-weak",
                            "Excluded Extensions (comma or space separated, no dots)"
                        }
                        input {
                            class: "bg-muted border border-stroke rounded p-1 text-10px font-mono",
                            value: excluded_exts_input(),
                            oninput: move |e| excluded_exts_input.set(e.value().to_string()),
                        }
                        span { class: "text-8px text-weak", "Example: tmp, bak, psd" }
                    }
                    div { class: "grid grid-cols-2 gap-4",
                        div { class: "flex flex-col gap-1",
                            label { class: "text-10px text-weak", "Override After (YYYY-MM-DD)" }
                            input {
                                class: "bg-muted border border-stroke rounded p-1 text-10px font-mono",
                                value: rec_after_input(),
                                oninput: move |e| rec_after_input.set(e.value().to_string()),
                            }
                        }
                        div { class: "flex flex-col gap-1",
                            label { class: "text-10px text-weak", "Override Before (YYYY-MM-DD)" }
                            input {
                                class: "bg-muted border border-stroke rounded p-1 text-10px font-mono",
                                value: rec_before_input(),
                                oninput: move |e| rec_before_input.set(e.value().to_string()),
                            }
                        }
                    }
                    div { class: "flex gap-2 justify-end pt-2",
                        button {
                            class: "btn px-3 py-1 text-10px",
                            onclick: move |_| {
                                let mut f = filters.write();
                                f.recursive_excluded_dirs = excluded_dirs_input
                                    .read()
                                    .lines()
                                    .filter_map(|l| {
                                        let t = l.trim();
                                        if t.is_empty() { None } else { Some(std::path::PathBuf::from(t)) }
                                    })
                                    .collect();
                                f.recursive_excluded_exts = excluded_exts_input
                                    .read()
                                    .split(&[',', ' ', ';'][..])
                                    .filter_map(|p| {
                                        let t = p.trim().to_ascii_lowercase();
                                        if t.is_empty() { None } else { Some(t) }
                                    })
                                    .collect();
                                f.recursive_modified_after = if rec_after_input.read().trim().is_empty() {
                                    None
                                } else {
                                    Some(rec_after_input.read().trim().to_string())
                                };
                                f.recursive_modified_before = if rec_before_input.read().trim().is_empty() {
                                    None
                                } else {
                                    Some(rec_before_input.read().trim().to_string())
                                };
                                recursive_settings_open.set(false);
                            },
                            "Save"
                        }
                        button {
                            class: "btn px-3 py-1 text-10px",
                            onclick: move |_| recursive_settings_open.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

fn parse_date_str(s: &str) -> Option<Date> { Date::parse(s, time::macros::format_description!("[year]-[month]-[day] ")).ok().or_else(|| Date::parse(s, time::macros::format_description!("[year]-[month]-[day]")).ok()) }
// maybe_trigger_rescan removed: resource reactivity handles reruns when filters mutate

