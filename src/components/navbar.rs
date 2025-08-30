use dioxus::prelude::*;
use dioxus_primitives::separator::Separator;
use crate::settings::{UiSettings, save_settings};
use crate::types::{ViewMode, Filters};
use crate::scan::{begin_scan, cancel_scan};
use dioxus_primitives::menubar::{Menubar, MenubarMenu, MenubarTrigger, MenubarContent, MenubarItem};
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem};
use dioxus_primitives::calendar::{Calendar, CalendarGrid, CalendarHeader, CalendarNavigation, CalendarPreviousMonthButton, CalendarNextMonthButton, CalendarSelectMonth, CalendarSelectYear};
use dioxus_primitives::switch::{Switch, SwitchThumb};
use time::{Date, OffsetDateTime};
use std::collections::{BTreeSet, BTreeMap};
use std::path::PathBuf;

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
    pub search_text: Signal<String>,
    pub ai_search_results: Signal<Vec<crate::ai::FileMetadata>>,
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
}

// Minimal visual shell for new Navbar; actions will be filled to parity with NavHamburgerMenu.
#[allow(non_snake_case)]
pub fn NewNavbar(props: NewNavbarProps) -> Element {
    let NewNavbarProps { mut ui, mut view_mode, mut preview_collapsed, mut qa_collapsed, mut drives_collapsed, results, app_view: _app_view, error, mut filters, scan_generation, mut scanning, dir_items, progress, ai_search_engine, selected_paths, mut auto_indexing, mut search_text, mut ai_search_results, mut ai_search_active, mut group_by_category, selected_path: _selected_path, mut nav_history, mut recursive_current, mut only_subdirs, mut scan_started, mut scan_finished, ext_filters, mut ext_enabled, mut excluded_dirs } = props;

    let mut exporting = use_signal(|| false);
    let mut bulk_generating = use_signal(|| false);
    let mut bulk_progress = use_signal(|| (0usize,0usize));
    let mut prefs_open = use_signal(|| false);
    let mut date_menu_after_open = use_signal(|| false);
    let mut date_menu_before_open = use_signal(|| false);
    let mut calendar_after_view = use_signal(|| OffsetDateTime::now_utc().date());
    let mut calendar_before_view = use_signal(|| OffsetDateTime::now_utc().date());

    // convenience clones

    rsx! {
        // Nav container uses utility classes; no inline style overrides
        nav { class: "app-nav flex items-center gap-3 px-2 bg-panel border-b border-stroke h-12",
            // Back / Up navigation controls
            div { class: "flex items-center gap-1 pr-1",
                button { class: "btn px-2 py-1", disabled: nav_history.read().is_empty(),
                    onclick: move |_| {
                        if nav_history.read().is_empty() { return; }
                        // Clone, pop, then set to avoid mutable borrow requirement
                        let mut stack = nav_history.read().clone();
                        if let Some(prev) = stack.pop() {
                            nav_history.set(stack);
                            {
                                let mut f = filters.write();
                                f.root = prev.clone();
                            }
                            only_subdirs.set(false);
                            recursive_current.set(false);
                            scan_started.set(Some(std::time::Instant::now()));
                            scan_finished.set(None);
                            begin_scan(filters.clone(), scan_generation.clone(), scanning.clone(), results.clone(), dir_items.clone(), progress.clone(), false);
                        }
                    },
                    i { class: "material-icons text-[18px] opacity-80", "arrow_back" }
                }
                button { class: "btn px-2 py-1", disabled: filters.read().root.parent().is_none(),
                    onclick: move |_| {
                        let current = filters.read().root.clone();
                        if let Some(parent) = current.parent().map(|p| p.to_path_buf()) {
                            // push current into history then navigate up
                            let mut stack = nav_history.read().clone();
                            stack.push(current.clone());
                            nav_history.set(stack);
                            {
                                let mut f = filters.write();
                                f.root = parent.clone();
                            }
                            only_subdirs.set(false);
                            recursive_current.set(false);
                            scan_started.set(Some(std::time::Instant::now()));
                            scan_finished.set(None);
                            begin_scan(filters.clone(), scan_generation.clone(), scanning.clone(), results.clone(), dir_items.clone(), progress.clone(), false);
                        }
                    },
                    i { class: "material-icons text-[18px] opacity-80", "arrow_upward" }
                }
            }
            Menubar { class: "menubar flex-1",
                // File
                MenubarMenu { class: "menubar-menu", index: 0usize,
                    MenubarTrigger { class: "menubar-trigger", "File" }
                    // Rely on global .menubar-content CSS (no inline position/z-index overrides)
                    MenubarContent { class: "menubar-content",
                        MenubarItem { index:0usize, class:"menubar-item", value:"export",
                            on_select: move |_| { if results.read().items.is_empty() || *exporting.read(){ return; } exporting.set(true); let rows=results.read().items.clone(); let mut exporting_flag=exporting.clone(); let mut err_sig=error.clone(); spawn(async move { let res=crate::app::app_export_csv(&rows); if let Err(e)=res { err_sig.set(Some(e)); } exporting_flag.set(false); }); },
                            i { class: "material-icons text-[14px] opacity-70", "download" }
                            span { "Export CSV" }
                        }
                        Separator { horizontal: true }
                        MenubarItem { index:1usize, class:"menubar-item", value:"prefs", on_select: move |_| prefs_open.set(true),
                            i { class: "material-icons text-[14px] opacity-70", "settings" }
                            span { "Preferences" }
                        }
                    }
                }
                // View
                MenubarMenu { class: "menubar-menu", index: 1usize,
                    MenubarTrigger { class: "menubar-trigger", "View" }
                    MenubarContent { class: "menubar-content",
                        MenubarItem { index:0usize, class:"menubar-item", value:"icons", on_select: move |_| { view_mode.set(ViewMode::Icons); let mut s=ui.write(); s.view_mode=Some("icons".into()); save_settings(&s); }, "Icons" }
                        Separator { horizontal: true }
                        MenubarItem { index:1usize, class:"menubar-item", value:"details", on_select: move |_| { view_mode.set(ViewMode::Details); let mut s=ui.write(); s.view_mode=Some("details".into()); save_settings(&s); }, "Details" }
                        Separator { horizontal: true }
                        MenubarItem { index:2usize, class:"menubar-item", value:"toggle-preview", on_select: move |_| { let new_val=!*preview_collapsed.read(); preview_collapsed.set(new_val); let mut s=ui.write(); s.preview_collapsed=new_val; save_settings(&s); }, { if *preview_collapsed.read() { "Show Preview" } else { "Hide Preview" } } }
                        Separator { horizontal: true }
                        MenubarItem { index:3usize, class:"menubar-item", value:"toggle-left", on_select: move |_| { let hide = !(*qa_collapsed.read() && *drives_collapsed.read()); qa_collapsed.set(hide); drives_collapsed.set(hide); let mut s=ui.write(); s.qa_collapsed=hide; s.drives_collapsed=hide; save_settings(&s); }, { if *qa_collapsed.read() && *drives_collapsed.read() { "Show Left" } else { "Hide Left" } } }
                        Separator { horizontal: true }
                        MenubarItem { index:4usize, class:"menubar-item", value:"group-cat", on_select: move |_| { let cur=*group_by_category.read(); group_by_category.set(!cur); }, { if *group_by_category.read() { "Ungroup Categories" } else { "Group by Category" } } }
                    }
                }
                // Scan
                MenubarMenu { class: "menubar-menu", index: 2usize,
                    MenubarTrigger { class: "menubar-trigger", "Scan" }
                    MenubarContent { class: "menubar-content",
                        MenubarItem { index:0usize, class:"menubar-item", value:"scan-recursive", disabled:*scanning.read(), on_select: { let filters=filters.clone(); let scan_generation=scan_generation.clone(); let scanning=scanning.clone(); let results=results.clone(); let dir_items=dir_items.clone(); let progress=progress.clone(); move |_| { if *scanning.read(){return;} begin_scan(filters.clone(), scan_generation.clone(), scanning.clone(), results.clone(), dir_items.clone(), progress.clone(), true); } }, "Recursive Scan" }
                        Separator { horizontal: true }
                        MenubarItem { index:1usize, class:"menubar-item", value:"cancel-scan", disabled:! *scanning.read(), on_select: move |_| { if !*scanning.read(){return;} cancel_scan(); scanning.set(false); }, "Cancel Scan" }
                        Separator { horizontal: true }
                        MenubarItem { index:2usize, class:"menubar-item", value:"bulk-generate", disabled:*bulk_generating.read() || ai_search_engine.read().is_none(), on_select: move |_| { crate::ai::bulk::spawn_bulk_generate(ai_search_engine.read().clone(), results.read().items.clone(), ui.read().ai_prompt_template.clone(), bulk_progress.clone(), bulk_generating.clone(), error.clone()); }, { if *bulk_generating.read() { "Generating..." } else { "Bulk Generate" } } }
                    }
                }
                // AI
                MenubarMenu { class: "menubar-menu", index: 3usize,
                    MenubarTrigger { class: "menubar-trigger", "AI" }
                    MenubarContent { class: "menubar-content",
                        Separator { horizontal: true }
                        MenubarItem { index:0usize, class:"menubar-item", value:"toggle-ai", on_select: move |_| { let active_now=*ai_search_active.read(); let new_state=!active_now; ai_search_active.set(new_state); if !new_state { ai_search_results.set(Vec::new()); } }, { if *ai_search_active.read() { "Disable AI Search" } else { "Enable AI Search" } } }
                        Separator { horizontal: true }
                        MenubarItem { index:1usize, class:"menubar-item", value:"index-selected", disabled: ai_search_engine.read().is_none(), on_select: move |_| { if let Some(engine)=ai_search_engine.read().as_ref(){ engine.auto_descriptions_enabled.store(*auto_indexing.read(), std::sync::atomic::Ordering::Relaxed); let selected=selected_paths.read().clone(); let engine2=engine.clone(); let mut err_sig=error.clone(); spawn(async move { let mut queued=0usize; for p in selected.iter(){ if let Ok(md)=std::fs::metadata(p){ let ext=p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase(); let kind= if crate::types::IMAGE_EXTS.iter().any(|e| *e==ext){crate::types::MediaKind::Image}else if crate::types::VIDEO_EXTS.iter().any(|e| *e==ext){crate::types::MediaKind::Video}else{crate::types::MediaKind::Other}; let ff=crate::types::FoundFile { path:p.clone(), modified:None, created:None, size:Some(md.len()), kind, thumb_data:None }; if engine2.enqueue_index(crate::ai::found_file_to_metadata(&ff)).await { queued+=1; } } } if queued==0 { err_sig.set(Some("No files indexed (selection empty or unsupported)".into())); } }); } }, "Index Selected" }
                        Separator { horizontal: true }
                        MenubarItem { index:2usize, class:"menubar-item", value:"auto-index", on_select: move |_| { let new=!*auto_indexing.read(); auto_indexing.set(new); if let Some(engine)=ai_search_engine.read().as_ref(){ engine.auto_descriptions_enabled.store(new, std::sync::atomic::Ordering::Relaxed); } }, { if *auto_indexing.read() { "Disable Auto Index" } else { "Enable Auto Index" } } }
                    }
                }
                // Filters
                MenubarMenu { class: "menubar-menu", index: 4usize,
                    MenubarTrigger { class: "menubar-trigger", "Filters" }
                    MenubarContent { class: "menubar-content flex flex-col gap-3 w-[340px] max-h-[420px] overflow-auto p-2",
                        // Media toggles
                        div { class: "grid grid-cols-3 gap-2 text-10px",
                            div { class: "flex items-center gap-1", span { class: "text-8px text-weak", "Img" } 
                                Switch { class: "switch", checked: filters.read().include_images,
                                    on_checked_change: move |v: bool| { let root={ let mut f=filters.write(); f.include_images=v; f.root.clone() }; scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); let rec=*recursive_current.read(); if crate::app::shallow_should_scan(&root) || rec { only_subdirs.set(false); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, rec); } },
                                    SwitchThumb { class: "switch-thumb" }
                                }
                            }
                            div { class: "flex items-center gap-1", span { class: "text-8px text-weak", "Vid" }
                                Switch { class: "switch", checked: filters.read().include_videos,
                                    on_checked_change: move |v: bool| { let root={ let mut f=filters.write(); f.include_videos=v; f.root.clone() }; scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); let rec=*recursive_current.read(); if crate::app::shallow_should_scan(&root) || rec { only_subdirs.set(false); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, rec); } },
                                    SwitchThumb { class: "switch-thumb" }
                                }
                            }
                            div { class: "flex items-center gap-1", span { class: "text-8px text-weak", "Thumbs" }
                                Switch { class: "switch", checked: filters.read().only_with_thumb,
                                    on_checked_change: move |v: bool| { filters.write().only_with_thumb=v; },
                                    SwitchThumb { class: "switch-thumb" }
                                }
                            }
                        }
                        // Extensions
                        if !ext_filters.read().is_empty() { div { class: "flex flex-wrap gap-2",
                            for ext in ext_filters.read().iter() { 
                                { let ext_name = ext.clone(); 
                                  let active = *ext_enabled.read().get(&ext_name).unwrap_or(&true);
                                  let inactive_cls = if active { "" } else { " inactive" };
                                  rsx! { div { key: "ext-{ext_name}", class: "ext-chip flex items-center gap-1 text-11px px-1.5 py-0.5 rounded-md border cursor-pointer bg-muted/40{inactive_cls}",
                                        span { ".{ext_name}" }
                                        Switch { class: "switch", checked: active, on_checked_change: move |v: bool| { let mut map=ext_enabled.write(); map.insert(ext_name.clone(), v); }, SwitchThumb { class: "switch-thumb" } }
                                    } }
                                }
                            }
                        } }
                        if !excluded_dirs.read().is_empty() { div { class: "flex items-center gap-2 mt-1", span { class: "text-11px", "Excluded: {excluded_dirs.read().len()} dirs" } button { class: "btn text-10px px-2 py-0.5", onclick: move |_| excluded_dirs.write().clear(), "Clear" } } }
                        // Modified After
                        DropdownMenu { class: "dropdown-menu w-full", default_open: false,
                            DropdownMenuTrigger { class: "dropdown-menu-trigger w-full flex justify-between",
                                button { class: "flex-1 text-left", onclick: move |_| date_menu_after_open.set(!date_menu_after_open()), { filters.read().modified_after.clone().unwrap_or_else(|| "After: (any)".into()) } }
                            }
                            if date_menu_after_open() { DropdownMenuContent { class: "dropdown-menu-content p-2",
                                Calendar { selected_date: filters.read().modified_after.as_ref().and_then(|s| parse_date_str(s)),
                                    on_date_change: move |d: Option<Date>| {
                                        // capture new value first, then update inside scoped block so write lock ends before rescan
                                        let new_val = d.map(|dd| dd.to_string());
                                        {
                                            let mut f = filters.write();
                                            f.modified_after = new_val;
                                        } // write lock dropped here
                                        date_menu_after_open.set(false);
                                        maybe_trigger_rescan(&filters, scan_generation, scanning, results, dir_items, progress);
                                    },
                                    view_date: calendar_after_view(),
                                    on_view_change: move |new_view: Date| calendar_after_view.set(new_view),
                                    min_date: Date::from_calendar_date(1995, time::Month::July, 21).unwrap(),
                                    max_date: Date::from_calendar_date(2035, time::Month::September, 11).unwrap(),
                                    CalendarHeader { CalendarNavigation { CalendarPreviousMonthButton {} CalendarSelectMonth {} CalendarSelectYear {} CalendarNextMonthButton {} } }
                                    CalendarGrid {}
                                }
                                DropdownMenuItem::<&'static str> { class: "dropdown-menu-item", value: "clear_after", index:0usize, on_select: move |_| {
                                    {
                                        let mut f = filters.write();
                                        f.modified_after = None;
                                    } // drop write lock
                                    date_menu_after_open.set(false);
                                    maybe_trigger_rescan(&filters, scan_generation, scanning, results, dir_items, progress);
                                }, "Clear" }
                            } }
                        }
                        // Modified Before
                        DropdownMenu { class: "dropdown-menu w-full", default_open: false,
                            DropdownMenuTrigger { class: "dropdown-menu-trigger w-full flex justify-between",
                                button { class: "flex-1 text-left", onclick: move |_| date_menu_before_open.set(!date_menu_before_open()), { filters.read().modified_before.clone().unwrap_or_else(|| "Before: (any)".into()) } }
                            }
                            if date_menu_before_open() { DropdownMenuContent { class: "dropdown-menu-content p-2",
                                Calendar { selected_date: filters.read().modified_before.as_ref().and_then(|s| parse_date_str(s)),
                                    on_date_change: move |d: Option<Date>| {
                                        let new_val = d.map(|dd| dd.to_string());
                                        {
                                            let mut f = filters.write();
                                            f.modified_before = new_val;
                                        }
                                        date_menu_before_open.set(false);
                                        maybe_trigger_rescan(&filters, scan_generation, scanning, results, dir_items, progress);
                                    },
                                    view_date: calendar_before_view(),
                                    on_view_change: move |new_view: Date| calendar_before_view.set(new_view),
                                    min_date: Date::from_calendar_date(1995, time::Month::July, 21).unwrap(),
                                    max_date: Date::from_calendar_date(2035, time::Month::September, 11).unwrap(),
                                    CalendarHeader { CalendarNavigation { CalendarPreviousMonthButton {} CalendarSelectMonth {} CalendarSelectYear {} CalendarNextMonthButton {} } }
                                    CalendarGrid {}
                                }
                                DropdownMenuItem::<&'static str> { class: "dropdown-menu-item", value: "clear_before", index:0usize, on_select: move |_| {
                                    {
                                        let mut f = filters.write();
                                        f.modified_before = None;
                                    }
                                    date_menu_before_open.set(false);
                                    maybe_trigger_rescan(&filters, scan_generation, scanning, results, dir_items, progress);
                                }, "Clear" }
                            } }
                        }
                    }
                }
            }
            // Right side
            div { class: "flex items-center gap-2",
                input { class: "w-60 bg-muted text-var-text border border-stroke rounded-md px-2 py-1 text-sm", placeholder: if *ai_search_active.read() { "Describe & Enter" } else { "Search..." }, value: "{search_text.read().clone()}",
                    oninput: move |e| { let query=e.value(); search_text.set(query.clone()); if *ai_search_active.read(){ if query.trim().is_empty(){ ai_search_results.set(Vec::new()); return; } if ai_search_engine.read().is_none(){ return; } let engine=ai_search_engine.read().clone(); let mut res_sig=ai_search_results.clone(); let q2=query.clone(); spawn(async move { if let Some(engine)=engine { if let Ok(r)=engine.search(&q2).await { res_sig.set(r); } } }); } }
                }
                if *ai_search_active.read() { span { class: "text-10px px-2 py-0.5 rounded bg-accent/20 text-accent", "AI" } }
                if *bulk_generating.read() { span { class: "text-10px text-weak", { let (d,t)=*bulk_progress.read(); format!("Bulk {d}/{t}") } } }
            }
        }
        dioxus_primitives::dialog::DialogRoot { class: "dialog-backdrop", open: prefs_open(), on_open_change: move |v| prefs_open.set(v),
            dioxus_primitives::dialog::DialogContent { class: "dialog",
                button { class: "dialog-close", aria_label: "Close", tabindex: if prefs_open() {"0"} else {"-1"}, onclick: move |_| prefs_open.set(false), "×" }
                dioxus_primitives::dialog::DialogTitle { class: "dialog-title", "Preferences" }
                dioxus_primitives::dialog::DialogDescription { class: "dialog-description", "Configure AI indexing and prompts." }
                div { class: "flex flex-col gap-2 mt-2",
                    label { class: "text-10px text-weak", "Vision Prompt Template" }
                    textarea { class: "textarea w-full h-48 bg-muted border border-stroke rounded p-1 font-mono text-10px", value: ui.read().ai_prompt_template.clone(), oninput: move |evt| { let mut s=ui.write(); s.ai_prompt_template=evt.value().clone(); save_settings(&s); } }
                }
            }
        }
    }
}

fn parse_date_str(s: &str) -> Option<Date> { Date::parse(s, time::macros::format_description!("[year]-[month]-[day] ")).ok().or_else(|| Date::parse(s, time::macros::format_description!("[year]-[month]-[day]")).ok()) }

#[allow(clippy::too_many_arguments)]
fn maybe_trigger_rescan(
    filters: &Signal<Filters>,
    scan_generation: Signal<u64>,
    scanning: Signal<bool>,
    results: Signal<crate::types::ScanResults>,
    dir_items: Signal<Vec<crate::types::DirItem>>,
    progress: Signal<Option<(usize,usize)>>,
) {
    if crate::app::shallow_should_scan(&filters.read().root) {
        begin_scan(filters.clone(), scan_generation, scanning, results, dir_items, progress, false);
    }
}

