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

    // Drain scan messages
    if let Some(rx) = rx_state.read().as_ref() {
        for msg in rx.try_iter() {
            match msg {
                ScanMsg::Found(item) => results.write().items.push(item),
                ScanMsg::UpdateThumb { path, thumb } => {
                    if let Some(it) = results.write().items.iter_mut().find(|f| f.path == path) {
                        it.thumb_data = Some(thumb);
                    }
                }
                ScanMsg::Progress { scanned, total } => progress.set(Some((scanned, total))),
                ScanMsg::Error(e) => { error.set(Some(e)); scanning.set(false); },
                ScanMsg::Done => { scanning.set(false); },
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

    let s_now = sort.read().clone();
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { href: "https://fonts.googleapis.com/icon?family=Material+Icons", rel: "stylesheet" }

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
                        if p.exists() { let new_root = p.clone(); { let mut f = filters.write(); f.root = new_root.clone(); } recursive_current.set(false); if shallow_should_scan(&new_root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); dir_items.set(list_dir_items(new_root).unwrap_or_default()); results.set(Default::default()); } }
                    }
                }
            }
            // Up directory
            button { class: "btn", disabled: filters.read().root.parent().is_none(), onclick: move |_| {
                    let mut new_root = filters.read().root.clone();
                    if new_root.pop() {
                        { let mut f = filters.write(); path_text.set(new_root.display().to_string()); f.root = new_root; }
                        recursive_current.set(false);
                        if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); dir_items.set(list_dir_items(filters.read().root.clone()).unwrap_or_default()); results.set(Default::default()); }
                    }
                }, i { class: "material-icons", "arrow_upward" } }
            // Deep scan trigger
            button { class: "btn", title: "Deep recursive scan all subfolders", onclick: move |_| {
                    if scanning.read().clone() { rx_state.set(None); scanning.set(false); }
                    recursive_current.set(true);
                    begin_scan(filters, rx_state, scanning, results, dir_items, progress, true);
                }, i { class: "material-icons", "travel_explore" } }
            // Cancel current scan
            button { class: "btn", disabled: !scanning.read().clone(), onclick: move |_| { rx_state.set(None); scanning.set(false); }, i { class: "material-icons", "close" } }
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
            // Spacer pushes preview toggle to far right
            span { class: "flex-1" }
            // Preview pane toggle (far right)
            button { class: "btn", title: if *preview_collapsed.read() { "Show preview pane" } else { "Hide preview pane" }, onclick: move |_| { let curr = *preview_collapsed.read(); preview_collapsed.set(!curr); let mut s = ui.write(); s.preview_collapsed = !curr; s.preview_width = *preview_width.read(); save_settings(&s); },
                i { class: "material-icons", { if *preview_collapsed.read() { "visibility" } else { "visibility_off" } } }
            }
        }

        // Progress bar + status details
        if scanning.read().clone() {
            { let prog = progress.read().clone(); let started = scan_started.read().clone(); let elapsed = started.map(|st| st.elapsed()).unwrap_or_default(); let secs = elapsed.as_secs_f32();
                rsx!{ div { class: "w-full bg-muted overflow-hidden flex flex-col", style: "position:relative;",
                    // bar
                    if let Some((scanned,total)) = prog {
                        if total > 0 { { let pct = (scanned as f32 / total.max(1) as f32 * 100.0).min(100.0); rsx!{ div { class: "h-1 bg-accent", style: "width:{pct}%; transition:width .15s linear;" } } } }
                        else { div { class: "h-1 bg-accent animate-pulse", style: "width:40%; position:absolute; left:0; animation: scan-indeterminate 1.2s linear infinite;" } }
                    } else { div { class: "h-1 bg-accent animate-pulse", style: "width:30%;" } }
                    // info line
                    div { class: "flex flex-wrap gap-3 px-2 py-1 text-11px text-weak items-center", style: "user-select:none;",
                        // Status chip indicating scan mode
                        span { class: "px-1.5 py-0.5 rounded-full text-10px tracking-wide uppercase font-medium bg-accent/10 border border-accent text-accent", { if *recursive_current.read() { "Deep" } else { "Shallow" } } }
                        if let Some((scanned,total)) = prog {
                            if total > 0 {
                                {{
                                    let pct = scanned as f32 * 100.0 / total.max(1) as f32;
                                    let pct_rounded = pct.round() as i32;
                                    let rate = if secs > 0.2 { scanned as f32 / secs } else { 0.0 };
                                    let remain = if rate > 0.1 { (total.saturating_sub(scanned) as f32 / rate).max(0.0) } else { 0.0 };
                                    rsx! {
                                        span { "{scanned} / {total} ({pct_rounded}%)" }
                                        if rate > 0.1 { span { "{rate:.1} items/s" } }
                                        if remain > 0.2 { span { "~ {remain:.1}s left" } }
                                    }
                                }}
                            } else {
                                {{
                                    let rate = if secs > 0.2 { scanned as f32 / secs } else { 0.0 };
                                    rsx! {
                                        span { "{scanned} items" }
                                        if rate > 0.1 { span { "{rate:.1} items/s" } }
                                        span { "elapsed {secs:.1}s" }
                                    }
                                }}
                            }
                        } else { span { "Starting scan..." } }
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
                    input { r#type: "checkbox", checked: filters.read().include_images, oninput: move |_| { let mut f = filters.write(); f.include_images = !f.include_images; } }
                    span { " Images" }
                }
                label { class: "chk",
                    input { r#type: "checkbox", checked: filters.read().include_videos, oninput: move |_| { let mut f = filters.write(); f.include_videos = !f.include_videos; } }
                    span { " Videos" }
                }
            }
            div { class: "filter-group",
                button { class: "btn", onclick: move |_| {
                        { let mut f = filters.write(); f.date_field = match f.date_field { DateField::Modified => DateField::Created, DateField::Created => DateField::Modified }; }
                        recursive_current.set(false);
                        if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); }
                    }, span { "Date: " } strong { match filters.read().date_field { DateField::Modified => "Modified", DateField::Created => "Created" } } }
                label { " After:" }
                input { r#type: "date", value: filters.read().modified_after.clone().unwrap_or_default(), oninput: move |evt| { { let mut f = filters.write(); f.modified_after = Some(evt.value()); } recursive_current.set(false); if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); } } }
                label { " Before:" }
                input { r#type: "date", value: filters.read().modified_before.clone().unwrap_or_default(), oninput: move |evt| { { let mut f = filters.write(); f.modified_before = Some(evt.value()); } recursive_current.set(false); if shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); begin_scan(filters, rx_state, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); } } }
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
                if results.read().items.is_empty() {
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
                } else {
                    if view_mode.read().clone() == ViewMode::Icons {
                        ul { class: "results",
                            for item in results.read().items.iter() {
                                { let item_path = item.path.clone(); let item_path_click = item_path.clone(); let item_path_open = item_path.clone(); let path_str = item_path.display().to_string(); let mtime = item.modified.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "-".to_string()); let size = item.size.map(|s| format_size(s, DECIMAL)).unwrap_or_else(|| "-".to_string());
                                    rsx! { li { key: "{path_str}", onclick: move |_| { selected_path.set(Some(item_path_click.clone())); if *preview_collapsed.read() { preview_collapsed.set(false); let mut s = ui.write(); s.preview_collapsed = false; save_settings(&s); } }, ondoubleclick: move |_| { let _ = open::that(&item_path_open); },
                                        if let Some(img) = &item.thumb_data { img { class: "h-16 w-16 rounded-md object-contain bg-111216 border border-stroke", src: "{img}" } } else { i { class: "material-icons text-22px file-icon", {item.icon_name()} } }
                                        div { class: "meta",
                                            h3 { class: "name", {Path::new(&path_str).file_name().and_then(|s| s.to_str()).unwrap_or(&path_str)} }
                                            div { class: "sub", span { class: "mtime", "{mtime}" } span { class: "size", "{size}" } code { class: "path", {path_str} } }
                                        }
                                    } }
                                }
                            }
                        }
                    } else {
                        // Details view header with sorting
                        div { class: "results-header", style: "display:grid; grid-template-columns: 24px 2fr 1.2fr 1.2fr .8fr .8fr; gap:10px; align-items:center; padding:6px 10px; color: var(--text-weak); border-bottom: 1px solid var(--stroke);",
                            span { "" }
                            button { class: "btn", onclick: move |_| { let mut s = sort.write(); s.asc = if matches!(s.by, SortBy::Name) { !s.asc } else { true }; s.by = SortBy::Name; let mut u = ui.write(); u.sort = Some(s.clone()); save_settings(&u); },
                                { if matches!(s_now.by, SortBy::Name) { format!("Name {}", if s_now.asc { "▲" } else { "▼" }) } else { "Name".to_string() } }
                            }
                            button { class: "btn", onclick: move |_| { let mut s = sort.write(); s.asc = if matches!(s.by, SortBy::Modified) { !s.asc } else { false }; s.by = SortBy::Modified; let mut u = ui.write(); u.sort = Some(s.clone()); save_settings(&u); },
                                { if matches!(s_now.by, SortBy::Modified) { format!("Modified {}", if s_now.asc { "▲" } else { "▼" }) } else { "Modified".to_string() } }
                            }
                            button { class: "btn", onclick: move |_| { let mut s = sort.write(); s.asc = if matches!(s.by, SortBy::Created) { !s.asc } else { false }; s.by = SortBy::Created; let mut u = ui.write(); u.sort = Some(s.clone()); save_settings(&u); },
                                { if matches!(s_now.by, SortBy::Created) { format!("Created {}", if s_now.asc { "▲" } else { "▼" }) } else { "Created".to_string() } }
                            }
                            button { class: "btn", onclick: move |_| { let mut s = sort.write(); s.asc = if matches!(s.by, SortBy::Size) { !s.asc } else { false }; s.by = SortBy::Size; let mut u = ui.write(); u.sort = Some(s.clone()); save_settings(&u); },
                                { if matches!(s_now.by, SortBy::Size) { format!("Size {}", if s_now.asc { "▲" } else { "▼" }) } else { "Size".to_string() } }
                            }
                            button { class: "btn", onclick: move |_| { let mut s = sort.write(); s.asc = if matches!(s.by, SortBy::Type) { !s.asc } else { true }; s.by = SortBy::Type; let mut u = ui.write(); u.sort = Some(s.clone()); save_settings(&u); },
                                { if matches!(s_now.by, SortBy::Type) { format!("Type {}", if s_now.asc { "▲" } else { "▼" }) } else { "Type".to_string() } }
                            }
                        }
                        ul { class: "results",
                            for item in {
                                let mut v = results.read().items.clone();
                                let s = sort.read().clone();
                                v.sort_by(|a,b| {
                                    let ord = match s.by {
                                        SortBy::Name => a.path.file_name().and_then(|x| x.to_str()).cmp(&b.path.file_name().and_then(|x| x.to_str())),
                                        SortBy::Modified => a.modified.cmp(&b.modified),
                                        SortBy::Created => a.created.cmp(&b.created),
                                        SortBy::Size => a.size.cmp(&b.size),
                                        SortBy::Type => a.icon_name().cmp(&b.icon_name()),
                                    };
                                    if s.asc { ord } else { ord.reverse() }
                                });
                                v
                            }.iter() {
                                { let item_path = item.path.clone(); let item_path_click = item_path.clone(); let item_path_open = item_path.clone(); let path_str = item_path.display().to_string(); let mtime = item.modified.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "-".to_string()); let ctime = item.created.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "-".to_string()); let size = item.size.map(|s| format_size(s, DECIMAL)).unwrap_or_else(|| "-".to_string());
                                    rsx! { li { key: "{path_str}", onclick: move |_| { selected_path.set(Some(item_path_click.clone())); if *preview_collapsed.read() { preview_collapsed.set(false); let mut s = ui.write(); s.preview_collapsed = false; save_settings(&s); } }, ondoubleclick: move |_| { let _ = open::that(&item_path_open); },
                                        i { class: "material-icons text-22px file-icon", {item.icon_name()} }
                                        div { class: "meta", style: "display:grid; grid-template-columns: 2fr 1.2fr 1.2fr .8fr .8fr; gap:10px; align-items:center;",
                                            h3 { class: "name ellipsis", {Path::new(&path_str).file_name().and_then(|s| s.to_str()).unwrap_or(&path_str)} }
                                            span { class: "text-weak", "{mtime}" }
                                            span { class: "text-weak", "{ctime}" }
                                            span { class: "text-weak", "{size}" }
                                            span { class: "text-weak", {item.icon_name()} }
                                        }
                                    } }
                                }
                            }
                        }
                    }
                }
            }

            // Right preview pane
            aside { class: "preview bg-panel border-l border-stroke p-2", style: "{preview_style}",
                div { class: "flex items-center justify-between mb-2",
                    h3 { class: "text-18px font-semibold", "Preview" }
                    button { class: "btn", onclick: move |_| { let curr = *preview_collapsed.read(); preview_collapsed.set(!curr); let mut s = ui.write(); s.preview_collapsed = !curr; s.preview_width = *preview_width.read(); save_settings(&s); }, i { class: "material-icons", { if *preview_collapsed.read() { "chevron_left" } else { "chevron_right" } } } }
                }
                if !preview_collapsed.read().clone() {
                    { let sel = selected_path.read().clone(); let item_opt = sel.as_ref().and_then(|s| { results.read().items.iter().find(|it| &it.path == s).cloned() });
                        rsx! { if let Some(item) = item_opt {
                            div { class: "flex flex-col gap-2", style: "min-width:0;",
                                if let Some(img) = &item.thumb_data { img { class: "w-full h-48 object-contain rounded-md border border-stroke bg-muted", src: "{img}" } }
                                h4 { class: "text-18px font-semibold", { item.path.file_name().and_then(|s| s.to_str()).unwrap_or("") } }
                                p { class: "text-sm text-weak", style: "white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", { item.path.display().to_string() } }
                                p { class: "text-sm text-weak", { item.size.map(|s| format_size(s, DECIMAL)).unwrap_or_else(|| "-".into()) } }
                                div { class: "flex gap-2 mt-2",
                                    button { class: "btn", onclick: move |_| { let _ = open::that(&item.path); }, i { class: "material-icons", "open_in_new" } span { " Open" } }
                                }
                            }
                        } else { p { class: "text-sm text-weak", "No selection" } } }
                    }
                    // small drag handle to resize preview width
                    div { class: "resize-handle", style: "position:absolute; top:0; left:-3px; width:6px; height:100%; cursor: ew-resize;",
                        onmousedown: move |evt| { resizing_preview.set(Some((evt.client_coordinates().x as i32, *preview_width.read()))); }
                    }
                } else {
                    // Collapsed gutter clickable to expand
                    div { style: "position:absolute; inset:0; cursor:pointer;", onclick: move |_| { preview_collapsed.set(false); let mut s = ui.write(); s.preview_collapsed = false; save_settings(&s); } }
                }
            }
        }
        // Global mouse handlers for resizing (overlay only while dragging)
        if resizing_preview.read().is_some() || resizing_left.read().is_some() {
            div { style: "position:fixed; inset:0; z-index: 1000; cursor: ew-resize;",
                onmousemove: move |evt| {
                    if let Some((start_x, start_w)) = resizing_preview.read().clone() {
                        let dx = (evt.client_coordinates().x as i32) - start_x;
                        let new_w = (start_w as i32 - dx).clamp(240, 800) as u32;
                        preview_width.set(new_w);
                    }
                    if let Some((start_x, start_w)) = resizing_left.read().clone() {
                        let dx = (evt.client_coordinates().x as i32) - start_x;
                        let new_w = (start_w as i32 + dx).clamp(180, 480) as u32;
                        left_width.set(new_w);
                    }
                },
                onmouseup: move |_| {
                    if resizing_preview.read().is_some() { let mut s = ui.write(); s.preview_width = *preview_width.read(); save_settings(&s); }
                    resizing_preview.set(None);
                    if resizing_left.read().is_some() { let mut s = ui.write(); s.left_width = *left_width.read(); save_settings(&s); }
                    resizing_left.set(None);
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
