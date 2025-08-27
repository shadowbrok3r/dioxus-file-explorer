use dioxus::prelude::*;
use humansize::{format_size, DECIMAL};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::settings::{SortBy, SortSetting, UiSettings};
use crate::types::{FoundFile, ViewMode, IMAGE_EXTS, VIDEO_EXTS};

// New helper component: performs bulk thumbnail generation with stable hook order
#[derive(Props, PartialEq, Clone)]
struct BulkThumbLoaderProps {
    items: Vec<FoundFile>,
    all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>,
}

#[allow(non_snake_case)]
fn BulkThumbLoader(props: BulkThumbLoaderProps) -> Element {
    let mut generating = use_signal(|| HashSet::<String>::new());
    let items = props.items.clone();
    let all_cached_sig = props.all_cached.clone();

    use_effect(move || {
        for f in items.iter() {
            let path = f.path.clone();
            let path_str = path.display().to_string();

            // Skip if already have inline thumb or cached
            let already_cached = {
                let cache = all_cached_sig.read();
                cache.get(&path_str).and_then(|(_, t, _)| t.as_ref()).is_some()
            };
            if f.thumb_data.is_some() || already_cached {
                continue;
            }

            // Skip if already generating
            if generating.read().contains(&path_str) {
                continue;
            }

            // Check extension
            let ext_opt = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
            if let Some(ext) = ext_opt {
                let is_img = IMAGE_EXTS.iter().any(|e| *e == ext);
                let is_vid = VIDEO_EXTS.iter().any(|e| *e == ext);
                if !(is_img || is_vid) {
                    continue;
                }

                // Mark generating
                generating.write().insert(path_str.clone());

                let mut all_cached_clone = all_cached_sig.clone();
                let mut generating_clone = generating.clone();
                spawn(async move {
                    let thumb_res = if is_img {
                        crate::thumbs::generate_image_thumb_data(&path).ok()
                    } else {
                        #[cfg(windows)]
                        { crate::thumbs::generate_video_thumb_data(&path).ok() }
                        #[cfg(not(windows))]
                        { None }
                    };
                    if let Some(t) = thumb_res {
                        let mut cache_w = all_cached_clone.write();
                        let entry = cache_w.entry(path_str.clone()).or_insert((None, None, None));
                        entry.1 = Some(t);
                    }
                    generating_clone.write().remove(&path_str);
                });
            }
        }
    });

    rsx! { div {} }
}

#[derive(Props, PartialEq, Clone)]
pub struct ResultsProps {
    pub view_mode: Signal<ViewMode>,
    pub sort: Signal<SortSetting>,
    pub ui: Signal<UiSettings>,
    pub filtered_items: Vec<FoundFile>, // already filtered by ext/search/exclusions
    pub group_by_category: Signal<bool>,
    pub all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>, // path -> (hash, thumb, category)
    pub selected_path: Signal<Option<std::path::PathBuf>>,
    pub selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>,
    pub ai_descriptions: Signal<HashMap<String,String>>,
    pub grouped_items: Option<BTreeMap<String, Vec<FoundFile>>>,
    pub ai_search_active: Signal<bool>,
    pub ai_search_results: Signal<Vec<crate::ai::FileMetadata>>,
    pub clip_search_results: Signal<Vec<crate::ai::FileMetadata>>,
    pub detail_column_widths: Signal<[f32;6]>,
    pub resizing_col: Signal<Option<(usize,i32,f32)>>,
}

pub fn results_view(props: ResultsProps) -> Element {
    // Stable loader always first
    let items_for_loader = props.filtered_items.clone();
    let all_cached_for_loader = props.all_cached.clone();
    let loader_key = format!("bulk-thumbs-{}", items_for_loader.len());

    // Precompute content so rsx sibling order stays constant
    let content = if *props.view_mode.read() == ViewMode::Icons {
        render_icons(props)
    } else {
        render_details(props)
    };

    rsx! {
        BulkThumbLoader { key: "{loader_key}", items: items_for_loader, all_cached: all_cached_for_loader }
        {content}
    }
}

fn render_icons(props: ResultsProps) -> Element {
    let selected_path = props.selected_path;
    let multi_selected = props.selected_paths;
    let ai_desc = props.ai_descriptions;
    let all_cached = props.all_cached;
    let group = *props.group_by_category.read();
    let grouped_opt = props.grouped_items.clone();
    let ai_active = *props.ai_search_active.read();
    let ai_results = props.ai_search_results.read().clone();
    let clip_results = props.clip_search_results;

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
                                { icon_card(meta.path.clone(), meta.thumb_b64.clone().or(meta.thumbnail_path.clone()), meta.file_type.clone(), meta.description.clone(), meta.category.clone(), meta.similarity_score, selected_path, multi_selected, ai_desc, all_cached) }
                            }
                        }
                    }
                }
            }
            if !clip_results.read().is_empty() {
                div { class: "mb-4",
                    h3 { class: "text-12px font-semibold uppercase tracking-wide text-weak mb-2", "CLIP Results" }
                    div { class: "grid gap-3 grid-cols-[repeat(auto-fill,minmax(120px,1fr))]",
                        for meta in clip_results.read().iter() {
                            { icon_card(meta.path.clone(), meta.thumb_b64.clone().or(meta.thumbnail_path.clone()), meta.file_type.clone(), meta.description.clone(), meta.category.clone(), meta.clip_similarity_score.or(meta.similarity_score), selected_path, multi_selected, ai_desc, all_cached) }
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
                                        icon_card(p, thumb, ft.into(), None, None, None, selected_path, multi_selected, ai_desc, all_cached)
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
                            icon_card(p, thumb, ft.into(), None, None, None, selected_path, multi_selected, ai_desc, all_cached)
                        }
                    }
                }
            }
        }
    }
}

fn icon_card(path: String, thumb: Option<String>, file_type: String, desc: Option<String>, cat: Option<String>, similarity_val: Option<f32>, mut selected_path: Signal<Option<std::path::PathBuf>>, mut selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>, ai_desc: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>) -> Element {
    let selected = selected_path.read().as_ref().map(|p| p.display().to_string() == path).unwrap_or(false);
    let multi_selected_state = selected_paths.read().contains(&std::path::PathBuf::from(&path));
    let ai_desc_map = ai_desc.read();
    let desc_final = desc.or(ai_desc_map.get(&path).cloned());
    let cat_final = cat.or(all_cached.read().get(&path).and_then(|(_,_,c)| c.clone()));
    let similarity: Option<String> = similarity_val.map(|s| format!("{s:.3}"));
    let style = if multi_selected_state { "border-indigo-400 bg-indigo-500/15" } else if selected { "border-accent selected-item" } else { "border-stroke bg-panel" };
    rsx! {
        div { key: "icon-{path}", class: "p-2 rounded-lg border text-center flex flex-col gap-2 cursor-pointer transition hover:border-accent {style}",
            onclick: move |evt| {
                let pb = std::path::PathBuf::from(path.clone());
                // Dioxus desktop mouse event currently does not expose KeyModifiers type in this crate version; attempt ctrl/meta detection via cfg or fallback
                let ctrl = evt.modifiers().ctrl() || evt.modifiers().meta();
                if ctrl {
                    let mut set = selected_paths.write();
                    if set.contains(&pb) { set.remove(&pb); } else { set.insert(pb.clone()); }
                } else {
                    selected_path.set(Some(pb.clone()));
                    // Reset multi-selection to only this if previously multiple
                    let mut set = selected_paths.write();
                    set.clear();
                    set.insert(pb);
                }
            },
            div { class: "relative w-full aspect-square rounded-md overflow-hidden bg-muted flex items-center justify-center",
                if let Some(t) = thumb.clone() { img { class: "object-cover w-full h-full max-w-[128px] max-h-[128px]", style: "display:block;", src: "{t}" } }
                else if let Some((_, Some(cached_thumb), _)) = all_cached.read().get(&path) { img { class: "object-cover w-full h-full max-w-[128px] max-h-[128px]", style: "display:block;", src: "{cached_thumb}" } }
                else { div { class: "flex flex-col items-center justify-center text-weak gap-1",
                        i { class: "material-icons text-4xl opacity-60", "{file_type}" }
                        span { class: "text-8px animate-pulse", "loading" }
                    } }
                if let Some(sim) = similarity { span { class: "absolute top-1 right-1 bg-indigo-500/80 text-white text-8px px-1 rounded", "{sim}" } }
            }
            if let Some(c) = cat_final.as_ref() { 
                if !c.is_empty() { span { class: "text-10px px-2 py-0.5 rounded-full bg-muted border border-stroke truncate", "{c}" } }
            }
            if let Some(d) = desc_final.as_ref() {
                p { class: "text-10px leading-snug line-clamp-3", "{d}" }
            }
            span { class: "text-10px break-all text-weak", 
                { std::path::Path::new(&path).file_name().and_then(|f| f.to_str()).unwrap_or("") }
            }
        }
    }
}

fn render_details(props: ResultsProps) -> Element {
    let sort = props.sort;
    let _ui = props.ui;
    let selected_path = props.selected_path;
    let multi_selected = props.selected_paths;
    let ai_descriptions = props.ai_descriptions;
    let all_cached = props.all_cached;
    let ai_active = *props.ai_search_active.read();
    let ai_results = props.ai_search_results.read().clone();
    let clip_results = props.clip_search_results.read().clone();
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

    // Compute common root for relative paths
    let common_root: Option<std::path::PathBuf> = {
        if items.is_empty() {
            None
        } else {
            let mut comps: Vec<_> = items[0].path.components().collect();
            for it in items.iter().skip(1) {
                let mut keep = 0;
                for (a,b) in comps.iter().zip(it.path.components()) {
                    if a == &b { keep += 1 } else { break; }
                }
                comps.truncate(keep);
                if comps.is_empty() { break; }
            }
            if comps.is_empty() { None } else {
                let mut p = std::path::PathBuf::new();
                for c in comps { p.push(c.as_os_str()); }
                Some(p)
            }
        }
    };

    let sort_val = &sort.read().by;
    let (show_modified, show_created) = {
        let sm = matches!(sort_val, SortBy::Modified);
        let sc = matches!(sort_val, SortBy::Created);
        if !sm && !sc {
            // Fallback: show Modified column when sorting by other fields
            (true, false)
        } else {
            (sm, sc)
        }
    };

    // Determine if any item has a non-empty relative parent path -> decide to show Path column
    let show_path_col = {
        if items.is_empty() { false } else {
            if let Some(root) = &common_root {
                items.iter().any(|it| {
                    if it.path.starts_with(root) {
                        let rp = it.path.strip_prefix(root).unwrap();
                        rp.parent().map(|p| !p.as_os_str().is_empty()).unwrap_or(false)
                    } else {
                        true
                    }
                })
            } else {
                true
            }
        }
    };

    rsx! { div { class: "details space-y-6",
        if ai_active && !ai_results.is_empty() { div { class: "space-y-1",
            h3 { class: "text-11px font-semibold uppercase tracking-wide text-weak px-1", "AI Results ({ai_results.len()})" }
            for meta in ai_results.iter() {
                { ai_detail_row(meta.clone(), selected_path, ai_descriptions, all_cached) }
            }
        }}
        if !clip_results.is_empty() { div { class: "space-y-1",
            h3 { class: "text-11px font-semibold uppercase tracking-wide text-weak px-1", "CLIP Results ({clip_results.len()})" }
            for meta in clip_results.iter() {
                { clip_detail_row(meta.clone(), selected_path, ai_descriptions, all_cached) }
            }
        }}
        // existing original listing
        { details_header(sort, props.ui, props.detail_column_widths, props.resizing_col, &props.filtered_items, show_modified, show_created, show_path_col) }
        if group {
            if let Some(groups) = grouped_opt.as_ref() {
                for (cat, list) in groups.iter() {
                    div { key: "g-{cat}", class: "space-y-1",
                        h4 { class: "text-11px font-semibold uppercase tracking-wide text-weak px-1 mt-4", "{cat} ({list.len()})" }
                        for it in list.iter() { { detail_row(it.clone(), selected_path, multi_selected, ai_descriptions, all_cached, props.detail_column_widths, &common_root, show_modified, show_created, show_path_col) } }
                    }
                }
            }
        } else {
            for it in items.iter() { { detail_row(it.clone(), selected_path, multi_selected, ai_descriptions, all_cached, props.detail_column_widths, &common_root, show_modified, show_created, show_path_col) } }
        }
    }}
}

fn ai_detail_row(meta: crate::ai::FileMetadata, mut selected_path: Signal<Option<std::path::PathBuf>>, ai_descriptions: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>) -> Element {
    let path = meta.path.clone();
    let selected = selected_path.read().as_ref().map(|p| p.display().to_string() == path).unwrap_or(false);
    let style = if selected { "border-accent bg-accent-weak/40" } else { "border-stroke bg-panel" };
    let desc = meta.description.or(ai_descriptions.read().get(&path).cloned());
    let category = meta.category.or(all_cached.read().get(&path).and_then(|(_,_,c)| c.clone()));
    let tags = meta.tags.clone();
    let filename = std::path::Path::new(&path).file_name().and_then(|f| f.to_str()).unwrap_or("");
    let similarity_text = meta.similarity_score.map(|s| format!("{s:.3}"));
    rsx! { div { key: "ai-row-{path}", class: "p-2 rounded-md border flex flex-col gap-1 text-11px cursor-pointer {style}", onclick: move |_| { selected_path.set(Some(std::path::PathBuf::from(path.clone()))); },
        div { class: "flex items-center gap-2",
            span { class: "font-medium truncate", "{filename}" }
            if let Some(c) = category { span { class: "px-1 rounded bg-muted border border-stroke text-8px", "{c}" } }
            if let Some(score_txt) = similarity_text { span { class: "px-1 rounded bg-accent/20 text-accent text-8px", "{score_txt}" } }
        }
        if let Some(d) = desc { p { class: "line-clamp-2", "{d}" } }
        if !tags.is_empty() { div { class: "flex flex-wrap gap-1", for t in tags.iter().take(8) { span { class: "px-1 rounded bg-muted border border-stroke text-8px", "{t}" } } } }
    }}
}

fn clip_detail_row(meta: crate::ai::FileMetadata, mut selected_path: Signal<Option<std::path::PathBuf>>, ai_descriptions: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>) -> Element {
    let path = meta.path.clone();
    let selected = selected_path.read().as_ref().map(|p| p.display().to_string() == path).unwrap_or(false);
    let style = if selected { "border-indigo-400 bg-indigo-500/10" } else { "border-stroke bg-panel" };
    let desc = meta.description.or(ai_descriptions.read().get(&path).cloned());
    let category = meta.category.or(all_cached.read().get(&path).and_then(|(_,_,c)| c.clone()));
    let tags = meta.tags.clone();
    let score_val = meta.clip_similarity_score.or(meta.similarity_score);
    let filename = std::path::Path::new(&path).file_name().and_then(|f| f.to_str()).unwrap_or("");
    let score_text = score_val.map(|s| format!("{s:.3}"));
    rsx! { div { key: "clip-row-{path}", class: "p-2 rounded-md border flex flex-col gap-1 text-11px cursor-pointer {style}", onclick: move |_| { selected_path.set(Some(std::path::PathBuf::from(path.clone()))); },
        div { class: "flex items-center gap-2",
            span { class: "font-medium truncate", "{filename}" }
            if let Some(c) = category { span { class: "px-1 rounded bg-indigo-500/15 border border-indigo-400 text-indigo-300 text-8px", "{c}" } }
            if let Some(st) = score_text { span { class: "px-1 rounded bg-indigo-400/20 text-indigo-300 text-8px", "{st}" } }
        }
        if let Some(d) = desc { p { class: "line-clamp-2", "{d}" } }
        if !tags.is_empty() { div { class: "flex flex-wrap gap-1", for t in tags.iter().take(8) { span { class: "px-1 rounded bg-muted border border-stroke text-8px", "{t}" } } } }
    }}
}

// Header updated: add show_path_col flag; fuse Name+Path widths when Path hidden
fn details_header(sort: Signal<SortSetting>, ui: Signal<UiSettings>, mut widths: Signal<[f32;6]>, resizing: Signal<Option<(usize,i32,f32)>>, items: &Vec<crate::types::FoundFile>, show_modified: bool, show_created: bool, show_path_col: bool) -> Element {
    let w = widths.read();
    // Column order: Name(0)[+Path(1) if hidden], [Path(1)?], [Modified(2)?], [Created(3)?], Size(4), Type(5)
    let mut col_specs: Vec<(usize,f32)> = Vec::new();
    if show_path_col {
        col_specs.push((0, w[0]));          // Name
        col_specs.push((1, w[1]));          // Path
    } else {
        col_specs.push((0, w[0] + w[1]));   // Name widened
    }
    if show_modified { col_specs.push((2, w[2])); }
    if show_created  { col_specs.push((3, w[3])); }
    col_specs.push((4, w[4])); // Size
    col_specs.push((5, w[5])); // Type

    let template = {
        let mut s = "56px ".to_string();
        for (_, fr) in &col_specs { s.push_str(&format!("{fr}fr ")); }
        s
    };
    let items_ref = items.clone();
    rsx! { div { class: "results-header grid gap-0 px-0 py-0 items-stretch text-11px border-b border-stroke select-none",
        style: format!("display:grid;grid-template-columns:{};align-items:stretch;width:100%;", template),
        onmousemove: move |evt| {
            if let Some((col_idx, start_x, start_w)) = resizing.read().clone() {
                let dx = evt.client_coordinates().x as i32 - start_x;
                let mut wcopy = widths.read().clone();
                let new_w = (start_w + (dx as f32 * 0.15)).clamp(0.25, 18.0);
                if col_idx < wcopy.len() { wcopy[col_idx] = new_w; widths.set(wcopy); }
            }
        },
        span { "" }
        for (width_idx, _) in col_specs.iter() {
            match *width_idx {
                0 => { resizable_head(sortable_col("Name", SortBy::Name, sort, ui), *width_idx, widths, resizing, items_ref.clone()) }
                1 => if show_path_col { resizable_head(plain_col("Path"), *width_idx, widths, resizing, items_ref.clone()) } else { rsx! { span { } }}
                2 => if show_modified { resizable_head(sortable_col("Modified", SortBy::Modified, sort, ui), *width_idx, widths, resizing, items_ref.clone()) } else { rsx! { span { } }}
                3 => if show_created  { resizable_head(sortable_col("Created", SortBy::Created, sort, ui), *width_idx, widths, resizing, items_ref.clone()) } else { rsx! { span { } }}
                4 => { resizable_head(sortable_col("Size", SortBy::Size, sort, ui), *width_idx, widths, resizing, items_ref.clone()) }
                5 => { resizable_head(sortable_col("Type", SortBy::Type, sort, ui), *width_idx, widths, resizing, items_ref.clone()) }
                _ => { rsx! { span { } }}
            }
        }
    }}
}



// detail_row updated: show_path_col flag; if hidden, omit path cell & widen template first column
fn detail_row(
    item: FoundFile,
    mut selected_path: Signal<Option<std::path::PathBuf>>,
    mut selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>,
    ai_descriptions: Signal<HashMap<String,String>>,
    all_cached: Signal<HashMap<String,(Option<String>,Option<String>,Option<String>)>>,
    widths: Signal<[f32;6]>,
    common_root: &Option<std::path::PathBuf>,
    show_modified: bool,
    show_created: bool,
    show_path_col: bool
) -> Element {
    let abs_path_str = item.path.display().to_string();
    // Relative parent path (empty if directly under root)
    let rel_path = if let Some(root) = common_root {
        if item.path.starts_with(root) {
            let rp = item.path.strip_prefix(root).unwrap();
            if let Some(parent) = rp.parent() {
                if parent.as_os_str().is_empty() { "".to_string() } else { parent.display().to_string() }
            } else { "".to_string() }
        } else { abs_path_str.clone() }
    } else { abs_path_str.clone() };
    let display_rel = if rel_path.is_empty() { ".".to_string() } else { rel_path.clone() };
    let name = item.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
    let size_txt = item.size.map(|s| format_size(s, DECIMAL)).unwrap_or("-".into());
    let modified_txt = item.modified.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or("-".into());
    let created_txt  = item.created.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or("-".into());
    let ext_txt = item.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let selected = selected_path.read().as_ref().map(|p| p == &item.path).unwrap_or(false);
    let multi_selected_state = selected_paths.read().contains(&item.path);
    let row_style = if multi_selected_state { "selected-item" } else if selected { "selected-item border-accent" } else { "non-selected-item border-stroke" };
    let desc_opt = ai_descriptions.read().get(&abs_path_str).cloned();
    let cat_opt = all_cached.read().get(&abs_path_str).and_then(|(_,_,c)| c.clone());

    let w = widths.read();
    let mut col_order: Vec<usize> = Vec::new();
    if show_path_col {
        col_order.push(0); // Name
        col_order.push(1); // Path
    } else {
        col_order.push(0); // Name fused
    }
    if show_modified { col_order.push(2); }
    if show_created  { col_order.push(3); }
    col_order.push(4);
    col_order.push(5);

    let mut template = "56px ".to_string();
    for idx in &col_order {
        let fr = if !show_path_col && *idx == 0 { w[0] + w[1] } else { w[*idx] };
        template.push_str(&format!("{fr}fr "));
    }

    rsx! { div { key: "det-{abs_path_str}", class: "detail-row grid items-center gap-2 rounded-md border px-2 h-[50px] cursor-pointer text-11px {row_style}",
        style: format!("display:grid;grid-template-columns:{};width:100%;height:50px", template),
        onclick: move |evt| {
            let ctrl = evt.modifiers().ctrl() || evt.modifiers().meta();
            let shift = evt.modifiers().shift();
            if ctrl {
                let mut set = selected_paths.write();
                if set.contains(&item.path) { set.remove(&item.path); } else { set.insert(item.path.clone()); }
            } else if shift {

            } else {
                selected_path.set(Some(item.path.clone()));
                let mut set = selected_paths.write();
                set.clear();
                set.insert(item.path.clone());
            }
        },
        // Thumb
        div { class: "thumb flex items-center justify-center rounded bg-muted",
            if let Some(img) = item.thumb_data.clone() { img { src: "{img}", class: "object-cover w-full h-full max-w-[48px] max-h-[48px] ", style: "display:block;" } }
            else if let Some((_, Some(cached), _)) = all_cached.read().get(&abs_path_str) { img { src: "{cached}", class: "object-cover w-full h-full max-w-[48px] max-h-[48px]", style: "display:block;" } }
            else { div { class: "flex flex-col items-center justify-center text-weak gap-0.5 w-full h-full",
                    i { class: "material-icons text-base", "{item.icon_name()}" }
                    span { class: "text-[9px] animate-pulse", "loading" }
                } }
        }
        // Name
        div { class: "truncate font-semibold", title: "{name}", "{name}" }
        // Path (only if enabled)
        if show_path_col {
            div { class: "truncate text-weak", title: "{abs_path_str}", "{display_rel}" }
        }
        if show_modified { span { "{modified_txt}" } }
        if show_created  { span { "{created_txt}" } }
        span { "{size_txt}" }
        div { class: "flex items-center gap-1 truncate",
            span { "{ext_txt}" }
            if let Some(cat) = cat_opt { span { class: "px-1 rounded bg-muted border border-stroke text-8px", "{cat}" } }
            if let Some(desc) = desc_opt { span { class: "px-1 rounded bg-accent-weak text-8px truncate", title: "{desc}", "AI" } }
        }
    }}

}

// Modified details_header still calls plain_col -> re-add helper (was removed)
fn plain_col(label: &str) -> Element {
    rsx! { span { class: "truncate", "{label}" } }
}

// sortable column header with current sort indicator & click behavior
fn sortable_col(label: &str, col: SortBy, mut sort: Signal<SortSetting>, _ui: Signal<UiSettings>) -> Element {
    let current = sort.read().clone();
    let active = current.by == col;
    let asc = current.asc;
    rsx! { span { class: "truncate flex items-center gap-1 cursor-pointer select-none",
        onclick: move |_| {
            let mut s = sort.read().clone();
            if s.by == col { s.asc = !s.asc; } else { s.by = col; s.asc = true; }
            sort.set(s.clone());
            if let Some(mut ui_sig) = dioxus::prelude::try_consume_context::<Signal<UiSettings>>() {
                let mut settings = ui_sig.write();
                settings.sort = Some(s.clone());
                crate::settings::save_settings(&settings);
            }
        },
        span { "{label}" }
        if active { i { class: "material-icons text-[13px] opacity-70", { if asc { "arrow_upward" } else { "arrow_downward" } } } }
    } }
}

// resizable_head now receives width index directly (unchanged logic, just clarified name)
fn resizable_head(content: Element, width_idx: usize, mut widths: Signal<[f32;6]>, mut resizing: Signal<Option<(usize,i32,f32)>>, items: Vec<crate::types::FoundFile>) -> Element {
    let active = resizing.read().clone().map(|(i,_,_)| i == width_idx).unwrap_or(false);
    rsx! { div { class: "relative flex items-center px-2 py-1 gap-1 border-r border-stroke last:border-r-0 transition-colors",
        class: if active { "bg-accent/10" } else { "bg-panel" },
        style: "min-height:28px;",
        {content}
        div { class: "absolute top-0 right-0 h-full group select-none",
            style: "width:8px;touch-action:none;cursor:col-resize;user-select:none;z-index:10;right:0;top:0;",
            onmousedown: move |evt| {
                let start_x = evt.client_coordinates().x as i32;
                let start_w = widths.read()[width_idx];
                resizing.set(Some((width_idx, start_x, start_w)));
            },
            ondoubleclick: move |_| {
                let mut wcopy = widths.read().clone();
                let target = if items.is_empty() { 1.0 } else {
                    match width_idx {
                        0 => { // Name
                            let max_len = items.iter().take(500).filter_map(|f| f.path.file_name().and_then(|n| n.to_str())).map(|s| s.len()).max().unwrap_or(8);
                            (max_len as f32 / 18.0).clamp(0.4, 6.0)
                        }
                        1 => { // Path
                            let max_len = items.iter().take(300).map(|f| f.path.parent().map(|p| p.display().to_string().len()).unwrap_or(1)).max().unwrap_or(12);
                            (max_len as f32 / 30.0).clamp(0.6, 6.0)
                        }
                        2 => 0.9,
                        3 => 0.9,
                        4 => 0.7,
                        5 => 0.6,
                        _ => 1.0
                    }
                };
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
            div { class: "absolute top-0 left-1/2 -translate-x-1/2 h-full w-px", style: "background:rgba(180,180,200,0.15);" }
        }
    } }
}
