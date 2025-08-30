use dioxus::prelude::*;
use dioxus_primitives::switch::{Switch, SwitchThumb};
use crate::scan::begin_scan;

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
    let scan_generation = props.scan_generation;
    let scanning = props.scanning;
    let results = props.results;
    let dir_items = props.dir_items;
    let progress = props.progress;
    let mut recursive_current = props.recursive_current;
    let mut only_subdirs = props.only_subdirs;
    let mut scan_started = props.scan_started;
    let mut scan_finished = props.scan_finished;
    let ext_filters = props.ext_filters;
    let mut ext_enabled = props.ext_enabled;
    let mut excluded_dirs = props.excluded_dirs;
    let _ui = props.ui; // currently only used indirectly via save_settings for persistence elsewhere if needed

    // Note: Date range pickers (modified_before / modified_after) were migrated into the Menubar > Filters
    // dropdown with Calendar components inside navbar.rs. This bar now only holds quick media toggles
    // and extension / exclusion filters.
    rsx! { section { class: "filters", style: "position: sticky; top: 48px; ",
        div { class: "filter-group",
            div { class: "flex items-center px-4",
                div { class: "flex items-center gap-2 text-10px",
                    span { class: "text-8px text-weak", "Img" }
                    Switch { class: "switch", checked: filters.read().include_images,
                        on_checked_change: move |v: bool| {
                            let root = { let mut flt = filters.write(); flt.include_images = v; flt.root.clone() };
                            scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None);
                            let rec = *recursive_current.read();
                            if crate::app::shallow_should_scan(&root) || rec { only_subdirs.set(false); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, rec); }
                        },
                        SwitchThumb { class: "switch-thumb" }
                    }
                }
                div { class: "filter-group flex items-center gap-2 text-10px",
                    span { class: "text-8px text-weak", "Vid" }
                    Switch { class: "switch", checked: filters.read().include_videos,
                        on_checked_change: move |v: bool| {
                            let root = { let mut flt = filters.write(); flt.include_videos = v; flt.root.clone() };
                            scan_started.set(Some(std::time::Instant::now())); scan_finished.set(None);
                            let rec = *recursive_current.read();
                            if crate::app::shallow_should_scan(&root) || rec { only_subdirs.set(false); begin_scan(filters, scan_generation, scanning, results, dir_items, progress, rec); }
                        },
                        SwitchThumb { class: "switch-thumb" }
                    }
                }
            }
            div { class: "filter-group",
                div { class: "flex items-center gap-1 text-10px",
                    span { class: "text-8px text-weak", "Thumbs" }
                    Switch { class: "switch", checked: filters.read().only_with_thumb,
                        on_checked_change: move |v: bool| { filters.write().only_with_thumb = v; },
                        SwitchThumb { class: "switch-thumb" }
                    }
                }
            }
        }
    // Date filters moved into Navbar menubar -> Filters menu with calendar popups
        if !ext_filters.read().is_empty() {
            div { class: "filter-group", style: "display:flex; flex-wrap:wrap; gap:6px; align-items:center;",
                label { class: "font-semibold", "Ext:" }
                for ext in ext_filters.read().iter() {
                    { let ext_name = ext.clone(); rsx! {
                        { let active = *ext_enabled.read().get(&ext_name).unwrap_or(&true); let style_str = if active { "user-select:none; " } else { "user-select:none; opacity:.45;" }; rsx! {
                            div { key: "ext-{ext_name}", class: "flex items-center gap-1 text-11px px-1.5 py-0.5 rounded-md border cursor-pointer", style: "{style_str}",
                                span { ".{ext_name}" }
                                Switch { class: "switch", checked: active, on_checked_change: move |v: bool| { let mut map = ext_enabled.write(); map.insert(ext_name.clone(), v); }, SwitchThumb { class: "switch-thumb" } }
                            }
                        } }
                    } }
                }
            }
        }
        if !excluded_dirs.read().is_empty() {
            div { class: "filter-group flex items-center gap-2",
                span { class: "text-11px", "Excluded: {excluded_dirs.read().len()} dirs" }
                button { class: "button", "data-style": "outline", onclick: move |_| { excluded_dirs.write().clear(); }, span { class: "text-10px", "Clear" } }
            }
        }
    }}
}
