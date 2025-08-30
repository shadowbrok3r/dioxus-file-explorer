use dioxus::prelude::*;
use crate::settings::{UiSettings, save_settings};
use crate::types::ViewMode;
use crate::scan::{begin_scan, cancel_scan};

#[derive(Props, PartialEq, Clone)]
pub struct NewNavbarProps {
    pub ui: Signal<UiSettings>,
    pub view_mode: Signal<ViewMode>,
    pub preview_collapsed: Signal<bool>,
    pub qa_collapsed: Signal<bool>,
    pub drives_collapsed: Signal<bool>,
    pub results: Signal<crate::types::ScanResults>,
    pub app_view: Signal<crate::app::AppView>,
    pub error: Signal<Option<String>>,
    pub filters: Signal<crate::types::Filters>,
    pub scan_generation: Signal<u64>,
    pub scanning: Signal<bool>,
    pub dir_items: Signal<Vec<crate::types::DirItem>>,
    pub progress: Signal<Option<(usize, usize)>>,
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>,
    pub auto_indexing: Signal<bool>,
}

// Minimal visual shell for new Navbar; actions will be filled to parity with NavHamburgerMenu.
#[allow(non_snake_case)]
pub fn NewNavbar(props: NewNavbarProps) -> Element {
    let NewNavbarProps { mut ui, mut view_mode, mut preview_collapsed, mut qa_collapsed, mut drives_collapsed, results, mut app_view, error, filters, scan_generation, mut scanning, dir_items, progress, ai_search_engine, selected_paths, mut auto_indexing } = props;

    // Local expansion / panel states (settings, bulk ops etc.)
    let mut show_ai_settings = use_signal(|| false);
    let mut exporting = use_signal(|| false);
    let mut bulk_generating = use_signal(|| false);
    let mut bulk_progress = use_signal(|| (0usize,0usize));

    rsx! {
        nav { class: "flex items-center gap-2 ml-auto",
            // View mode toggles
            button { class: if *view_mode.read()==ViewMode::Icons {"btn primary"} else {"btn"}, onclick: move |_| { view_mode.set(ViewMode::Icons); let mut s = ui.write(); s.view_mode = Some("icons".into()); save_settings(&s); }, i { class: "material-icons text-sm", "grid_view" } }
            button { class: if *view_mode.read()==ViewMode::Details {"btn primary"} else {"btn"}, onclick: move |_| { view_mode.set(ViewMode::Details); let mut s = ui.write(); s.view_mode = Some("details".into()); save_settings(&s); }, i { class: "material-icons text-sm", "view_list" } }

            // Pane visibility toggles (Switch style)
            div { class: "flex items-center gap-1 text-10px",
                span { class: "text-8px text-weak", "Preview" }
                input { r#type: "checkbox", class: "appearance-none w-8 h-4 rounded-full bg-muted relative cursor-pointer",
                    checked: !*preview_collapsed.read(),
                    oninput: move |_| {
                        let currently_visible = !*preview_collapsed.read();
                        let new_visible = !currently_visible; // toggle
                        preview_collapsed.set(!new_visible); // store collapsed state
                        let mut s = ui.write();
                        s.preview_collapsed = !new_visible;
                        save_settings(&s);
                    }
                }
            }
            div { class: "flex items-center gap-1 text-10px",
                span { class: "text-8px text-weak", "Left" }
                input { r#type: "checkbox", class: "appearance-none w-8 h-4 rounded-full bg-muted relative cursor-pointer",
                    checked: !(*qa_collapsed.read() && *drives_collapsed.read()),
                    oninput: move |_| {
                        let currently_visible = !(*qa_collapsed.read() && *drives_collapsed.read());
                        let new_visible = !currently_visible;
                        let new_hide = !new_visible; // collapsed state for each
                        qa_collapsed.set(new_hide);
                        drives_collapsed.set(new_hide);
                        let mut s = ui.write();
                        s.qa_collapsed = new_hide;
                        s.drives_collapsed = new_hide;
                        save_settings(&s);
                    }
                }
            }

            // Recursive scan
            button { class: "btn", disabled: *scanning.read(), onclick: { let filters=filters.clone(); let scan_generation=scan_generation.clone(); let scanning=scanning.clone(); let results=results.clone(); let dir_items=dir_items.clone(); let progress=progress.clone(); move |_| { if *scanning.read(){return;} begin_scan(filters.clone(), scan_generation.clone(), scanning.clone(), results.clone(), dir_items.clone(), progress.clone(), true); } }, i { class: "material-icons text-sm", "travel_explore" } }
            // Cancel scan
            button { class: "btn", disabled: !*scanning.read(), onclick: move |_| { if !*scanning.read(){return;} cancel_scan(); scanning.set(false); }, i { class: "material-icons text-sm", "stop_circle" } }

            // Index selected files
            button { class: "btn", disabled: ai_search_engine.read().is_none(), onclick: move |_| {
                if let Some(engine) = ai_search_engine.read().as_ref() {
                    engine.auto_descriptions_enabled.store(*auto_indexing.read(), std::sync::atomic::Ordering::Relaxed);
                    let selected = selected_paths.read().clone();
                    let engine2 = engine.clone();
                    let mut err_sig = error.clone();
                    spawn(async move {
                        let mut queued=0usize;
                        for p in selected.iter() {
                            if let Ok(md) = std::fs::metadata(p) {
                                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                                let kind = if crate::types::IMAGE_EXTS.iter().any(|e| *e == ext) { crate::types::MediaKind::Image } else if crate::types::VIDEO_EXTS.iter().any(|e| *e == ext) { crate::types::MediaKind::Video } else { crate::types::MediaKind::Other };
                                let ff = crate::types::FoundFile { path: p.clone(), modified: None, created: None, size: Some(md.len()), kind, thumb_data: None };
                                if engine2.enqueue_index(crate::ai::found_file_to_metadata(&ff)).await { queued+=1; }
                            }
                        }
                        log::info!("[Navbar] Queued {queued} files for indexing");
                        if queued==0 { err_sig.set(Some("No files indexed (selection empty or unsupported)".into())); }
                    });
                }
            }, i { class: "material-icons text-sm", "memory" } }

            // Toggle AI auto indexing (Switch)
            div { class: "flex items-center gap-1 text-10px",
                span { class: "text-8px text-weak", "AutoIdx" }
                // Switch component placeholder (replace with actual Switch once available in project primitives)
                input { r#type: "checkbox", class: "appearance-none w-8 h-4 rounded-full bg-muted relative cursor-pointer",
                    checked: *auto_indexing.read(),
                    oninput: move |_| {
                        let newv = !*auto_indexing.read();
                        auto_indexing.set(newv);
                        if let Some(engine)=ai_search_engine.read().as_ref(){
                            engine.auto_descriptions_enabled.store(newv, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            }

            // Toggle debug view (Switch)
            div { class: "flex items-center gap-1 text-10px",
                span { class: "text-8px text-weak", "Debug" }
                input { r#type: "checkbox", class: "appearance-none w-8 h-4 rounded-full bg-muted relative cursor-pointer",
                    checked: *app_view.read()==crate::app::AppView::DebugDb,
                    oninput: move |_| {
                        let new_view = if *app_view.read()==crate::app::AppView::Explorer { crate::app::AppView::DebugDb } else { crate::app::AppView::Explorer };
                        app_view.set(new_view);
                        let u = ui.write();
                        save_settings(&u);
                    }
                }
            }

            // Bulk generate image descriptions (simplified subset - images only)
            button { class: "btn", disabled: *bulk_generating.read() || ai_search_engine.read().is_none(), onclick: move |_| {
                if *bulk_generating.read() { return; }
                bulk_generating.set(true);
                let engine_opt = ai_search_engine.read().clone();
                let rows = results.read().items.clone();
                let mut prog = bulk_progress.clone();
                let mut bulk_flag = bulk_generating.clone();
                let mut err_sig = error.clone();
                let mut ui_sig = ui.clone();
                spawn(async move {
                    let total = rows.len();
                    prog.set((0,total));
                    if let Some(engine) = engine_opt { 
                        for (idx,f) in rows.iter().enumerate() {
                            let ext = f.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
                            if !crate::types::IMAGE_EXTS.iter().any(|e| *e==ext) { prog.set((idx+1,total)); continue; }
                            #[cfg(feature="joycaption")]
                            if crate::ai::joycaption_adapter::is_enabled() {
                                if let Ok(bytes) = tokio::fs::read(&f.path).await {
                                    let prompt = ui_sig.read().ai_prompt_template.clone();
                                    let mut interim = String::new();
                                    let _ = crate::ai::joycaption_adapter::stream_describe_bytes_with_callback(bytes, &prompt, |frag| { interim.push_str(frag); }).await;
                                    if let Some(val) = crate::ai::joycaption_adapter::extract_json_vision(&interim) {
                                        if let Ok(vd) = serde_json::from_value::<crate::ai::generate::VisionDescription>(val) { let _ = engine.apply_vision_description(&f.path.display().to_string(), &vd).await; }
                                        else { let _ = engine.set_file_description(&f.path.display().to_string(), &interim).await; }
                                    } else { let _ = engine.set_file_description(&f.path.display().to_string(), &interim).await; }
                                }
                            } else {
                                if let Some(vd) = engine.generate_vision_description(&f.path).await { let _ = engine.apply_vision_description(&f.path.display().to_string(), &vd).await; }
                            }
                            prog.set((idx+1,total));
                        }
                    } else { err_sig.set(Some("AI engine not initialized".into())); }
                    bulk_flag.set(false);
                });
            }, i { class: "material-icons text-sm", "auto_fix_high" } }
            if *bulk_generating.read() { span { class: "text-10px text-weak", { let (d,t)=*bulk_progress.read(); format!("{d}/{t}") } } }

            // Export CSV
            button { class: "btn", disabled: *exporting.read() || results.read().items.is_empty(), onclick: move |_| {
                if results.read().items.is_empty() || *exporting.read() { return; }
                exporting.set(true);
                let rows = results.read().items.clone();
                let mut exporting_flag = exporting.clone();
                let mut err_sig = error.clone();
                spawn(async move { let res = crate::app::app_export_csv(&rows); if let Err(e)=res { err_sig.set(Some(e)); } exporting_flag.set(false); });
            }, i { class: "material-icons text-sm", "download" } }

            // AI Settings inline toggle
            button { class: if *show_ai_settings.read() {"btn primary"} else {"btn"}, onclick: move |_| { let cur=*show_ai_settings.read(); show_ai_settings.set(!cur); }, i { class: "material-icons text-sm", "settings" } }
        }
        if *show_ai_settings.read() {
            div { class: "flex flex-col gap-1 p-2 bg-panel border border-stroke rounded-md mt-2 ml-auto w-96", 
                label { class: "text-10px text-weak", "Vision Prompt Template" }
                textarea { class: "textarea w-full h-40 bg-muted border border-stroke rounded p-1 font-mono text-10px", value: ui.read().ai_prompt_template.clone(), oninput: move |evt| { let mut s=ui.write(); s.ai_prompt_template = evt.value().clone(); save_settings(&s); } }
                span { class: "text-8px text-weak", "Template must output ONLY JSON: description, caption, tags[], category." }
            }
        }
    }
}
