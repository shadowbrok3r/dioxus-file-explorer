use dioxus::prelude::*;
use keyboard_types::Key;
use crate::settings::{UiSettings, save_settings};
use crate::types::{ViewMode};
use std::path::PathBuf;
use dioxus_primitives::dialog::{DialogRoot, DialogContent, DialogTitle, DialogDescription};
use dioxus_primitives::switch::{Switch, SwitchThumb};
use crate::scan::{begin_scan, cancel_scan};

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
    let mut scanning = props.scanning;
    let mut results = props.results;
    let dir_items = props.dir_items;
    let progress = props.progress;
    let scan_generation = props.scan_generation;
    let mut recursive_current = props.recursive_current;
    let mut only_subdirs = props.only_subdirs;
    let mut scan_started = props.scan_started;
    let mut scan_finished = props.scan_finished;
    let mut ui = props.ui;
    let mut view_mode = props.view_mode;
    let mut preview_collapsed = props.preview_collapsed;
    let _preview_width = props.preview_width; // currently unused
    let mut qa_collapsed = props.qa_collapsed;
    let mut drives_collapsed = props.drives_collapsed;
    let mut search_text = props.search_text;
    let mut ai_search_results = props.ai_search_results;
    let _ai_model_ready = props.ai_model_ready;
    let mut ai_search_active = props.ai_search_active; // needs mut for set() in onclick
    let ai_search_engine = props.ai_search_engine; // restore binding (was underscored)
    let _ai_descriptions = props.ai_descriptions;
    let _ai_generating = props.ai_generating;
    let _ai_pending_desc = props.ai_pending_desc;
    let _selected_path = props.selected_path;
    let selected_paths = props.selected_paths; // now actively used for indexing
    let _filtered_items_count = props.filtered_items_count;
    let error = props.error;
    let _debug_thumb_rows = props.debug_thumb_rows;
    let _debug_doc_snips = props.debug_doc_snips;
    let _debug_loaded_at = props.debug_loaded_at;
    let mut app_view = props.app_view;
    let mut auto_indexing = props.auto_indexing;
    let index_queue_len = props.index_queue_len;
    let index_active = props.index_active;
    let index_completed = props.index_completed;
    let mut nav_history = props.nav_history;

    // Local UI state migrated from former NewNavbar component
    let bulk_generating = use_signal(|| false);
    let bulk_progress = use_signal(|| (0usize,0usize));
    let exporting = use_signal(|| false);

    
    let mut prefs_open = use_signal(|| false);

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
        // AI search toggle (Switch primitive)
        div { class: "flex items-center gap-1 text-10px px-1",
            span { class: "text-8px text-weak", "AI" }
            Switch { class: "switch", checked: *ai_search_active.read(),
                on_checked_change: move |v: bool| {
                    ai_search_active.set(v);
                    if !v { ai_search_results.set(Vec::new()); }
                },
                SwitchThumb { class: "switch-thumb" }
            }
        }
        // Indexing progress badge (manual mode only)
        if *index_queue_len.read() > 0 || *index_active.read() > 0 {
            span { class: "mx-2 px-2 py-1 rounded bg-accent/20 text-accent text-10px font-medium flex items-center gap-1",
                i { class: "material-icons text-xs", "memory" }
                span { "Idx q:{index_queue_len.read()} act:{index_active.read()}" }
            }
        } else if *index_completed.read() > 0 {
            span { class: "mx-2 px-2 py-1 rounded bg-green-600/20 text-green-400 text-10px font-medium", "Indexed {index_completed.read()}" }
        }
        // Menubar (logical grouping of legacy menu categories)
        // Simplified consolidated action groups (fallback while Menubar primitives unusable)
        div { class: "flex gap-2 items-center flex-1",
            // View group
            div { class: "flex items-center gap-1",
                button { class: "btn", title: "Icons view", onclick: move |_| { view_mode.set(ViewMode::Icons); let mut s=ui.write(); s.view_mode=Some("icons".into()); crate::settings::save_settings(&s); }, i { class: "material-icons text-sm", "grid_view" } }
                button { class: "btn", title: "Details view", onclick: move |_| { view_mode.set(ViewMode::Details); let mut s=ui.write(); s.view_mode=Some("details".into()); crate::settings::save_settings(&s); }, i { class: "material-icons text-sm", "view_list" } }
            }
            // Pane toggles
            button { class: "btn", title: if *preview_collapsed.read() { "Show preview" } else { "Hide preview" }, onclick: move |_| { let new_val=!*preview_collapsed.read(); preview_collapsed.set(new_val); let mut s=ui.write(); s.preview_collapsed=new_val; crate::settings::save_settings(&s); }, i { class: "material-icons text-sm", { if *preview_collapsed.read() { "visibility" } else { "visibility_off" } } } }
            button { class: "btn", title: if *qa_collapsed.read() && *drives_collapsed.read() { "Show left" } else { "Hide left" }, onclick: move |_| { let hide = !(*qa_collapsed.read() && *drives_collapsed.read()); qa_collapsed.set(hide); drives_collapsed.set(hide); let mut s=ui.write(); s.qa_collapsed=hide; s.drives_collapsed=hide; crate::settings::save_settings(&s); }, i { class: "material-icons text-sm", "view_sidebar" } }
            // Scan actions
            button { class: "btn", disabled: *scanning.read(), title: "Recursive scan", onclick: { let filters=filters.clone(); let scan_generation=scan_generation.clone(); let scanning=scanning.clone(); let results=results.clone(); let dir_items=dir_items.clone(); let progress=progress.clone(); move |_| { if *scanning.read(){return;} begin_scan(filters.clone(), scan_generation.clone(), scanning.clone(), results.clone(), dir_items.clone(), progress.clone(), true); } }, i { class: "material-icons text-sm", "travel_explore" } }
            button { class: "btn", disabled: !*scanning.read(), title: "Stop scan", onclick: move |_| { if !*scanning.read(){return;} cancel_scan(); scanning.set(false); }, i { class: "material-icons text-sm", "stop_circle" } }
            // Index selected
            button { class: "btn", disabled: ai_search_engine.read().is_none(), title: "Index selected files", onclick: { let ai_search_engine=ai_search_engine.clone(); let selected_paths=selected_paths.clone(); let auto_indexing=auto_indexing.clone(); let mut err_sig=error.clone(); move |_| { if let Some(engine)=ai_search_engine.read().as_ref(){ engine.auto_descriptions_enabled.store(*auto_indexing.read(), std::sync::atomic::Ordering::Relaxed); let selected=selected_paths.read().clone(); let engine2=engine.clone(); spawn(async move { let mut queued=0usize; for p in selected.iter(){ if let Ok(md)=std::fs::metadata(p){ let ext=p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase(); let kind= if crate::types::IMAGE_EXTS.iter().any(|e| *e==ext){crate::types::MediaKind::Image}else if crate::types::VIDEO_EXTS.iter().any(|e| *e==ext){crate::types::MediaKind::Video}else{crate::types::MediaKind::Other}; let ff=crate::types::FoundFile { path:p.clone(), modified:None, created:None, size:Some(md.len()), kind, thumb_data:None }; if engine2.enqueue_index(crate::ai::found_file_to_metadata(&ff)).await { queued+=1; } } } if queued==0 { err_sig.set(Some("No files indexed (selection empty or unsupported)".into())); } }); } } }, i { class: "material-icons text-sm", "memory" } }
            // Bulk generate (shared helper)
            button { class: "btn", disabled: *bulk_generating.read() || ai_search_engine.read().is_none(), title: "Bulk generate image descriptions",
                onclick: { let ai_search_engine=ai_search_engine.clone(); let results=results.clone(); let bulk_generating=bulk_generating.clone(); let bulk_progress=bulk_progress.clone(); let err_sig=error.clone(); let ui_sig=ui.clone(); move |_| { crate::ai::bulk::spawn_bulk_generate(ai_search_engine.read().clone(), results.read().items.clone(), ui_sig.read().ai_prompt_template.clone(), bulk_progress, bulk_generating, err_sig); } },
                i { class: "material-icons text-sm", { if *bulk_generating.read() { "hourglass_empty" } else { "auto_fix_high" } } }
            }
            // Export CSV
            button { class: "btn", disabled: *exporting.read() || results.read().items.is_empty(), title: "Export CSV", onclick: { let results=results.clone(); let mut exporting=exporting.clone(); let mut err_sig=error.clone(); move |_| { if results.read().items.is_empty() || *exporting.read(){ return; } exporting.set(true); let rows=results.read().items.clone(); spawn(async move { let res=crate::app::app_export_csv(&rows); if let Err(e)=res { err_sig.set(Some(e)); } exporting.set(false); }); } }, i { class: "material-icons text-sm", { if *exporting.read() { "hourglass_bottom" } else { "download" } } } }
            // Debug toggle
            button { class: "btn", title: "Toggle debug view", onclick: move |_| { let new_view = if *app_view.read()==crate::app::AppView::Explorer { crate::app::AppView::DebugDb } else { crate::app::AppView::Explorer }; app_view.set(new_view); let u=ui.write(); crate::settings::save_settings(&u); }, i { class: "material-icons text-sm", { if *app_view.read()==crate::app::AppView::DebugDb { "dataset" } else { "storage" } } } }
            // Preferences
            button { class: "btn", title: "Preferences", onclick: move |_| { prefs_open.set(true); }, i { class: "material-icons text-sm", "settings" } }
        }
        // Progress indicators for bulk generation / indexing
    if *bulk_generating.read() { span { class: "text-10px text-weak px-2", { let (d,t)=*bulk_progress.read(); format!("Bulk {d}/{t}") } } }
        // Preferences dialog (AI settings + auto index toggle)
        DialogRoot { class: "dialog-backdrop", open: prefs_open(), on_open_change: move |v| prefs_open.set(v),
            DialogContent { class: "dialog",
                button { class: "dialog-close", aria_label: "Close", tabindex: if prefs_open() {"0"} else {"-1"}, onclick: move |_| prefs_open.set(false), "×" }
                DialogTitle { class: "dialog-title", "Preferences" }
                DialogDescription { class: "dialog-description", "Configure AI indexing and prompts." }
                div { class: "flex flex-col gap-2 mt-2", 
                    label { class: "text-10px text-weak", "Vision Prompt Template" }
                    textarea { class: "textarea w-full h-48 bg-muted border border-stroke rounded p-1 font-mono text-10px", value: ui.read().ai_prompt_template.clone(), oninput: move |evt| { let mut s=ui.write(); s.ai_prompt_template=evt.value().clone(); save_settings(&s); } }
                    div { class: "flex items-center gap-2", span { class: "text-10px", "Auto Index" } 
                        Switch { class: "switch", checked: *auto_indexing.read(), on_checked_change: move |v| { auto_indexing.set(v); if let Some(engine)=ai_search_engine.read().as_ref(){ engine.auto_descriptions_enabled.store(v, std::sync::atomic::Ordering::Relaxed); } }, SwitchThumb { class: "switch-thumb" } }
                        span { class: "text-10px text-weak", if *auto_indexing.read() { "ON" } else { "OFF" } }
                    }
                }
            }
        }
    }}
}
