use dioxus::prelude::*;
use crate::settings::{UiSettings, save_settings};
use crate::types::ViewMode;
use crate::scan::{begin_scan, cancel_scan};

#[derive(Props, PartialEq, Clone)]
pub struct NavMenuProps {
    pub ui: Signal<UiSettings>,
    pub view_mode: Signal<ViewMode>,
    pub preview_collapsed: Signal<bool>,
    pub qa_collapsed: Signal<bool>,
    pub drives_collapsed: Signal<bool>,
    pub results: Signal<crate::types::ScanResults>,
    pub app_view: Signal<crate::app::AppView>,
    pub error: Signal<Option<String>>,
    // added for scanning
    pub filters: Signal<crate::types::Filters>,
    pub scan_generation: Signal<u64>,

    pub scanning: Signal<bool>,
    pub dir_items: Signal<Vec<crate::types::DirItem>>,
    pub progress: Signal<Option<(usize, usize)>>,
    // AI / indexing related signals
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>,
    pub auto_indexing: Signal<bool>, // reflects auto_descriptions_enabled atomic
}

#[allow(non_snake_case)]
pub fn NavHamburgerMenu(props: NavMenuProps) -> Element {
    let NavMenuProps {
    mut ui,
    mut view_mode,
    mut preview_collapsed,
    mut qa_collapsed,
    mut drives_collapsed,
        results,
        mut app_view,
        error,
        // added
        filters,
        scan_generation,
        scanning,
        dir_items,
        progress,
        ai_search_engine,
        selected_paths,
        mut auto_indexing,
    } = props;

    let mut open = use_signal(|| false);
    let mut anchor_pos = use_signal(|| (0i32, 0i32));
    let mut exporting = use_signal(|| false);
    // local activity flags for async AI actions
    let mut generating_embeddings = use_signal(|| false);
    let mut bulk_generating = use_signal(|| false);
    let mut bulk_progress = use_signal(|| (0usize,0usize));
    let mut show_settings = use_signal(|| false);

    // Precompute progress label each render to avoid inline numeric fragments in RSX
    let progress_label = {
        let (d, t) = *bulk_progress.read();
        format!("Generating Descriptions ({} / {})", d, t)
    };

    // FIX: proper toggle (remove stale precomputed current_open)
    let toggle_open = move |evt: MouseEvent| {
        evt.prevent_default();
        anchor_pos.set((evt.client_coordinates().x as i32 - 5, evt.client_coordinates().y as i32));
        let is_open = *open.read();
        open.set(!is_open);
    };

    rsx! {
        div {
            class: "relative select-none",
            oncontextmenu: toggle_open,
            button {
                class: "rounded transition border",
                style: "background: #0d0d0e; color: var(--error); border; border-radius: 10px; padding-top: 3px;",
                onclick: toggle_open,
                aria_label: "Menu",
                i { class: "material-icons text-lg", "menu" }
            }
            if *open.read() {
                div { class: "fixed inset-0 z-[80]", onclick: move |_| open.set(false) }
                {
                    let (ax, ay) = *anchor_pos.read();
                    let style = format!(
                        "position:fixed; top:{}px; left:{}px; z-index:90; min-width:230px;",
                        ay + 12,
                        (ax - 200).max(8)
                    );
                    rsx! {
                        div {
                            class: "rounded-md border border-stroke backdrop-blur shadow-lg flex flex-col text-11px overflow-hidden",
                            style: style,

                            // View Modes
                            span { class: "popup-menu-header px-2 py-1.5 font-semibold  border-b border-stroke bg-muted", "View" }
                            button {
                                // themed background for all menu buttons
                                class: "results-header-col px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
                                style: "color: white",
                                onclick: move |_| {
                                    view_mode.set(ViewMode::Icons);
                                    let mut u = ui.write(); u.view_mode = Some("icons".into()); save_settings(&u);
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "grid_view" }
                                span { "Icons" }
                                if *view_mode.read() == ViewMode::Icons { i { class: "material-icons text-sm ml-auto text-accent", "check" } }
                            }
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
                                style: "color: white",
                                onclick: move |_| {
                                    view_mode.set(ViewMode::Details);
                                    let mut u = ui.write(); u.view_mode = Some("details".into()); save_settings(&u);
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "view_list" }
                                span { "Details" }
                                if *view_mode.read() == ViewMode::Details { i { class: "material-icons text-sm ml-auto text-accent", "check" } }
                            }

                            // Panes
                            span { class: "popup-menu-header px-2 py-1.5 font-semibold bg-muted mt-1", "Panes" }
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
                                style: "color: white",
                                onclick: move |_| {
                                    let new_val = !*preview_collapsed.read();
                                    preview_collapsed.set(new_val);
                                    let mut u = ui.write();
                                    u.preview_collapsed = new_val;
                                    save_settings(&u);
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "visibility" }
                                span { if *preview_collapsed.read() { "Show Preview Pane" } else { "Hide Preview Pane" } }
                            }
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
                                style: "color: white",
                                onclick: move |_| {
                                    // Mirror header logic
                                    let hide = !(*qa_collapsed.read() && *drives_collapsed.read());
                                    qa_collapsed.set(hide);
                                    drives_collapsed.set(hide);
                                    let mut u = ui.write();
                                    u.qa_collapsed = hide;
                                    u.drives_collapsed = hide;
                                    save_settings(&u);
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "view_sidebar" }
                                span { if *qa_collapsed.read() && *drives_collapsed.read() { "Show Left Pane" } else { "Hide Left Pane" } }
                            }

                            span { class: "popup-menu-header px-2 py-1.5 font-semibold  border-y border-stroke bg-muted mt-1", "Scanning & AI" }

                            // Recursive Scan (uses begin_scan)
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: *scanning.read(),
                                onclick: {
                                    let filters = filters.clone();
                                    let scan_generation = scan_generation.clone();
                                    let scanning = scanning.clone();
                                    let results = results.clone();
                                    let dir_items = dir_items.clone();
                                    let progress = progress.clone();
                                    move |_| {
                                        if *scanning.read() { return; }
                                        begin_scan(
                                            filters.clone(),
                                            scan_generation.clone(),
                                            scanning.clone(),
                                            results.clone(),
                                            dir_items.clone(),
                                            progress.clone(),
                                            true,
                                        );
                                        open.set(false);
                                    }
                                },
                                i { class: "material-icons text-sm ", "travel_explore" }
                                span { if *scanning.read() { "Scanning…" } else { "Recursive Scan" } }
                            }

                            // Stop Scan (cancellation)
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel hover:bg-warn/20 flex items-center gap-2 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: !*scanning.read(),
                                onclick: {
                                    let mut scanning = scanning.clone();
                                    move |_| {
                                        if !*scanning.read() { return; }
                                        cancel_scan();
                                        scanning.set(false);
                                        open.set(false);
                                    }
                                },
                                i { class: "material-icons text-sm ", "stop_circle" }
                                span { "Stop Scan" }
                            }

                            // Generate AI semantic embeddings (recursive) using AI engine
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: *generating_embeddings.read(),
                                onclick: move |_| {
                                    if *generating_embeddings.read() { return; }
                                    generating_embeddings.set(true);
                                    let mut err_sig = error.clone();
                                    let mut gen_flag = generating_embeddings.clone();
                                    let engine_opt = ai_search_engine.read().clone();
                                    let selected = selected_paths.read().clone();
                                    let auto = *auto_indexing.read();
                                    // Spawn on tokio to avoid UI lock
                                    // Use dioxus spawn (not tokio::spawn) because Signals are !Send
                                    spawn(async move {
                                        if let Some(engine) = engine_opt {
                                            engine.auto_descriptions_enabled.store(auto, std::sync::atomic::Ordering::Relaxed);
                                            let mut queued = 0usize;
                                            if selected.is_empty() {
                                                log::info!("[Menu] No selected files to index");
                                            } else {
                                                for p in selected.iter() {
                                                    let meta_opt = if let Ok(md) = std::fs::metadata(p) {
                                                        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                                                        let kind = if crate::types::IMAGE_EXTS.iter().any(|e| *e == ext) { crate::types::MediaKind::Image } else if crate::types::VIDEO_EXTS.iter().any(|e| *e == ext) { crate::types::MediaKind::Video } else { crate::types::MediaKind::Other };
                                                        let ff = crate::types::FoundFile { path: p.clone(), modified: None, created: None, size: Some(md.len()), kind, thumb_data: None };
                                                        Some(crate::ai::found_file_to_metadata(&ff))
                                                    } else { None };
                                                    if let Some(meta) = meta_opt {
                                                        if engine.enqueue_index(meta).await { queued += 1; } else { log::warn!("[Menu] Failed to enqueue {:?}", p); }
                                                    }
                                                }
                                            }
                                            log::info!("[Menu] Queued {queued} files for indexing");
                                        } else { err_sig.set(Some("AI engine not initialized".into())); }
                                        gen_flag.set(false);
                                    });
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "memory" }
                                span { if *generating_embeddings.read() { "Indexing…" } else { "Index Selected Files" } }
                            }
                            // Bulk generate (stream) image descriptions for all current results
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex flex-col gap-1 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: *bulk_generating.read(),
                                onclick: move |_| {
                                    if *bulk_generating.read() { return; }
                                    bulk_generating.set(true);
                                    let mut bulk_prog_sig = bulk_progress.clone();
                                    let engine_opt = ai_search_engine.read().clone();
                                    let results_clone = results.read().items.clone();
                                    let mut ui_settings_sig = ui.clone();
                                    let mut bulk_generating_flag = bulk_generating.clone();
                                    let mut err_sig = error.clone();
                                    spawn(async move {
                                        let total = results_clone.len();
                                        bulk_prog_sig.set((0,total));
                                        if let Some(engine) = engine_opt {
                                            for (idx, f) in results_clone.iter().enumerate() {
                                                let path_str = f.path.display().to_string();
                                                // Only process images/videos heuristically
                                                let ext = f.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
                                                let is_img = crate::types::IMAGE_EXTS.iter().any(|e| *e == ext);
                                                if !is_img { bulk_prog_sig.set((idx+1,total)); continue; }
                                                #[cfg(feature="joycaption")]
                                                if crate::ai::joycaption_adapter::is_enabled() {
                                                    if let Ok(bytes) = tokio::fs::read(&f.path).await {
                                                        let prompt = ui_settings_sig.read().ai_prompt_template.clone();
                                                        let mut interim = String::new();
                                                        let _ = crate::ai::joycaption_adapter::stream_describe_bytes_with_callback(bytes, &prompt, |frag| { interim.push_str(frag); }).await;
                                                        if let Some(val) = crate::ai::joycaption_adapter::extract_json_vision(&interim) {
                                                            if let Ok(vd) = serde_json::from_value::<crate::ai::generate::VisionDescription>(val) { let _ = engine.apply_vision_description(&path_str, &vd).await; }
                                                            else { let _ = engine.set_file_description(&path_str, &interim).await; }
                                                        } else { let _ = engine.set_file_description(&path_str, &interim).await; }
                                                    }
                                                } else {
                                                    if let Some(vd) = engine.generate_vision_description(&f.path).await { let _ = engine.apply_vision_description(&path_str, &vd).await; }
                                                }
                                                bulk_prog_sig.set((idx+1,total));
                                            }
                                        } else { err_sig.set(Some("AI engine not initialized".into())); }
                                        bulk_generating_flag.set(false);
                                    });
                                },
                                i { class: "material-icons text-sm", "auto_fix_high" }
                                if *bulk_generating.read() {
                                    span { "{progress_label}" }
                                } else {
                                    span { "Generate Descriptions (All)" }
                                }
                                if *bulk_generating.read() { div { class: "w-full h-1 bg-stroke rounded overflow-hidden", div { class: "h-1 bg-accent", style: {
                                    let (d,t) = *bulk_progress.read(); let pct = if t>0 { (d as f32 / t as f32 *100.0).min(100.0)} else {0.0}; format!("width:{pct}%; transition:width .15s linear;") }
                                } } }
                            }
                            // Settings popup toggle
                            button { class: "results-header-col px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors", style: "color:white", onclick: move |_| { let cur = *show_settings.read(); show_settings.set(!cur); }, i { class: "material-icons text-sm", "settings" } span { "AI Settings" } }
                            if *show_settings.read() {
                                div { class: "px-3 py-2 bg-panel border-t border-stroke flex flex-col gap-2",
                                    {
                                        let mut ui_sig = ui.clone();
                                        rsx! {
                                            label { class: "text-10px font-semibold text-weak", "Vision Prompt Template" }
                                            textarea { class: "textarea w-full h-50 text-10px bg-muted border border-stroke rounded p-1 font-mono", 
                                                value: ui.read().ai_prompt_template.clone(), 
                                                cols: 25,
                                                rows: 15,
                                                oninput: move |evt| 
                                            {
                                                let mut settings = ui_sig.write();
                                                settings.ai_prompt_template = evt.value().clone();
                                                save_settings(&settings);
                                            } }
                                            span { class: "text-8px text-weak", "This template should output ONLY JSON. (description, caption, tags[], category)" }
                                        }
                                    }
                                }
                            }
                            // Auto indexing toggle (Switch style)
                            div { class: "px-3 py-1.5 text-left bg-panel flex items-center gap-3 transition-colors",
                                style: "color: white",
                                span { class: "text-10px font-medium", "Auto Index" }
                                input { r#type: "checkbox", class: "appearance-none w-10 h-5 rounded-full bg-muted relative cursor-pointer",
                                    checked: *auto_indexing.read(),
                                    oninput: move |_| {
                                        let new_state = !*auto_indexing.read();
                                        auto_indexing.set(new_state);
                                        if let Some(engine) = ai_search_engine.read().as_ref() { engine.auto_descriptions_enabled.store(new_state, std::sync::atomic::Ordering::Relaxed); }
                                    }
                                }
                                span { class: "text-10px", if *auto_indexing.read() { "ON" } else { "OFF" } }
                            }


                            // Actions
                            span { class: "popup-menu-header px-2 py-1.5 font-semibold  border-y border-stroke bg-muted mt-1", "Actions" }
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: *exporting.read() || results.read().items.is_empty(),
                                onclick: move |_| {
                                    if results.read().items.is_empty() || *exporting.read() { return; }
                                    exporting.set(true);
                                    let rows = results.read().items.clone();
                                    let mut error = error.clone();
                                    let mut exporting_flag = exporting.clone();
                                    spawn(async move {
                                        let res = crate::app::app_export_csv(&rows);
                                        if let Err(e) = res { error.set(Some(e)); } else { error.set(None); }
                                        exporting_flag.set(false);
                                    });
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "download" }
                                span { if *exporting.read() { "Exporting…" } else { "Download CSV" } }
                            }
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors",
                                style: "color: white",
                                onclick: move |_| {
                                    let new_view = if *app_view.read() == crate::app::AppView::Explorer {
                                        crate::app::AppView::DebugDb
                                    } else {
                                        crate::app::AppView::Explorer
                                    };
                                    app_view.set(new_view);
                                    let u = ui.write();
                                    save_settings(&u);
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", { if *app_view.read() == crate::app::AppView::DebugDb { "dataset" } else { "storage" } } }
                                span { if *app_view.read() == crate::app::AppView::DebugDb { "Exit Debug View" } else { "Debug View" } }
                            }

                            // Close
                            button {
                                class: "results-header-col px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 mt-1 border-t border-stroke transition-colors",
                                style: "color: white",
                                onclick: move |_| open.set(false),
                                i { class: "material-icons text-sm ", "close" }
                                span { "Close Menu" }
                            }
                        }
                    }
                }
            }
        }
    }
}