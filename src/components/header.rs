use dioxus::prelude::*;
use keyboard_types::Key;
use crate::settings::UiSettings;
use crate::types::{ViewMode};
use std::path::PathBuf;
use super::nav_menu::NavHamburgerMenu; // added import

#[derive(Props, PartialEq, Clone)]
pub struct HeaderProps {
    pub path_text: Signal<String>,
    pub filters: Signal<crate::types::Filters>,
    pub scanning: Signal<bool>,
    pub results: Signal<crate::types::ScanResults>,
    pub dir_items: Signal<Vec<crate::types::DirItem>>,
    pub progress: Signal<Option<(usize, usize)>>,
    pub scan_generation: Signal<u64>,
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
    pub selected_paths: Signal<std::collections::HashSet<PathBuf>>,
    pub filtered_items_count: usize,
    pub error: Signal<Option<String>>,
    pub debug_thumb_rows: Signal<Vec<crate::ai::ThumbRow>>,
    pub debug_doc_snips: Signal<Vec<crate::ai::DebugDocumentSnippet>>,
    pub debug_loaded_at: Signal<Option<std::time::Instant>>,
    pub auto_indexing: Signal<bool>,
    pub index_queue_len: Signal<usize>,
    pub index_active: Signal<usize>,
    pub index_completed: Signal<usize>,
    pub nav_history: Signal<Vec<PathBuf>>,
}

#[component]
pub fn Header(props: HeaderProps) -> Element {
    // Locals for closures
    let mut path_text = props.path_text;
    let mut filters = props.filters;
    let scanning = props.scanning;
    let mut results = props.results;
    let dir_items = props.dir_items;
    let progress = props.progress;
    let scan_generation = props.scan_generation;
    let mut recursive_current = props.recursive_current;
    let mut only_subdirs = props.only_subdirs;
    let mut scan_started = props.scan_started;
    let mut scan_finished = props.scan_finished;
    let ui = props.ui;
    let view_mode = props.view_mode;
    let preview_collapsed = props.preview_collapsed;
    let _preview_width = props.preview_width; // currently unused
    let qa_collapsed = props.qa_collapsed;
    let drives_collapsed = props.drives_collapsed;
    let mut search_text = props.search_text;
    let mut ai_search_results = props.ai_search_results;
    let _ai_model_ready = props.ai_model_ready;
    let mut ai_search_active = props.ai_search_active; // needs mut for set() in onclick
    let ai_search_engine = props.ai_search_engine; // restore binding (was underscored)
    let _ai_descriptions = props.ai_descriptions;
    let _ai_generating = props.ai_generating;
    let _ai_pending_desc = props.ai_pending_desc;
    let _selected_path = props.selected_path;
    let _selected_paths = props.selected_paths; // duplicates removed; keep for future
    let _filtered_items_count = props.filtered_items_count;
    let error = props.error;
    let _debug_thumb_rows = props.debug_thumb_rows;
    let _debug_doc_snips = props.debug_doc_snips;
    let _debug_loaded_at = props.debug_loaded_at;
    let app_view = props.app_view;
    let auto_indexing = props.auto_indexing;
    let index_queue_len = props.index_queue_len;
    let index_active = props.index_active;
    let index_completed = props.index_completed;
    let mut nav_history = props.nav_history;

    
    rsx! { header { class: "flex items-center gap-2 px-2 bg-panel border-b border-stroke",
        // Back button (navigation history)
        button { class: "btn", disabled: nav_history.read().is_empty(),
            onclick: move |_| {
                if let Some(prev) = nav_history.write().pop() {
                    {
                        let mut f = filters.write();
                        f.root = prev.clone();
                        path_text.set(prev.display().to_string());
                    }
                    recursive_current.set(false);
                    if crate::app::shallow_should_scan(&prev) {
                        only_subdirs.set(false);
                        scan_started.set(Some(std::time::Instant::now()));
                        scan_finished.set(None);
                        crate::scan::begin_scan(filters, scan_generation, scanning, results, dir_items, progress, false);
                    } else {
                        only_subdirs.set(true);
                        let mut dir_items_sig = dir_items.clone();
                        let root_for_list = prev.clone();
                        dioxus::prelude::spawn(async move {
                            match crate::explorer::list_dir_items(root_for_list).await {
                                Ok(items) => dir_items_sig.set(items),
                                Err(_) => dir_items_sig.set(Vec::new()),
                            }
                        });
                        results.set(Default::default());
                    }
                }
            },
            i { class: "material-icons", "arrow_back" }
        }
        // Up directory
        button { class: "btn", disabled: filters.read().root.parent().is_none(),
            onclick: move |_| {
                let mut new_root = filters.read().root.clone();
                if new_root.pop() {
                    // push current into history
                    nav_history.write().push(filters.read().root.clone());
                    {
                        {
                            let mut f = filters.write();
                            path_text.set(new_root.display().to_string());
                            f.root = new_root.clone();
                        }
                    }
                    recursive_current.set(false);
                        let cur_root = new_root.clone();
                        if crate::app::shallow_should_scan(&cur_root) {
                        only_subdirs.set(false);
                        scan_started.set(Some(std::time::Instant::now()));
                        scan_finished.set(None);
                        crate::scan::begin_scan(filters, scan_generation, scanning, results, dir_items, progress, false);
                    } else {
                        only_subdirs.set(true);
                        {
                            let mut dir_items_sig = dir_items.clone();
                            let root_for_list = filters.read().root.clone();
                            dioxus::prelude::spawn(async move {
                                match crate::explorer::list_dir_items(root_for_list).await {
                                    Ok(items) => dir_items_sig.set(items),
                                    Err(_) => dir_items_sig.set(Vec::new()),
                                }
                            });
                        }
                        results.set(Default::default());
                    }
                }
            },
            i { class: "material-icons", "arrow_upward" }
        }
        // Path input
        input { 
            class: "flex-1 bg-muted text-var-text border border-stroke rounded-md px-2 py-1",
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
                                filters, scan_generation, scanning, results, dir_items, progress, false,
                            );
                        } else {
                            only_subdirs.set(true);
                            {
                                let mut dir_items_sig = dir_items.clone();
                                let root_for_list = new_root.clone();
                                dioxus::prelude::spawn(async move {
                                    match crate::explorer::list_dir_items(root_for_list).await {
                                        Ok(items) => dir_items_sig.set(items),
                                        Err(_) => dir_items_sig.set(Vec::new()),
                                    }
                                });
                            }
                            results.set(Default::default());
                        }
                    }
                }
            }
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
        // AI search toggle button (explicit)
        button { class: if *ai_search_active.read() { "btn bg-accent text-white" } else { "btn" }, title: "Toggle AI semantic search mode",
            onclick: move |_| { let new_state = !*ai_search_active.read(); ai_search_active.set(new_state); if !new_state { ai_search_results.set(Vec::new()); } },
            i { class: "material-icons", { if *ai_search_active.read() { "psychology" } else { "psychology_alt" } } }
        }
        // Insert hamburger menu at far right
        // Indexing progress badge (manual mode only)
        if *index_queue_len.read() > 0 || *index_active.read() > 0 {
            span { class: "mx-2 px-2 py-1 rounded bg-accent/20 text-accent text-10px font-medium flex items-center gap-1",
                i { class: "material-icons text-xs", "memory" }
                span { "Idx q:{index_queue_len.read()} act:{index_active.read()}" }
            }
        } else if *index_completed.read() > 0 {
            span { class: "mx-2 px-2 py-1 rounded bg-green-600/20 text-green-400 text-10px font-medium", "Indexed {index_completed.read()}" }
        }
        NavHamburgerMenu {
            ui: ui,
            view_mode: view_mode,
            preview_collapsed: preview_collapsed,
            qa_collapsed: qa_collapsed,
            drives_collapsed: drives_collapsed,
            results: results,
            app_view: app_view,
            error: error,
            filters: filters,
            scan_generation: scan_generation,
            scanning: scanning,
            dir_items: dir_items,
                progress: progress,
            ai_search_engine: ai_search_engine,
            selected_paths: _selected_paths,
            auto_indexing: auto_indexing,
        }
    }}
}
