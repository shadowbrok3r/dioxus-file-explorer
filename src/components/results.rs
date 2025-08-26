use dioxus::prelude::*;
use humansize::{format_size, DECIMAL};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::settings::{SortBy, SortSetting, UiSettings, save_settings};
use crate::types::{FoundFile, ViewMode};

#[derive(Props, PartialEq, Clone)]
pub struct ResultsProps {
    pub view_mode: Signal<ViewMode>,
    pub sort: Signal<SortSetting>,
    pub ui: Signal<UiSettings>,
    pub filtered_items: Vec<FoundFile>, // already filtered by ext/search/exclusions
    pub group_by_category: Signal<bool>,
    pub all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>, // path -> (hash, thumb, category)
    pub selected_path: Signal<Option<PathBuf>>,
    pub ai_descriptions: Signal<HashMap<String,String>>,
    pub grouped_items: Option<BTreeMap<String, Vec<FoundFile>>>,
    pub ai_search_active: Signal<bool>,
    pub ai_search_results: Signal<Vec<crate::ai::FileMetadata>>,
    pub detail_column_widths: Signal<[f32;6]>,
    pub resizing_col: Signal<Option<(usize,i32,f32)>>,
}

pub fn results_view(props: ResultsProps) -> Element {
    // Decide which content to render
    if *props.view_mode.read() == ViewMode::Icons {
        render_icons(props)
    } else {
        render_details(props)
    }
}

fn render_icons(props: ResultsProps) -> Element {
    let selected_path = props.selected_path;
    let ai_desc = props.ai_descriptions;
    let all_cached = props.all_cached;
    let group = *props.group_by_category.read();
    let grouped_opt = props.grouped_items.clone();
    let ai_active = *props.ai_search_active.read();
    let ai_results = props.ai_search_results.read().clone();

    rsx! {
        div { class: "space-y-6",
            if ai_active {
                div { class: "mb-4",
                    h3 { class: "text-12px font-semibold uppercase tracking-wide text-weak mb-2", "AI Results" }
                    if ai_results.is_empty() {
                        p { class: "text-weak text-11px", "No AI matches yet." }
                    } else {
                        div { class: "grid gap-3 grid-cols-[repeat(auto-fill,minmax(120px,1fr))]",
                            for meta in ai_results.iter() { 
                                { icon_card(meta.path.clone(), meta.thumb_b64.clone().or(meta.thumbnail_path.clone()), meta.file_type.clone(), meta.description.clone(), meta.category.clone(), selected_path, ai_desc, all_cached) }
                            }
                        }
                    }
                }
            }
            if group {
                if let Some(groups) = grouped_opt.as_ref() {
                    for (cat, items) in groups.iter() {
                        div { key: "cat-{cat}", class: "space-y-2",
                            h4 { class: "text-12px font-semibold uppercase tracking-wide text-weak px-1", "{cat} ({items.len()})" }
                            div { class: "grid gap-3 grid-cols-[repeat(auto-fill,minmax(120px,1fr))]",
                                for it in items.iter() { 
                                    {
                                        let p = it.path.display().to_string();
                                        let ft = it.icon_name();
                                        let thumb = it.thumb_data.clone();
                                        icon_card(p, thumb, ft.into(), None, None, selected_path, ai_desc, all_cached)
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "grid gap-3 grid-cols-[repeat(auto-fill,minmax(120px,1fr))]",
                    for it in props.filtered_items.iter() { 
                        {
                            let p = it.path.display().to_string();
                            let ft = it.icon_name();
                            let thumb = it.thumb_data.clone();
                            icon_card(p, thumb, ft.into(), None, None, selected_path, ai_desc, all_cached)
                        }
                    }
                }
            }
        }
    }
}

fn icon_card(path: String, thumb: Option<String>, file_type: String, desc: Option<String>, cat: Option<String>, mut selected_path: Signal<Option<PathBuf>>, ai_desc: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>) -> Element {
    let selected = selected_path.read().as_ref().map(|p| p.display().to_string() == path).unwrap_or(false);
    let ai_desc_map = ai_desc.read();
    let desc_final = desc.or(ai_desc_map.get(&path).cloned());
    let cat_final = cat.or(all_cached.read().get(&path).and_then(|(_,_,c)| c.clone()));
    let style = if selected { "border-accent bg-accent-weak/40" } else { "border-stroke bg-panel" };
    rsx! {
        div { key: "icon-{path}", class: "p-2 rounded-lg border text-center flex flex-col gap-2 cursor-pointer transition hover:border-accent {style}",
            onclick: move |_| { selected_path.set(Some(PathBuf::from(path.clone()))); },
            div { class: "w-full aspect-square rounded-md overflow-hidden bg-muted flex items-center justify-center", 
                if let Some(t) = thumb.clone() { img { class: "object-cover w-full h-full max-w-[128px] max-h-[128px]", style: "display:block;", src: "{t}" } }
                else if let Some((_, Some(cached_thumb), _)) = all_cached.read().get(&path) { img { class: "object-cover w-full h-full max-w-[128px] max-h-[128px]", style: "display:block;", src: "{cached_thumb}" } }
                else { div { class: "flex flex-col items-center justify-center text-weak gap-1",
                        i { class: "material-icons text-4xl opacity-60", "{file_type}" }
                        span { class: "text-8px animate-pulse", "loading" }
                    } }
            }
            if let Some(c) = cat_final.as_ref() { 
                if !c.is_empty() { span { class: "text-10px px-2 py-0.5 rounded-full bg-muted border border-stroke truncate", "{c}" } }
            }
            if let Some(d) = desc_final.as_ref() {
                p { class: "text-10px leading-snug line-clamp-3", "{d}" }
            }
                    // Show filename
                    span { class: "text-10px break-all text-weak", 
                        { std::path::Path::new(&path).file_name().and_then(|f| f.to_str()).unwrap_or("") }
                    }
        }
    }
}

fn render_details(props: ResultsProps) -> Element {
    let sort = props.sort;
    let ui = props.ui;
    let selected_path = props.selected_path;
    let ai_descriptions = props.ai_descriptions;
    let all_cached = props.all_cached;
    let group = *props.group_by_category.read();
    let grouped_opt = props.grouped_items.clone();

    // Re-sort locally (filtered_items already filtered). This mirrors previous logic.
    let mut items = props.filtered_items.clone();
    let sv = sort.read();
    items.sort_by(|a,b| {
        let ord = match sv.by {
            SortBy::Name => a.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase()
                .cmp(&b.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase()),
            SortBy::Modified => a.modified.cmp(&b.modified),
            SortBy::Created => a.created.cmp(&b.created),
            SortBy::Size => a.size.cmp(&b.size),
            SortBy::Type => a.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
                .cmp(&b.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()),
        }; if sv.asc { ord } else { ord.reverse() }
    });

    rsx! { div { class: "details space-y-6",
    { details_header(sort, ui, props.detail_column_widths, props.resizing_col, &props.filtered_items) }
        if group {
            if let Some(groups) = grouped_opt.as_ref() {
                for (cat, list) in groups.iter() { 
                    div { key: "g-{cat}", class: "space-y-1",
                        h4 { class: "text-11px font-semibold uppercase tracking-wide text-weak px-1 mt-4", "{cat} ({list.len()})" }
                        for it in list.iter() { { detail_row(it.clone(), selected_path, ai_descriptions, all_cached, props.detail_column_widths) } }
                    }
                }
            }
        } else {
            for it in items.iter() { { detail_row(it.clone(), selected_path, ai_descriptions, all_cached, props.detail_column_widths) } }
        }
    }}
}

fn details_header(sort: Signal<SortSetting>, ui: Signal<UiSettings>, mut widths: Signal<[f32;6]>, resizing: Signal<Option<(usize,i32,f32)>>, items: &Vec<crate::types::FoundFile>) -> Element {
    let w = widths.read();
    let template = format!("56px {}fr {}fr {}fr {}fr {}fr {}fr", w[0], w[1], w[2], w[3], w[4], w[5]);
    let items_ref = items.clone(); // cheap clone of Vec
    rsx! { div { class: "results-header grid gap-0 px-0 py-0 items-stretch text-11px border-b border-stroke select-none",
        style: format!("display:grid;grid-template-columns:{};align-items:stretch;width:100%;", template),
        onmousemove: move |evt| {
            if let Some((col_idx, start_x, start_w)) = resizing.read().clone() {
                let dx = evt.client_coordinates().x as i32 - start_x;
                let mut wcopy = widths.read().clone();
                let new_w = (start_w + (dx as f32 * 0.15)).clamp(0.25, 18.0);
                if col_idx < wcopy.len() { wcopy[col_idx] = new_w; widths.set(wcopy); }
                log::debug!("[resize-header] col={col_idx} dx={dx} -> {new_w:.2}");
            }
        },
        span { "" }
        { resizable_head(sortable_col("Name", SortBy::Name, sort, ui), 0, widths, resizing, items_ref.clone()) }
        { resizable_head(plain_col("Path"), 1, widths, resizing, items_ref.clone()) }
        { resizable_head(sortable_col("Size", SortBy::Size, sort, ui), 2, widths, resizing, items_ref.clone()) }
        { resizable_head(sortable_col("Modified", SortBy::Modified, sort, ui), 3, widths, resizing, items_ref.clone()) }
        { resizable_head(sortable_col("Created", SortBy::Created, sort, ui), 4, widths, resizing, items_ref.clone()) }
        { resizable_head(sortable_col("Type", SortBy::Type, sort, ui), 5, widths, resizing, items_ref.clone()) }
    }}
}

fn sortable_col(label: &str, by: SortBy, mut sort: Signal<SortSetting>, mut ui: Signal<UiSettings>) -> Element {
    let arrow = { let sv = sort.read(); if sv.by == by { if sv.asc { "▲ " } else { "▼ " } } else { "" } };
    rsx! { span { class: "cursor-pointer select-none", onclick: move |_| {
        let mut s_sig = sort.write();
        let mut new = s_sig.clone();
        if new.by == by { new.asc = !new.asc; } else { new.by = by.clone(); new.asc = true; }
        *s_sig = new.clone();
        let mut uiw = ui.write(); uiw.sort = Some(new); save_settings(&uiw);
    }, "{arrow}{label}" } }
}

fn detail_row(item: FoundFile, mut selected_path: Signal<Option<PathBuf>>, ai_descriptions: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>, widths: Signal<[f32;6]>) -> Element {
    let path_disp = item.path.display().to_string();
    let name = item.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
    let size_txt = item.size.map(|s| format_size(s, DECIMAL)).unwrap_or("-".into());
    let modified_txt = item.modified.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or("-".into());
    let created_txt = item.created.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or("-".into());
    let ext_txt = item.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let selected = selected_path.read().as_ref().map(|p| p == &item.path).unwrap_or(false);
    let row_style = if selected { "bg-accent-weak border-accent" } else { "bg-panel border-stroke" };
    let desc_opt = ai_descriptions.read().get(&path_disp).cloned();
    let cat_opt = all_cached.read().get(&path_disp).and_then(|(_,_,c)| c.clone());
    // Use the same width template as header for alignment
    let w = widths.read();
    let template = format!("56px {}fr {}fr {}fr {}fr {}fr {}fr", w[0], w[1], w[2], w[3], w[4], w[5]);
    rsx! { div { key: "det-{path_disp}", class: "detail-row grid gap-0 rounded-md border px-2 py-1 cursor-pointer text-11px {row_style}",
        style: format!("display:grid;grid-template-columns:{};width:100%;", template),
        onclick: move |_| { selected_path.set(Some(item.path.clone())); },
        div { class: "w-12 h-12 flex items-center justify-center rounded bg-muted overflow-hidden",
            if let Some(img) = item.thumb_data.clone() { img { src: "{img}", class: "object-cover w-full h-full max-w-[48px] max-h-[48px]", style: "display:block;" } }
            else if let Some((_, Some(cached_thumb), _)) = all_cached.read().get(&path_disp) { img { src: "{cached_thumb}", class: "object-cover w-full h-full max-w-[48px] max-h-[48px]", style: "display:block;" } }
            else { div { class: "flex flex-col items-center justify-center text-weak gap-0.5 w-full h-full",
                    i { class: "material-icons text-base opacity-60", "{item.icon_name()}" }
                    span { class: "text-[9px] animate-pulse", "loading" }
                } }
        }
        div { class: "truncate font-semibold", title: "{name}", "{name}" }
        div { class: "truncate text-weak", title: "{path_disp}", "{path_disp}" }
        span { "{size_txt}" }
        span { "{modified_txt}" }
        span { "{created_txt}" }
        div { class: "flex items-center gap-1 truncate", span { "{ext_txt}" } 
            if let Some(cat) = cat_opt { span { class: "px-1 rounded bg-muted border border-stroke text-8px", "{cat}" } }
            if let Some(desc) = desc_opt { span { class: "px-1 rounded bg-accent-weak text-8px truncate", title: "{desc}", "AI" } }
        }
    }}
}

fn plain_col(label: &str) -> Element {
    rsx! { span { class: "truncate", "{label}" } }
}

fn resizable_head(content: Element, idx: usize, mut widths: Signal<[f32;6]>, mut resizing: Signal<Option<(usize,i32,f32)>>, items: Vec<crate::types::FoundFile>) -> Element {
    let active = resizing.read().clone().map(|(i,_,_)| i == idx).unwrap_or(false);
    rsx! { div { class: "relative flex items-center px-2 py-1 gap-1 border-r border-stroke last:border-r-0 transition-colors",
        class: if active { "bg-accent/10" } else { "bg-panel" },
        style: "min-height:28px;",
        {content}
        // Resize handle
        // Resize handle: explicit inline width so it works even if Tailwind arbitrary values aren't processed in .rs files
        div { class: "absolute top-0 right-0 h-full group select-none",
            style: "width:8px;touch-action:none;cursor:col-resize;user-select:none;z-index:10;position:absolute;right:0;top:0;",
            "data-col": "{idx}",
            title: "Drag to resize. Double-click to auto-fit",
            onmousedown: move |evt| {
                let start_x = evt.client_coordinates().x as i32;
                let start_w = widths.read()[idx];
                log::info!("col-resize mousedown idx={idx} start_w={start_w}");
                resizing.set(Some((idx, start_x, start_w)));
            },
            ondoubleclick: move |_| {
                // Auto-fit: estimate width from longest visible text for this column.
                let mut wcopy = widths.read().clone();
                let target = if items.is_empty() { 1.0 } else {
                    match idx {
                        0 => { // Name
                            let max_len = items.iter().take(500).filter_map(|f| f.path.file_name().and_then(|n| n.to_str())).map(|s| s.len()).max().unwrap_or(8);
                            (max_len as f32 / 18.0).clamp(0.4, 6.0)
                        }
                        1 => { // Path
                            let max_len = items.iter().take(300).map(|f| f.path.display().to_string().len()).max().unwrap_or(12);
                            (max_len as f32 / 30.0).clamp(0.6, 6.0)
                        }
                        2 => 0.7,
                        3 => 0.9,
                        4 => 0.9,
                        5 => 0.6,
                        _ => 1.0
                    }
                };
                wcopy[idx] = target;
                // Normalize lightly (do not drastically shrink others)
                let sum: f32 = wcopy.iter().sum();
                if sum > 0.0 {
                    let desired = 7.2_f32;
                    let scale = (desired / sum).clamp(0.7, 1.3);
                    for w in &mut wcopy { *w = (*w * scale).clamp(0.35, 6.0); }
                }
                widths.set(wcopy);
                // Persist immediately on auto-fit for better UX (no need to wait for mouseup)
                if let Some(mut ui_sig) = dioxus::prelude::try_consume_context::<Signal<UiSettings>>() {
                    let mut settings = ui_sig.write();
                    settings.detail_column_widths = Some(widths.read().clone());
                    crate::settings::save_settings(&settings);
                }
            },
            // Visual indicator line always visible at low opacity; brighter on hover/active
            div { class: "absolute top-0 left-1/2 -translate-x-1/2 h-full w-px", style: "background:rgba(180,180,200,0.15);" }
            div { class: "absolute top-0 left-0 h-full w-full", style: "background:transparent;" }
        }
    } }
}
