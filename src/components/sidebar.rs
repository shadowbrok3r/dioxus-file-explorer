use dioxus::prelude::*;
use humansize::{format_size, DECIMAL};
use std::path::PathBuf;

#[derive(Props, PartialEq, Clone)]
pub struct LeftSidebarProps {
    pub filters: Signal<crate::types::Filters>,
    pub qa_collapsed: Signal<bool>,
    pub drives_collapsed: Signal<bool>,
    pub ui: Signal<crate::settings::UiSettings>,
    pub left_width: Signal<u32>,
    pub resizing_left: Signal<Option<(i32,u32)>>,
    pub path_text: Signal<String>,
    pub recursive_current: Signal<bool>,
    pub only_subdirs: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_generation: Signal<u64>,
    pub scanning: Signal<bool>,
    pub results: Signal<crate::types::ScanResults>,
    pub dir_items: Signal<Vec<crate::types::DirItem>>,
    pub progress: Signal<Option<(usize,usize)>>,
}

#[allow(non_snake_case)]
pub fn LeftSidebar(props: LeftSidebarProps) -> Element {
    let mut filters = props.filters;
    let mut qa_collapsed = props.qa_collapsed;
    let mut drives_collapsed = props.drives_collapsed;
    let mut ui = props.ui;
    let left_width = props.left_width; // width style handled by parent (passed via style prop)
    let mut resizing_left = props.resizing_left;
    let mut path_text = props.path_text;
    let mut recursive_current = props.recursive_current;
    let mut only_subdirs = props.only_subdirs;
    let mut scan_started = props.scan_started;
    let scan_generation = props.scan_generation;
    let scanning = props.scanning;
    let mut results = props.results;
    let mut dir_items = props.dir_items;
    let progress = props.progress;

    let computed_width = if *qa_collapsed.read() && *drives_collapsed.read() { 14 } else { (*left_width.read()).max(180).min(480) };

    rsx! { aside { class: "bg-panel border border-stroke p-2", style: "width: {computed_width}px; overflow:auto; transition: width .08s ease; position:relative;border-radius:16px",
        // Quick Access header
        div { class: "flex items-center justify-between px-3 py-2 border-b border-stroke ",
            h3 { class: "text-18px font-semibold", "Quick Access" }
            button { class: "btn", onclick: move |_| {
                let curr = *qa_collapsed.read(); qa_collapsed.set(!curr);
                let mut s = ui.write(); s.qa_collapsed = !curr; crate::settings::save_settings(&s);
            }, i { class: "material-icons", { if *qa_collapsed.read() { "chevron_right" } else { "expand_more" } } } }
        }
        if !*qa_collapsed.read() {
            ul { class: "qa-list", style: "padding:8px 10px; display:grid; gap:6px;",
                for qa in crate::explorer::quick_access().into_iter() { 
                    { let label = qa.label.clone(); let path = qa.path.clone(); rsx! {
                        li { key: "{label}", class: "btn", onclick: move |_| {
                            let new_root = path.clone(); { let mut f = filters.write(); f.root = new_root.clone(); }
                            path_text.set(new_root.display().to_string()); recursive_current.set(false);
                            if crate::app::shallow_should_scan(&new_root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); crate::scan::begin_scan(filters, scan_generation, scanning, results, dir_items, progress, false); }
                            else { only_subdirs.set(true); dir_items.set(crate::explorer::list_dir_items(new_root).unwrap_or_default()); results.set(Default::default()); }
                        }, i { class: "material-icons", match label.as_str() {
                            "Pictures" => "image",
                            "Videos" => "video_call",
                            "Desktop" => "desktop_windows",
                            "Documents" => "article",
                            "Downloads" => "download",
                            "Home" => "home",
                            _ => "star",
                        } } span { "{label}" } }
                    }}
                }
            }
        }
        // Drives header
        div { class: "flex items-center justify-between px-3 py-2 border-t border-b border-stroke",
            h3 { class: "text-18px font-semibold", "Drives" }
            button { class: "btn", onclick: move |_| {
                let curr = *drives_collapsed.read(); drives_collapsed.set(!curr); let mut s = ui.write(); s.drives_collapsed = !curr; crate::settings::save_settings(&s);
            }, i { class: "material-icons", { if *drives_collapsed.read() { "chevron_right" } else { "expand_more" } } } }
        }
        if !*drives_collapsed.read() {
            ul { class: "qa-list", style: "padding:8px 10px; display:grid; gap:6px;",
                for info in crate::explorer::list_drive_infos().into_iter() { { let root = info.root.clone(); let display = if info.label.is_empty() { info.root.clone() } else { format!("{} ({})", info.root, info.label) }; let path = PathBuf::from(root.clone()); let free = format_size(info.free, DECIMAL); let total = format_size(info.total, DECIMAL); rsx! {
                    li { key: "{display}", class: "btn", style: "display:grid; grid-template-columns:24px 1fr; align-items:center; gap:6px; padding:6px 8px;",
                        onclick: move |_| {
                            let new_root = path.clone(); { let mut f = filters.write(); f.root = new_root.clone(); }
                            path_text.set(new_root.display().to_string()); recursive_current.set(false);
                            if crate::app::shallow_should_scan(&new_root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); crate::scan::begin_scan(filters, scan_generation, scanning, results, dir_items, progress, false); }
                            else { only_subdirs.set(true); dir_items.set(crate::explorer::list_dir_items(new_root).unwrap_or_default()); results.set(Default::default()); }
                        },
                        i { class: "material-icons", style: "font-size:20px;", { crate::explorer::drive_icon_for_root(&root) } }
                        div { class: "flex flex-col min-w-0", span { title: "{display}", style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:13px;", "{display}" } span { class: "text-weak text-8px", style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{free} free of {total}" } }
                    }
                }} }
            }
        }
        if *qa_collapsed.read() && *drives_collapsed.read() {
            div { style: "position:absolute; inset:0; z-index:10; cursor:pointer;", onclick: move |_| {
                qa_collapsed.set(false); drives_collapsed.set(false); let mut s = ui.write(); s.qa_collapsed=false; s.drives_collapsed=false; crate::settings::save_settings(&s);
            } }
            div { style: "position:absolute; top:8px; left:2px; width:20px; height:20px; z-index:11;", button { class: "btn", title: "Expand side panel", onclick: move |_| { qa_collapsed.set(false); let mut s = ui.write(); s.qa_collapsed=false; crate::settings::save_settings(&s); }, i { class: "material-icons", "chevron_right" } } }
        }
        // Resize handle
        div { class: "resize-handle", style: "position:absolute; top:0; right:-3px; width:5px; height:100%; cursor: ew-resize;",
            onmousedown: move |evt| { resizing_left.set(Some((evt.client_coordinates().x as i32, *left_width.read()))); }
        }
    }}
}
