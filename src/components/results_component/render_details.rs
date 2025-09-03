//! Details (list) view rendering: full headers, sorting, row thumbnails.
use dioxus::prelude::*;
use std::collections::{HashMap, BTreeMap};
use humansize::{format_size, DECIMAL};
use crate::utilities::types::FileRecord;
use crate::settings::SortBy;
use crate::components::results_component::headers; // reuse dropdown helpers
use super::ResultsProps;

pub fn render_details(
    props: ResultsProps,
    _collapsed: Signal<HashMap<String, bool>>,
    enriched: Vec<FileRecord>,
    _grouped: Option<BTreeMap<String, Vec<FileRecord>>>,
) -> Element {
    let sort_sig = props.sort.clone();
    let mut items = enriched.clone();
    let sort_val = sort_sig.read().clone();
    items.sort_by(|a,b| {
        let ord = match sort_val.by {
            SortBy::Name => a.path.file_name().cmp(&b.path.file_name()),
            SortBy::Size => a.size.cmp(&b.size),
            SortBy::Modified => a.modified.cmp(&b.modified),
            SortBy::Created => a.created.cmp(&b.created),
            SortBy::Type => a.path.extension().cmp(&b.path.extension()),
            SortBy::Category => a.category.cmp(&b.category),
        };
        if sort_val.asc { ord } else { ord.reverse() }
    });

    // Simple flex layout fallback (avoids grid stacking issues). We'll ignore persisted widths for now.
    let grouped = *props.group_by_category.read();
    let show_category = !grouped;

    let sort_button = |label: &'static str, col: SortBy| -> Element {
        let mut sig = sort_sig.clone();
        let active = sig.read().by == col;
        let asc = sig.read().asc;
        let icon = if !active { "unfold_more" } else if asc { "arrow_drop_up" } else { "arrow_drop_down" };
        rsx! {
            button { 
                class: "flex items-center gap-0.5 px-1 py-0.5 rounded",
                "data-style": "glass", 
                onclick: move |_| {
                    let mut s = sig.write();
                    if s.by == col { s.asc = !s.asc; } else { s.by = col; s.asc = true; }
                },
                span { "{label}" }
                i { class: "material-icons text-[15px] opacity-70", "{icon}" }
            }
        }
    };
    // Filters (extension/type & category & modified) – derive counts
    let filters_sig = props.filters.clone();
    let filters_val = filters_sig.read().clone();
    // Derive extension sets (collect from current items once per render)
    let mut all_exts: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for r in &items { if let Some(ext) = r.path.extension().and_then(|e| e.to_str()) { all_exts.insert(ext.to_ascii_lowercase()); } }
    let ext_filters_sig = use_signal(|| all_exts.clone());
    let ext_enabled_sig = use_signal(|| {
        let mut map = std::collections::BTreeMap::new();
        for e in all_exts.iter() { map.insert(e.clone(), true); }
        map
    });
    let disabled_ext_count = ext_enabled_sig.read().values().filter(|v| !**v).count();
    let category_count = filters_val.category_filters.len();
    let modified_active = filters_val.modified_after.is_some() || filters_val.modified_before.is_some();

    // Build header row with independent flex cells so they align with row cells
    let header = rsx! {
        div { class: "results-header sticky top-0 z-[10] backdrop-blur border-b border-stroke select-none px-2 py-1",
            div { class: "flex items-center text-xs text-weak gap-2",
                // spacer for icon
                div { class: "w-10 shrink-0" }
                // Name + type filter
                div { class: "flex-1 min-w-[160px] flex items-center gap-1", { sort_button("Name", SortBy::Name) }
                    { headers::type_filter_dropdown(ext_filters_sig.clone(), ext_enabled_sig.clone(), disabled_ext_count) }
                }
                if show_category { div { class: "w-28 shrink-0 flex items-center gap-1", "Category"
                    { headers::category_filter_dropdown(filters_sig.clone(), filters_val.category_filters.clone(), category_count) }
                } }
                div { class: "w-56 shrink-0", "Path" }
                div { class: "w-28 shrink-0 flex items-center gap-1", { sort_button("Modified", SortBy::Modified) }
                    { headers::modified_filter_dropdown(filters_sig.clone(), modified_active) }
                }
                div { class: "w-28 shrink-0", { sort_button("Created", SortBy::Created) } }
                div { class: "w-20 shrink-0 text-right", { sort_button("Size", SortBy::Size) } }
                div { class: "w-16 shrink-0", { sort_button("Type", SortBy::Type) } }
            }
        }
    };

    // Rows: need mutable signal handles for .set
    let mut selected_path = props.selected_path;
    let mut selected_paths = props.selected_paths;
    let mut rows: Vec<Element> = Vec::new();
    for rec in items.iter() {
        let key = rec.path.display().to_string();
        let fname = rec.path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        let parent = rec.path.parent().and_then(|p| p.to_str()).unwrap_or("");
        let size_txt = rec.size.map(|s| format_size(s, DECIMAL)).unwrap_or("-".into());
        let modified_txt = rec.modified.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or("-".into());
        let created_txt = rec.created.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or("-".into());
        let ext_txt = rec.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let is_sel = selected_path.read().as_ref().map(|p| p == &rec.path).unwrap_or(false) || selected_paths.read().contains(&rec.path);
        let mut cls = "file-row grid items-center gap-2 px-2 py-1 rounded cursor-pointer select-none text-xs".to_string();
        if is_sel { cls.push_str(" is-selected"); }
    // Flex columns
    let rec_click = rec.path.clone();
    let rec_key = rec.path.clone();
        let thumb_opt = rec.thumb_data.clone();
        rows.push(rsx! {
            div { key: "row-{key}", class: "{cls} flex gap-2",
                onclick: move |_| {
                    selected_path.set(Some(rec_click.clone()));
                    let mut set = selected_paths.read().clone();
                    set.clear();
                    set.insert(rec_click.clone());
                    selected_paths.set(set);
                },
                onkeydown: move |e| {
                    let k = e.key().to_string();
                    if k == "Enter" || k == " " || k == "Space" || k == "Spacebar" {
                        e.prevent_default();
                        selected_path.set(Some(rec_key.clone()));
                        let mut set = selected_paths.read().clone();
                        set.clear();
                        set.insert(rec_key.clone());
                        selected_paths.set(set);
                    }
                },
                div { class: "w-10 h-10 flex items-center justify-center rounded-md overflow-hidden bg-surface file-card-thumb shrink-0",
                    if let Some(t) = thumb_opt.as_ref() { img { class: "object-cover w-full h-full select-none pointer-events-none", style: "max-width:40px;max-height:40px;", src: "{t}", alt: "thumb" } }
                    if thumb_opt.is_none() { i { class: "material-icons text-[24px] opacity-60", "{rec.kind.icon_name()}" } }
                }
                div { class: "flex-1 min-w-[160px] truncate font-medium", title: "{fname}", "{fname}" }
                if show_category { div { class: "w-28 shrink-0 text-weak truncate", {rec.category.clone().unwrap_or_else(|| "-".into())} } }
                div { class: "w-56 shrink-0 text-weak truncate", title: "{parent}", "{parent}" }
                div { class: "w-28 shrink-0 text-weak", "{modified_txt}" }
                div { class: "w-28 shrink-0 text-weak", "{created_txt}" }
                div { class: "w-20 shrink-0 text-right pr-2 tabular-nums", "{size_txt}" }
                div { class: "w-16 shrink-0 uppercase text-weak", "{ext_txt}" }
            }
        });
    }

    rsx! {
        div { class: "flex flex-col w-full h-full overflow-hidden details",
            {header}
            div { class: "flex-1 overflow-auto space-y-0.5 py-1", { rows.into_iter() } }
        }
    }
}
