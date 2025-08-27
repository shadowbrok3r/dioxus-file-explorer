use dioxus::prelude::*;
use crate::settings::{UiSettings, save_settings};
use crate::types::ViewMode;
use crate::scan::{begin_scan, cancel_scan, ScanMsg}; // added
use crossbeam::channel::Receiver;                    // added

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
    pub scan_rx: Signal<Option<Receiver<ScanMsg>>>,

    pub scanning: Signal<bool>,
    pub dir_items: Signal<Vec<crate::types::DirItem>>,
    pub progress: Signal<Option<(usize, usize)>>,
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
        scan_rx,
        scanning,
        dir_items,
        progress,
    } = props;

    let mut open = use_signal(|| false);
    let mut anchor_pos = use_signal(|| (0i32, 0i32));
    let mut exporting = use_signal(|| false);
    // local activity flags for async AI actions
    let mut generating_embeddings = use_signal(|| false);
    let mut backfilling_clip = use_signal(|| false);

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
                class: "rounded transition",
                style: "background: #0d0d0e; color: var(--error)",
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
                            // Added subtle translucent background
                            class: "rounded-md border border-stroke backdrop-blur shadow-lg flex flex-col text-11px overflow-hidden",
                            style: style,

                            // View Modes
                            span { class: "px-2 py-1.5 font-semibold  border-b border-stroke bg-muted", "View" }
                            button {
                                // themed background for all menu buttons
                                class: "px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
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
                                class: "px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
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
                            span { class: "px-2 py-1.5 font-semibold  bg-muted mt-1", "Panes" }
                            button {
                                class: "px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
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
                                class: "px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors",
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

                            // NEW Scanning & AI section
                            span { class: "px-2 py-1.5 font-semibold  border-y border-stroke bg-muted mt-1", "Scanning & AI" }

                            // Recursive Scan (uses begin_scan)
                            button {
                                class: "px-3 py-1.5 text-left bg-panel flex items-center gap-2 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: *scanning.read(),
                                onclick: {
                                    let filters = filters.clone();
                                    let scan_rx = scan_rx.clone();
                                    let scanning = scanning.clone();
                                    let results = results.clone();
                                    let dir_items = dir_items.clone();
                                    let progress = progress.clone();
                                    move |_| {
                                        if *scanning.read() { return; }
                                        begin_scan(
                                            filters.clone(),
                                            scan_rx.clone(),
                                            scanning.clone(),
                                            results.clone(),
                                            dir_items.clone(),
                                            progress.clone(),
                                            true, // recursive
                                        );
                                        open.set(false);
                                    }
                                },
                                i { class: "material-icons text-sm ", "travel_explore" }
                                span { if *scanning.read() { "Scanning…" } else { "Recursive Scan" } }
                            }

                            // Stop Scan (cancellation)
                            button {
                                class: "px-3 py-1.5 text-left bg-panel hover:bg-warn/20 flex items-center gap-2 transition-colors disabled:opacity-40",
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
                                class: "px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: *generating_embeddings.read(),
                                onclick: move |_| {
                                    if *generating_embeddings.read() { return; }
                                    generating_embeddings.set(true);
                                    let mut err_sig = error.clone();
                                    let mut gen_flag = generating_embeddings.clone();
                                    spawn(async move {
                                        match crate::ai::AISearchEngine::new().await {
                                            Ok(engine) => { let added = engine.generate_semantic_recursive().await; log::info!("[Menu] semantic recursive added {added}"); },
                                            Err(e) => err_sig.set(Some(e.to_string())),
                                        }
                                        gen_flag.set(false);
                                    });
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "memory" }
                                span { if *generating_embeddings.read() { "Generating…" } else { "Generate Embeddings" } }
                            }

                            // Backfill CLIP embeddings for missing images
                            button {
                                class: "px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors disabled:opacity-40",
                                style: "color: white",
                                disabled: *backfilling_clip.read(),
                                onclick: move |_| {
                                    if *backfilling_clip.read() { return; }
                                    backfilling_clip.set(true);
                                    let mut err_sig = error.clone();
                                    let mut flag = backfilling_clip.clone();
                                    spawn(async move {
                                        match crate::ai::AISearchEngine::new().await {
                                            Ok(engine) => { let added = engine.backfill_clip_embeddings().await; log::info!("[Menu] CLIP backfill added {added}"); },
                                            Err(e) => err_sig.set(Some(e.to_string())),
                                        }
                                        flag.set(false);
                                    });
                                    open.set(false);
                                },
                                i { class: "material-icons text-sm ", "auto_awesome" }
                                span { if *backfilling_clip.read() { "Backfilling…" } else { "Backfill CLIP Embeddings" } }
                            }

                            // Actions
                            span { class: "px-2 py-1.5 font-semibold  border-y border-stroke bg-muted mt-1", "Actions" }
                            button {
                                class: "px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors disabled:opacity-40",
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
                                class: "px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 transition-colors",
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
                                class: "px-3 py-1.5 text-left bg-panel hover:bg-accent/10 flex items-center gap-2 mt-1 border-t border-stroke transition-colors",
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