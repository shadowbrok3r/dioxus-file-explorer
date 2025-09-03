use dioxus::prelude::*;
use std::collections::HashMap;
use crate::settings::{SortSetting, UiSettings};
use crate::utilities::types::{FoundFile, ViewMode};

mod thumbnails;
mod render_icons;
mod utilities;
mod render_details;
mod headers;

// (export removed) Previously: pub use render_icons::*; Not needed outside module now.

#[derive(Props, PartialEq, Clone)]
pub struct ResultsProps {
    pub view_mode: Signal<ViewMode>,
    pub sort: Signal<SortSetting>,
    pub ui: Signal<UiSettings>,
    pub group_by_category: Signal<bool>,
    pub all_cached: Signal<HashMap<String, crate::Thumbnail>>, // path -> cached thumbnail row
    pub selected_path: Signal<Option<std::path::PathBuf>>,
    pub selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>,
    pub ai_descriptions: Signal<HashMap<String,String>>,
    pub ai_search_active: Signal<bool>,
    pub ai_search_results: Signal<Vec<crate::FileMetadata>>,
    pub detail_column_widths: Signal<[f32;6]>,
    pub category_col_width: Signal<f32>,
    pub resizing_col: Signal<Option<(usize,i32,f32)>>,
    pub filters: Signal<crate::utilities::types::Filters>,
}

#[component]
pub fn ResultsView(props: ResultsProps) -> Element {
    // Use new hook to derive filtered/grouped records
    let filtered = crate::components::hooks::use_filtered_records(props.group_by_category.clone());
    let enriched_records = filtered.records.read().clone();
    log::warn!("[results] render start filtered_items={} grouped={} view_mode={:?}", enriched_records.len(), *props.group_by_category.read(), *props.view_mode.read());
    let items_for_loader: Vec<FoundFile> = enriched_records.iter().map(|r| FoundFile { path: r.path.clone(), modified: r.modified, created: r.created, size: r.size, kind: r.kind.clone(), thumb_data: r.thumb_data.clone() }).collect();
    let all_cached_for_loader = props.all_cached.clone();
    // Single persistent generating set to avoid recreating signal per render (prevents scope warnings)
    // generating_set removed (worker handles de-dup)
    // Collapsed categories state must be declared unconditionally to satisfy hooks ordering
    let collapsed_cats = use_signal(|| HashMap::<String, bool>::new());

    // Provide contexts needed by rows (category width & grouped flag) BEFORE rendering descendants
    // so that any hooks inside descendants do not shift ordering relative to these provides.
    provide_context(props.category_col_width.clone());
    provide_context(props.group_by_category.clone());

    // Choose view based on setting
    let content = if *props.view_mode.read() == ViewMode::Details {
        crate::components::results_component::render_details::render_details(
            props.clone(),
            collapsed_cats.clone(),
            enriched_records.clone(),
            filtered.grouped.read().clone(),
        )
    } else {
        crate::components::results_component::render_icons::render_icons(
            props.clone(),
            collapsed_cats.clone(),
            enriched_records.clone(),
            filtered.grouped.read().clone(),
        )
    };
    log::warn!("[results] render complete filtered_items={} ui_nodes_ready", items_for_loader.len());
    // Keyboard navigation support: Up/Down arrows move primary selection within the currently
    // visible (filtered) flat record list when not grouped or across grouped order when grouped.
    // Shift extends range (adds all intermediate), Ctrl/Meta toggles without clearing.
    let mut selected_path_sig = props.selected_path.clone();
    let mut selected_paths_sig = props.selected_paths.clone();
    let flat_paths: Vec<std::path::PathBuf> = enriched_records.iter().map(|r| r.path.clone()).collect();
    let on_key = move |evt: KeyboardEvent| {
        let key = evt.key();
        let key_str = key.to_string();
        if key_str.as_str() != "ArrowDown" && key_str.as_str() != "ArrowUp" { return; }
        if flat_paths.is_empty() { return; }
        // Determine current index (prefer singular selected_path else first of selected_paths)
        let current_idx_opt = selected_path_sig.read().as_ref().and_then(|p| {
            flat_paths.iter().position(|fp| fp == p)
        }).or_else(|| {
            // fallback: any key from multi-set
            selected_paths_sig.read().iter().find_map(|p| flat_paths.iter().position(|fp| fp == p))
        });
        let dir = if key_str == "ArrowDown" { 1isize } else { -1isize };
        let next_idx = match current_idx_opt {
            Some(i) => ((i as isize) + dir).clamp(0, (flat_paths.len() - 1) as isize) as usize,
            None => if key_str == "ArrowDown" { 0 } else { flat_paths.len() - 1 }
        };
        if let Some(cur) = current_idx_opt { if cur == next_idx { return; } }
        let next_path = flat_paths[next_idx].clone();
        let shift = evt.modifiers().shift();
        let ctrl = evt.modifiers().ctrl() || evt.modifiers().meta();
        if shift {
            // Range select from anchor (current) to next
            let anchor_idx = current_idx_opt.unwrap_or(next_idx);
            let (start, end) = if anchor_idx <= next_idx { (anchor_idx, next_idx) } else { (next_idx, anchor_idx) };
            let mut set = selected_paths_sig.read().clone();
            for p in flat_paths[start..=end].iter() { set.insert(p.clone()); }
            selected_paths_sig.set(set);
            selected_path_sig.set(Some(next_path));
        } else if ctrl {
            // Toggle only next without clearing primary anchor
            let mut set = selected_paths_sig.read().clone();
            if set.contains(&next_path) { set.remove(&next_path); } else { set.insert(next_path.clone()); }
            selected_paths_sig.set(set);
            selected_path_sig.set(Some(next_path));
        } else {
            // Clear and select single
            selected_path_sig.set(Some(next_path.clone()));
            let mut set = selected_paths_sig.read().clone();
            set.clear();
            set.insert(next_path.clone());
            selected_paths_sig.set(set);
        }
        // Attempt to scroll the newly selected element into view by id (if we can assign ids to rows/cards)
        // For now rely on browser auto-scrolling due to focus; prevent default page scroll when we handled.
        evt.prevent_default();
    };

    rsx! {
        // Wrapper div captures keyboard events; tabindex enables focus.
        div { class: "results-keyboard-wrapper outline-none", tabindex: 0, onkeydown: on_key,
            crate::components::results_component::thumbnails::BulkThumbLoader { items: items_for_loader, all_cached: all_cached_for_loader }
            {content}
        }
    }
}

