use dioxus::prelude::{rsx, *};
use crate::settings::{SortBy, SortSetting, UiSettings};
use crate::utilities::types::FoundFile;
// bring in dropdown primitives
use dioxus_primitives::dropdown_menu::{
    DropdownMenu, DropdownMenuTrigger, DropdownMenuContent,
};
// date helper utilities
use super::utilities::{set_date_range, clear_dates};

// Simplified: we don't currently store extension signals separately; derive from Files each render.
pub fn type_filter_dropdown(ext_filters: Signal<std::collections::BTreeSet<String>>, mut ext_enabled: Signal<std::collections::BTreeMap<String,bool>>, disabled_ct: usize) -> Element {
    let type_nodes = rsx! {
        for ename in ext_filters.read().iter().cloned() {
            {
                let active = *ext_enabled.read().get(&ename).unwrap_or(&true);
                let state_cls = if active { "filter-btn-active" } else { "filter-btn-inactive" };
                rsx! {
                    button {
                        key: "{ename}",
                        "data-style": "outline",
                        class: "button text-10px flex justify-between {state_cls}",
                        onclick: move |_| {
                            let mut map = ext_enabled.write();
                            let cur = map.get(&ename).cloned().unwrap_or(true);
                            map.insert(ename.clone(), !cur);
                        },
                        span { ".{ename}" }
                        span { class: "material-icons", {if active { "check" } else { "close" }} }
                    }
                }
            }
        }
    };
    rsx! {
        DropdownMenu { class: "inline-block",
            DropdownMenuTrigger {
                "data-style": "glass",
                class: "button px-1 py-0 h-6 text-[10px] leading-none flex items-center gap-0.5 relative filter-trigger",
                i { class: "material-icons text-[16px] opacity-80", "filter_list" }
                if disabled_ct > 0 {
                    span { class: "filter-badge", "{disabled_ct}" }
                }
            }
            DropdownMenuContent { class: "menubar-content flex flex-col gap-2 min-w-[180px] filter-dropdown-list",
                if ext_filters.read().is_empty() {
                    span { class: "text-10px text-weak", "No types" }
                }
                if !ext_filters.read().is_empty() {
                    div { class: "filter-dropdown-list", {type_nodes} }
                    button {
                        class: "filter-reset-link",
                        onclick: move |_| {
                            let exts: Vec<String> = ext_filters.read().iter().cloned().collect();
                            let mut map = ext_enabled.write();
                            for e in exts {
                                map.insert(e, true);
                            }
                        },
                        "Enable All"
                    }
                }
            }
        }
    }
}

// sortable column header with compact glass button style
pub fn sortable_col(label: &str, col: SortBy, mut sort: Signal<SortSetting>, _ui: Signal<UiSettings>) -> Element {
    let state = sort.read().clone();
    let active = state.by == col;
    let asc = state.asc;
    let icon = if !active { "unfold_more" } else if asc { "arrow_drop_up" } else { "arrow_drop_down" };
    rsx! {
        button {
            "data-style": "glass",
            class: "button px-1 py-0 h-6 text-[10px] leading-none flex items-center gap-0.5 font-medium tracking-wide",
            title: if active { if asc { "Ascending" } else { "Descending" } } else { "Sort" },
            onclick: move |_| {
                let mut s = sort.read().clone();
                if s.by == col {
                    s.asc = !s.asc;
                } else {
                    s.by = col;
                    s.asc = true;
                }
                sort.set(s.clone());
                if let Some(mut ui_sig) = dioxus::prelude::try_consume_context::<
                    Signal<UiSettings>,
                >() {
                    let mut settings = ui_sig.write();
                    settings.sort = Some(s.clone());
                    crate::settings::save_settings(&settings);
                }
            },
            span { class: if active { "text-accent" } else { "opacity-80" }, "{label}" }
            i { class: "material-icons text-[14px] opacity-70", "{icon}" }
        }
    }
}

// resizable_head now receives width index directly (unchanged logic, just clarified name)
pub fn resizable_head(content: Element, width_idx: usize, mut widths: Signal<[f32;6]>, mut resizing: Signal<Option<(usize,i32,f32)>>, items: Vec<FoundFile>) -> Element {
    let active = resizing.read().map(|(i,_,_)| i == width_idx).unwrap_or(false);
    rsx! {
        div { class: "results-header-col relative flex items-center group",
            {content}
            // Resize handle (visual + pointer area)
            div {
                class: "col-resize-handle absolute top-0 right-0 h-full select-none flex items-center justify-center",
                style: "width:8px;touch-action:none;cursor:col-resize;user-select:none;z-index:15;",
                onmousedown: move |evt| {
                    let start_x = evt.client_coordinates().x as i32;
                    let start_w = widths.read()[width_idx];
                    resizing.set(Some((width_idx, start_x, start_w)));
                },
                onpointerup: move |_| {
                    if resizing.read().is_some() { resizing.set(None); }
                },
                ondoubleclick: move |_| {
                    let mut wcopy = widths.read().clone();
                    let target = if items.is_empty() { 1.0 } else { match width_idx {
                        0 => {
                            let max_len = items.iter().take(500)
                                .filter_map(|f| f.path.file_name().and_then(|n| n.to_str()))
                                .map(|s| s.len()).max().unwrap_or(8);
                            (max_len as f32 / 18.0).clamp(0.4, 6.0)
                        }
                        1 => {
                            let max_len = items.iter().take(300)
                                .map(|f| f.path.parent().map(|p| p.display().to_string().len()).unwrap_or(1))
                                .max().unwrap_or(12);
                            (max_len as f32 / 30.0).clamp(0.6, 6.0)
                        }
                        2 => 0.9,
                        3 => 0.9,
                        4 => 0.7,
                        5 => 0.6,
                        _ => 1.0,
                    }};
                    wcopy[width_idx] = target;
                    let sum: f32 = wcopy.iter().sum();
                    if sum > 0.0 {
                        let desired = 7.2_f32;
                        let scale = (desired / sum).clamp(0.7, 1.3);
                        for wv in &mut wcopy { *wv = (*wv * scale).clamp(0.35, 6.0); }
                    }
                    widths.set(wcopy);
                    if let Some(mut ui_sig) = dioxus::prelude::try_consume_context::<Signal<UiSettings>>() {
                        let mut settings = ui_sig.write();
                        settings.detail_column_widths = Some(widths.read().clone());
                        crate::settings::save_settings(&settings);
                    }
                },
                // Visual center line
                div { class: "resize-bar w-px h-full" }
                if active { div { class: "resize-overlay" } }
            }
        }
    }
}

pub fn header_with_filter(main: Element, dropdown: Option<Element>) -> Element {
    if dropdown.is_none() { return main; }
    let dd = dropdown.unwrap();
    // New layout: simple flex row filling the header cell. Label left, filter trigger flush right.
    // Padding-right keeps distance from resize handle (8px) while avoiding overlap.
    rsx! {
        div { class: "flex items-center w-full gap-1 pr-1", // tighter right padding
            div { class: "flex items-center gap-1 min-w-0 overflow-hidden", // min-w-0 allows truncation
                {main}
            }
            div { class: "ml-auto flex items-center shrink-0", // ensure trigger stays at far right
                {dd}
            }
        }
    }.into()
}

pub fn category_filter_dropdown(mut filters: Signal<crate::utilities::types::Filters>, categories: std::collections::BTreeSet<String>, active_ct: usize) -> Element {
    rsx! {
        DropdownMenu { class: "inline-block",
            DropdownMenuTrigger {
                "data-style": "glass",
                class: "button px-1 py-0 h-6 text-[10px] leading-none flex items-center gap-0.5 relative filter-trigger ml-auto",
                i { class: "material-icons text-[16px] opacity-80", "filter_list" }
                if active_ct > 0 {
                    span { class: "filter-badge", "{active_ct}" }
                }
            }
            DropdownMenuContent { class: "menubar-content flex flex-col gap-2 min-w-[200px] filter-dropdown-list",
                if categories.is_empty() {
                    span { class: "text-10px text-weak", "No categories" }
                }
                if !categories.is_empty() {
                    div { class: "filter-dropdown-list",
                        for cat in categories.iter() {
                            {
                                let cname = cat.clone();
                                let active = filters.read().category_filters.contains(&cname);
                                let state_cls = if active { "filter-btn-active" } else { "filter-btn-inactive" };
                                rsx! {
                                    button {
                                        key: "cat-{cname}",
                                        "data-style": "outline",
                                        class: "button text-10px {state_cls}",
                                        onclick: move |_| {
                                            let mut f = filters.write();
                                            if f.category_filters.contains(&cname) {
                                                f.category_filters.remove(&cname);
                                            } else {
                                                f.category_filters.insert(cname.clone());
                                            }
                                        },
                                        "{cname}"
                                    }
                                }
                            }
                        }
                    }
                    if !filters.read().category_filters.is_empty() {
                        button {
                            class: "filter-reset-link",
                            onclick: move |_| {
                                filters.write().category_filters.clear();
                            },
                            "Clear"
                        }
                    }
                }
            }
        }
    }
}

pub fn modified_filter_dropdown(mut filters: Signal<crate::utilities::types::Filters>, active: bool) -> Element {
    rsx! {
        DropdownMenu { class: "inline-block",
            DropdownMenuTrigger {
                "data-style": "glass",
                class: "button px-1 py-0 h-6 text-[10px] leading-none flex items-center gap-0.5 relative filter-trigger",
                i { class: "material-icons text-[16px] opacity-80", "filter_list" }
                if active {
                    span { class: "filter-badge", "1" }
                }
            }
            DropdownMenuContent { class: "menubar-content flex flex-col gap-1 min-w-[160px] filter-dropdown-list",
                button {
                    "data-style": "outline",
                    class: "button text-left text-10px",
                    onclick: move |_| {
                        set_date_range(&mut filters, 1);
                    },
                    "Last 24h"
                }
                button {
                    "data-style": "outline",
                    class: "button text-left text-10px",
                    onclick: move |_| {
                        set_date_range(&mut filters, 7);
                    },
                    "Last 7 days"
                }
                button {
                    "data-style": "outline",
                    class: "button text-left text-10px",
                    onclick: move |_| {
                        set_date_range(&mut filters, 30);
                    },
                    "Last 30 days"
                }
                button {
                    "data-style": "outline",
                    class: "button text-left text-10px",
                    onclick: move |_| {
                        clear_dates(&mut filters);
                    },
                    "Clear"
                }
            }
        }
    }
}
