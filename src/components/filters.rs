use dioxus::prelude::*;
use crate::types::DateField;
use crate::scan::begin_scan;
use crate::settings::save_settings;

#[derive(Props, PartialEq, Clone)]
pub struct FiltersBarProps {
    pub filters: Signal<crate::types::Filters>,
    pub scan_generation: Signal<u64>,
    pub scanning: Signal<bool>,
    pub results: Signal<crate::types::ScanResults>,
    pub dir_items: Signal<Vec<crate::types::DirItem>>,
    pub progress: Signal<Option<(usize,usize)>>,
    pub recursive_current: Signal<bool>,
    pub only_subdirs: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_finished: Signal<Option<std::time::Instant>>,
    pub ext_filters: Signal<std::collections::BTreeSet<String>>,
    pub ext_enabled: Signal<std::collections::BTreeMap<String,bool>>,
    pub excluded_dirs: Signal<std::collections::BTreeSet<std::path::PathBuf>>,
    pub ui: Signal<crate::settings::UiSettings>,
}

#[allow(non_snake_case)]
pub fn FiltersBar(props: FiltersBarProps) -> Element {
    let mut filters = props.filters;
    let mut scan_generation = props.scan_generation;
    let mut scanning = props.scanning;
    let mut results = props.results;
    let mut dir_items = props.dir_items;
    let progress = props.progress;
    let mut recursive_current = props.recursive_current;
    let mut only_subdirs = props.only_subdirs;
    let mut scan_started = props.scan_started;
    let mut scan_finished = props.scan_finished;
    let mut ext_filters = props.ext_filters;
    let mut ext_enabled = props.ext_enabled;
    let mut excluded_dirs = props.excluded_dirs;
    let ui = props.ui; // currently only used indirectly via save_settings for persistence elsewhere if needed

    rsx! { section { class: "filters", style: "position: sticky; top: 56px; z-index: 5;",
        div { class: "filter-group",
            label { class: "chk",
                input { r#type: "checkbox", checked: filters.read().include_images, oninput: move |_| {
                    let root = { let mut flt = filters.write(); flt.include_images = !flt.include_images; flt.root.clone() };
                    scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None);
                    let rec = *recursive_current.read();
                    if crate::app::shallow_should_scan(&root) || rec { only_subdirs.set(false); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, rec); }
                } }
                span { " Images" }
            }
            label { class: "chk",
                input { r#type: "checkbox", checked: filters.read().include_videos, oninput: move |_| {
                    let root = { let mut flt = filters.write(); flt.include_videos = !flt.include_videos; flt.root.clone() };
                    scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None);
                    let rec = *recursive_current.read();
                    if crate::app::shallow_should_scan(&root) || rec { only_subdirs.set(false); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, rec); }
                } }
                span { " Videos" }
            }
            div { class: "filter-group",
                label { class: "chk",
                    input { r#type: "checkbox", checked: filters.read().only_with_thumb, oninput: move |_| {
                        let current_val = filters.read().only_with_thumb; filters.write().only_with_thumb = !current_val;
                    }}
                    span { " Thumbs only" }
                }
            }
        }
        div { class: "filter-group",
            button { class: "btn", onclick: move |_| {
                { let mut f = filters.write(); f.date_field = match f.date_field { DateField::Modified => DateField::Created, DateField::Created => DateField::Modified }; }
                recursive_current.set(false);
                if crate::app::shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); }
            }, span { "Date: " } strong { match filters.read().date_field { DateField::Modified => "Modified", DateField::Created => "Created" } } }
            label { " After:" }
            input { r#type: "date", value: filters.read().modified_after.clone().unwrap_or_default(), oninput: move |evt| { { let mut f = filters.write(); f.modified_after = Some(evt.value()); } recursive_current.set(false); if crate::app::shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); } } }
            label { " Before:" }
            input { r#type: "date", value: filters.read().modified_before.clone().unwrap_or_default(), oninput: move |evt| { { let mut f = filters.write(); f.modified_before = Some(evt.value()); } recursive_current.set(false); if crate::app::shallow_should_scan(&filters.read().root) { only_subdirs.set(false); scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, false); } else { only_subdirs.set(true); } } }
        }
        if !ext_filters.read().is_empty() {
            div { class: "filter-group", style: "display:flex; flex-wrap:wrap; gap:6px; align-items:center; max-width:760px;",
                label { class: "font-semibold", "Ext:" }
                for ext in ext_filters.read().iter() {
                    { let ext_name = ext.clone(); rsx! {
                        { let active = *ext_enabled.read().get(&ext_name).unwrap_or(&true); let style_str = if active { "user-select:none; background:var(--accent-weak); border-color:var(--accent);" } else { "user-select:none; opacity:.45;" }; rsx! {
                            label { key: "ext-{ext_name}", class: "flex items-center gap-1 text-11px px-1.5 py-0.5 rounded-md border cursor-pointer", style: "{style_str}",
                                input { r#type: "checkbox", checked: active, oninput: move |_| {
                                    let mut map = ext_enabled.write(); let cur = map.get(&ext_name).cloned().unwrap_or(true); map.insert(ext_name.clone(), !cur);
                                } }
                                span { ".{ext_name}" }
                            }
                        } }
                    } }
                }
            }
        }
        if !excluded_dirs.read().is_empty() {
            div { class: "filter-group flex items-center gap-2",
                span { class: "text-11px", "Excluded: {excluded_dirs.read().len()} dirs" }
                button { class: "btn text-10px px-2 py-0.5", onclick: move |_| { excluded_dirs.write().clear(); }, "Clear" }
            }
        }
    }}
}
