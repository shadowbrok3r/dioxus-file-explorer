use dioxus::prelude::*;
use keyboard_types::Key;
use crate::settings::{save_settings, UiSettings};
use crate::types::{ViewMode};
use std::path::PathBuf;

#[derive(Props, PartialEq, Clone)]
pub struct HeaderProps {
    pub path_text: Signal<String>,
    pub filters: Signal<crate::types::Filters>,
    pub scanning: Signal<bool>,
    pub results: Signal<crate::types::ScanResults>,
    pub dir_items: Signal<Vec<crate::types::DirItem>>,
    pub progress: Signal<Option<(usize, usize)>>,
    pub rx_state: Signal<Option<crossbeam::channel::Receiver<crate::scan::ScanMsg>>>,
    pub recursive_current: Signal<bool>,
    pub only_subdirs: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_finished: Signal<Option<std::time::Instant>>,
    pub ui: Signal<UiSettings>,
    pub view_mode: Signal<ViewMode>,
    pub preview_collapsed: Signal<bool>,
    pub preview_width: Signal<u32>,
    pub left_width: Signal<u32>,
    pub qa_collapsed: Signal<bool>,
    pub drives_collapsed: Signal<bool>,
    pub search_text: Signal<String>,
    pub ai_search_results: Signal<Vec<crate::ai::FileMetadata>>,
    pub ai_model_ready: Signal<bool>,
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub app_view: Signal<super::super::app::AppView>,
    pub group_by_category: Signal<bool>,
    pub ai_search_active: Signal<bool>,
    pub ai_descriptions: Signal<std::collections::HashMap<String,String>>,
    pub ai_generating: Signal<bool>,
    pub ai_pending_desc: Signal<usize>,
    pub selected_path: Signal<Option<PathBuf>>,
    pub error: Signal<Option<String>>,
    pub debug_thumb_rows: Signal<Vec<crate::ai::ThumbRow>>,
    pub debug_doc_snips: Signal<Vec<crate::ai::DebugDocumentSnippet>>,
    pub debug_loaded_at: Signal<Option<std::time::Instant>>,
}

pub fn header(props: HeaderProps) -> Element {
    // Locals for closures
    let mut path_text = props.path_text;
    let mut filters = props.filters;
    let mut scanning = props.scanning;
    let mut results = props.results;
    let mut dir_items = props.dir_items;
    let progress = props.progress;
    let mut rx_state = props.rx_state;
    let mut recursive_current = props.recursive_current;
    let mut only_subdirs = props.only_subdirs;
    let mut scan_started = props.scan_started;
    let mut scan_finished = props.scan_finished;
    let mut ui = props.ui;
    let mut view_mode = props.view_mode;
    let mut preview_collapsed = props.preview_collapsed;
    let preview_width = props.preview_width;
    let mut qa_collapsed = props.qa_collapsed;
    let mut drives_collapsed = props.drives_collapsed;
    let mut search_text = props.search_text;
    let mut ai_search_results = props.ai_search_results;
    let ai_model_ready = props.ai_model_ready;
    let ai_search_engine = props.ai_search_engine;
    let mut app_view = props.app_view;
    let _group_by_category = props.group_by_category; // reserved for future toggle
    let mut ai_search_active = props.ai_search_active;
    let ai_descriptions = props.ai_descriptions;
    let ai_generating = props.ai_generating;
    let ai_pending_desc = props.ai_pending_desc;
    let selected_path = props.selected_path;
    let mut error = props.error;
    let debug_thumb_rows = props.debug_thumb_rows;
    let debug_doc_snips = props.debug_doc_snips;
    let debug_loaded_at = props.debug_loaded_at;

    rsx! { header { class: "flex items-center gap-2 px-3 py-2 bg-panel border-b border-stroke",
        // Path input
        input { class: "flex-1 bg-muted text-var-text border border-stroke rounded-md px-2 py-1",
            value: "{path_text.read().clone()}",
            oninput: move |evt| { path_text.set(evt.value()); },
            onkeydown: move |evt| {
                if evt.key() == Key::Enter {
                    let p = PathBuf::from(path_text.read().clone());
                    if p.exists() {
                        let new_root = p.clone();
                        {
                            let mut f = filters.write();
                            f.root = new_root.clone();
                        }
                        recursive_current.set(false);
                        if crate::app::shallow_should_scan(&new_root) {
                            only_subdirs.set(false);
                            scan_started.set(Some(std::time::Instant::now()));
                            scan_finished.set(None);
                            crate::scan::begin_scan(
                                filters, rx_state, scanning, results, dir_items, progress, false,
                            );
                        } else {
                            only_subdirs.set(true);
                            dir_items.set(crate::explorer::list_dir_items(new_root).unwrap_or_default());
                            results.set(Default::default());
                        }
                    }
                }
            }
        }
        // Up directory
        button { class: "btn", disabled: filters.read().root.parent().is_none(),
            onclick: move |_| {
                let mut new_root = filters.read().root.clone();
                if new_root.pop() {
                    {
                        let mut f = filters.write();
                        path_text.set(new_root.display().to_string());
                        f.root = new_root;
                    }
                    recursive_current.set(false);
                    if crate::app::shallow_should_scan(&filters.read().root) {
                        only_subdirs.set(false);
                        scan_started.set(Some(std::time::Instant::now()));
                        scan_finished.set(None);
                        crate::scan::begin_scan(filters, rx_state, scanning, results, dir_items, progress, false);
                    } else {
                        only_subdirs.set(true);
                        dir_items.set(crate::explorer::list_dir_items(filters.read().root.clone()).unwrap_or_default());
                        results.set(Default::default());
                    }
                }
            },
            i { class: "material-icons", "arrow_upward" }
        }
        // Deep scan
        button { class: "btn", title: "Deep recursive scan all subfolders",
            onclick: move |_| {
                if scanning.read().clone() { rx_state.set(None); scanning.set(false); }
                recursive_current.set(true);
                scan_started.set(Some(std::time::Instant::now()));
                scan_finished.set(None);
                crate::scan::begin_scan(filters, rx_state, scanning, results, dir_items, progress, true);
            },
            i { class: "material-icons", "travel_explore" }
        }
        // Cancel
        button { class: "btn", disabled: !scanning.read().clone(),
            onclick: move |_| { rx_state.set(None); scanning.set(false); scan_finished.set(Some(std::time::Instant::now())); },
            i { class: "material-icons", "close" }
        }
        // Icons view
        button { class: "btn", title: "Icons view",
            onclick: move |_| {
                view_mode.set(ViewMode::Icons);
                let mut s = ui.write(); s.view_mode = Some("icons".into()); save_settings(&s);
            },
            i { class: "material-icons", "grid_view" }
        }
        // Details view
        button { class: "btn", title: "Details view",
            onclick: move |_| {
                view_mode.set(ViewMode::Details);
                let mut s = ui.write(); s.view_mode = Some("details".into()); save_settings(&s);
            },
            i { class: "material-icons", "view_list" }
        }
        // Export CSV
        button { class: "btn", disabled: results.read().items.is_empty(),
            onclick: move |_| {
                if results.read().items.is_empty() { return; }
                if let Err(e) = crate::app::app_export_csv(&results.read().items) { error.set(Some(e)); } else { error.set(None); }
            },
            i { class: "material-icons", "download" }
        }
        // DB Debug toggle
        button { class: if *app_view.read() == super::super::app::AppView::DebugDb { "btn bg-fuchsia-600 text-white" } else { "btn" },
            title: "Toggle DB Debug View",
            onclick: move |_| {
                let new_view = if *app_view.read() == super::super::app::AppView::Explorer { super::super::app::AppView::DebugDb } else { super::super::app::AppView::Explorer };
                app_view.set(new_view);
                if new_view == super::super::app::AppView::DebugDb {
                    if let Some(engine) = ai_search_engine.read().as_ref() {
                        let engine_clone = engine.clone();
                        let mut thumb_sig = debug_thumb_rows.clone();
                        let mut doc_sig = debug_doc_snips.clone();
                        let mut ts_sig = debug_loaded_at.clone();
                        spawn(async move {
                            let thumbs = engine_clone.list_thumbnail_rows(500).await;
                            let docs = engine_clone.list_document_snippets(200).await;
                            thumb_sig.set(thumbs); doc_sig.set(docs); ts_sig.set(Some(std::time::Instant::now()));
                        });
                    }
                }
            },
            i { class: "material-icons", {if *app_view.read() == super::super::app::AppView::DebugDb { "dataset" } else { "storage" }} }
        }
        // Toggle left nav
        button { class: "btn", title: if *qa_collapsed.read() && *drives_collapsed.read() { "Show left navigation" } else { "Hide left navigation" },
            onclick: move |_| {
                let hide = !(*qa_collapsed.read() && *drives_collapsed.read());
                qa_collapsed.set(hide); drives_collapsed.set(hide);
                let mut s = ui.write(); s.qa_collapsed = hide; s.drives_collapsed = hide; save_settings(&s);
            },
            i { class: "material-icons", { if *qa_collapsed.read() && *drives_collapsed.read() { "chevron_right" } else { "chevron_left" } } }
        }
        // Unified Search / AI Search input
        input { class: "w-72 bg-muted text-var-text border border-stroke rounded-md px-2 py-1 text-sm",
            placeholder: if *ai_search_active.read() { "Describe what you're looking for..." } else { "Search..." },
            value: "{search_text.read().clone()}",
            oninput: move |e| {
                let query = e.value();
                search_text.set(query.clone());
                if *ai_search_active.read() {
                    if query.trim().is_empty() { ai_search_results.set(Vec::new()); return; }
                    if ai_search_engine.read().is_none() { return; }
                    let engine = ai_search_engine.read().clone();
                    let mut results_sig = ai_search_results.clone();
                    let query2 = query.clone();
                    spawn(async move { if let Some(engine) = engine { if let Ok(results) = engine.search(&query2).await { results_sig.set(results); } } });
                }
            }
        }
        // AI toggle & enrichment tools
        div { class: "flex items-center gap-2",
            label { class: "flex items-center gap-1 text-11px px-2 py-1 rounded border border-stroke bg-muted cursor-pointer select-none",
                input { r#type: "checkbox", checked: *ai_search_active.read(), oninput: move |_| { let new_state = !*ai_search_active.read(); ai_search_active.set(new_state); ai_search_results.set(Vec::new()); } }
                span { "AI" }
            }
            if *ai_search_active.read() {
                if !*ai_model_ready.read() { span { class: "text-10px px-2 py-0.5 rounded bg-muted border border-stroke text-weak", "Loading model..." } }
                else if *ai_generating.read() { span { class: "text-10px px-2 py-0.5 rounded bg-accent/10 border border-accent text-accent", "Generating ({ai_pending_desc.read()})" } }
                else if *ai_pending_desc.read() > 0 { span { class: "text-10px px-2 py-0.5 rounded bg-muted border border-stroke text-weak", "{ai_pending_desc.read()} missing" } }
            }
            if *ai_search_active.read() && ai_search_engine.read().is_some() {
                button { class: "btn", title: "Generate all missing image descriptions",
                    onclick: move |_| {
                        if let Some(engine) = ai_search_engine.read().clone() {
                            let mut ai_generating2 = ai_generating.clone();
                            let mut ai_pending2 = ai_pending_desc.clone();
                            let mut desc_map = ai_descriptions.clone();
                            spawn(async move {
                                ai_generating2.set(true);
                                ai_pending2.set(engine.count_missing_descriptions().await);
                                let produced = engine.enrich_missing_descriptions().await;
                                if produced > 0 { if let Ok(all) = engine.get_all_files().await { for f in all { if let Some(d) = f.description { desc_map.write().insert(f.path.clone(), d); } } } }
                                ai_pending2.set(engine.count_missing_descriptions().await);
                                ai_generating2.set(false);
                            });
                        }
                    },
                    i { class: "material-icons", "auto_fix_high" }
                }
            }
            if selected_path.read().is_some() && ai_search_engine.read().is_some() {
                button { class: "btn", title: "Force AI reindex selected file",
                    onclick: move |_| {
                        if let (Some(p), Some(engine)) = (selected_path.read().clone(), ai_search_engine.read().clone()) {
                            let path_str = p.display().to_string();
                            let mut ai_desc_sig = ai_descriptions.clone();
                            spawn(async move {
                                match engine.force_reindex_path(&path_str).await {
                                    Ok(_) => { if let Some(meta) = engine.get_file_metadata(&path_str).await { if let Some(desc) = meta.description { ai_desc_sig.write().insert(path_str.clone(), desc); } } }
                                    Err(e) => log::warn!("Force reindex failed for {}: {}", path_str, e),
                                }
                            });
                        }
                    },
                    i { class: "material-icons", "refresh" }
                }
            }
        }
        span { class: "flex-1" }
        // Preview toggle
        button { class: "btn", title: if *preview_collapsed.read() { "Show preview pane" } else { "Hide preview pane" },
            onclick: move |_| { let curr = *preview_collapsed.read(); preview_collapsed.set(!curr); let mut s = ui.write(); s.preview_collapsed = !curr; s.preview_width = *preview_width.read(); save_settings(&s); },
            i { class: "material-icons", { if *preview_collapsed.read() { "visibility" } else { "visibility_off" } } }
        }
    }}
}
