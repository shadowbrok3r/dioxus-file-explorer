use dioxus::prelude::*;
use std::path::PathBuf;
use crossbeam::channel::{unbounded, Sender, Receiver};
use once_cell::sync::Lazy;
use dioxus_primitives::context_menu::{
    ContextMenu, ContextMenuTrigger, ContextMenuContent, ContextMenuItem
};
use humansize::{format_size, DECIMAL};
// rayon left imported elsewhere for other code, but BulkThumbLoader no longer uses parallel iter
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::settings::{SortBy, SortSetting, UiSettings};
use chrono::Utc; // for db_created timestamp when saving selection rows
use crate::utilities::types::{FoundFile, ViewMode, IMAGE_EXTS, VIDEO_EXTS};
use chrono; // needed for date presets
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuTrigger, DropdownMenuContent};

// ----------------------------------------------------------------------------------
// Global thumbnail worker (single background thread) to avoid heavy work in hooks
// ----------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct ThumbTask { path: PathBuf, is_image: bool, is_video: bool }

static THUMB_CHANNELS: Lazy<(Sender<ThumbTask>, Receiver<ThumbTask>, Sender<(PathBuf,String)>, Receiver<(PathBuf,String)>)> = Lazy::new(|| {
    let (tx, rx) = unbounded::<ThumbTask>();
    let (rtx, rrx) = unbounded::<(PathBuf,String)>();
    // Spawn worker thread (single thread sequential – avoids contention & UI blocking)
    std::thread::spawn({
        let rx = rx.clone();
        let rtx = rtx.clone();
        move || {
            log::warn!("[thumb-worker] started");
            while let Ok(task) = rx.recv() { // blocking (off UI thread)
                let ext_info = if task.is_image { "image" } else if task.is_video { "video" } else { "other" };
                // Generate thumbnail
                let thumb_res = if task.is_image {
                    crate::utilities::thumbs::generate_image_thumb_data(&task.path)
                } else if task.is_video {
                    #[cfg(windows)]
                    { crate::utilities::thumbs::generate_video_thumb_data(&task.path) }
                    #[cfg(not(windows))]
                    { Err("video unsupported".into()) }
                } else { Err("unsupported".into()) };
                match thumb_res {
                    Ok(data) => {
                        let _ = rtx.send((task.path.clone(), data));
                    }
                    Err(e) => {
                        log::warn!("[thumb-worker] failed {ext_info} {}: {e}", task.path.display());
                    }
                }
            }
        }
    });
    (tx, rx, rtx, rrx)
});

fn enqueue_thumb(path: &PathBuf, is_image: bool, is_video: bool) {
    let (tx, _, _, _) = &*THUMB_CHANNELS;
    // Best-effort enqueue (ignore send error if channel closed)
    let _ = tx.send(ThumbTask { path: path.clone(), is_image, is_video });
}

// New helper component: enqueues thumbnail tasks & polls global result channel
#[derive(Props, PartialEq, Clone)]
struct BulkThumbLoaderProps {
    items: Vec<FoundFile>,
    all_cached: Signal<HashMap<String, crate::Thumbnail>>, // full cached rows
}

#[allow(non_snake_case)]
fn BulkThumbLoader(props: BulkThumbLoaderProps) -> Element {
    let items = props.items.clone();
    let all_cached_sig = props.all_cached.clone();

    // Enqueue tasks (lightweight; no blocking primitives) once per render diff
    use_effect(move || {
        // Limit enqueues per effect to avoid flooding (e.g. first 500 missing)
        let mut scheduled = 0usize;
        for f in items.iter() {
            if scheduled > 500 { break; }
            let path = &f.path;
            let path_key = path.display().to_string();
            let already_cached = {
                let cache = all_cached_sig.read();
                cache.get(&path_key).and_then(|t| t.thumbnail_b64.as_ref()).is_some()
            };
            if f.thumb_data.is_some() || already_cached { continue; }
            let ext_opt = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
            if let Some(ext) = ext_opt {
                let is_img = IMAGE_EXTS.iter().any(|e| *e == ext);
                let is_vid = VIDEO_EXTS.iter().any(|e| *e == ext);
                if is_img || is_vid {
                    enqueue_thumb(path, is_img, is_vid);
                    scheduled += 1;
                }
            }
        }
        // log::warn!("[thumb-enqueue] scheduled={scheduled} (render batch)");
    });

    // Poll results channel and apply updates (single future instance per component)
    {
        let mut all_cached_sig = all_cached_sig.clone();
        let _poller = use_future(move || async move {
            use tokio::time::{sleep, Duration};
            let (_, _, _, res_rx) = &*THUMB_CHANNELS;
            loop {
                let mut applied = 0usize;
                while let Ok((path, thumb)) = res_rx.try_recv() {
                    use chrono::Utc;
                    let key = path.display().to_string();
                    let mut cache_w = all_cached_sig.write();
                    let entry = cache_w.entry(key.clone()).or_insert_with(|| crate::Thumbnail {
                        db_created: Utc::now().into(),
                        path: key.clone(),
                        filename: std::path::Path::new(&key).file_name().and_then(|n| n.to_str()).unwrap_or("").into(),
                        file_type: "other".into(),
                        size: 0,
                        description: None,
                        caption: None,
                        tags: Vec::new(),
                        category: None,
                        embedding: None,
                        thumbnail_b64: None,
                        modified: Some(Utc::now().into()),
                        hash: None,
                    });
                    if entry.thumbnail_b64.is_none() { entry.thumbnail_b64 = Some(thumb.clone()); }
                    applied += 1;
                }
                if applied > 0 { log::warn!("[thumb-poll] applied={applied}"); }
                sleep(Duration::from_millis(120)).await;
            }
        });
    }

    rsx! { div { class: "hidden" }}
}

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
    let collapsed_cats = use_signal(|| HashSet::<String>::new());

    // Provide contexts needed by rows (category width & grouped flag) BEFORE rendering descendants
    // so that any hooks inside descendants do not shift ordering relative to these provides.
    provide_context(props.category_col_width.clone());
    provide_context(props.group_by_category.clone());

    // Precompute content so rsx sibling order stays constant (after providing context)
    let props_for_render = props.clone();
    let content = if *props.view_mode.read() == ViewMode::Icons {
        render_icons(props_for_render, collapsed_cats.clone(), enriched_records.clone(), filtered.grouped.read().clone())
    } else {
        render_details(props_for_render, collapsed_cats.clone(), enriched_records.clone(), filtered.grouped.read().clone(), filtered.categories_available.clone())
    };
    log::warn!("[results] render complete filtered_items={} ui_nodes_ready", items_for_loader.len());
    rsx! {
        // Removed dynamic key so component instance persists; pass parent-owned generating signal
        BulkThumbLoader { items: items_for_loader, all_cached: all_cached_for_loader }
        {content}
    }
}

fn render_icons(
    props: ResultsProps,
    mut collapsed_cats: Signal<HashSet<String>>,
    enriched: Vec<crate::utilities::types::FileRecord>,
    grouped_opt_new: Option<BTreeMap<String, Vec<crate::utilities::types::FileRecord>>>,
) -> Element {
    let selected_path = props.selected_path;
    let multi_selected = props.selected_paths;
    let ai_desc = props.ai_descriptions;
    let all_cached = props.all_cached;
    let group = *props.group_by_category.read();
    let grouped_opt = grouped_opt_new.clone();
    let ai_active = *props.ai_search_active.read();
    let ai_results = props.ai_search_results.read().clone();
        // collapsed_cats passed from parent
    // Prebuild grouped icon sections (avoid inline let in rsx loops which can confuse parser)
    let mut grouped_nodes: Vec<Element> = Vec::new();
    if group {
        if let Some(groups) = grouped_opt.as_ref() {
            log::warn!("[icons] grouped categories={} total_items={}", groups.len(), enriched.len());
            for (cat_name, items) in groups.iter() {
                let cat_id = cat_name.clone();
                let collapsed_now = collapsed_cats.read().contains(&cat_id);
                let rotate_cls = if collapsed_now { " rotate-[-90deg]" } else { "" };
                let items_clone: Vec<FoundFile> = items.iter().map(|r| FoundFile { path: r.path.clone(), modified: r.modified, created: r.created, size: r.size, kind: r.kind.clone(), thumb_data: r.thumb_data.clone() }).collect();
                let node = rsx! {
                    div {
                        key: "cat-{cat_id}",
                        class: "space-y-1 border border-stroke rounded-md overflow-hidden ",
                        button {
                            "data-style": "outline",
                            class: "button w-full flex items-center justify-center px-2 py-1 cursor-pointer select-none ",
                            onclick: move |_| {
                                let mut set = collapsed_cats.write();
                                if set.contains(&cat_id) {
                                    set.remove(&cat_id);
                                } else {
                                    set.insert(cat_id.clone());
                                }
                            },
                            div { class: "flex items-center gap-2",
                                i { class: format!("material-icons transition-transform{rotate_cls}"),
                                    "expand_more"
                                }
                                span { class: "text-11px font-semibold uppercase tracking-wide text-weak",
                                    "{cat_name} ({items_clone.len()})"
                                }
                            }
                            if collapsed_now {
                                span { class: "text-8px text-weak", "{items_clone.len()} items" }
                            }
                        }
                        if !collapsed_now {
                            div { class: "grid gap-3 grid-cols-[repeat(auto-fill,minmax(120px,1fr))] p-2",
                                for it in items_clone.iter() {
                                    {
                                        icon_card(
                                            it.path.display().to_string(),
                                            it.thumb_data.clone(),
                                            it.icon_name().into(),
                                            None,
                                            None,
                                            None,
                                            selected_path,
                                            multi_selected,
                                            ai_desc,
                                            all_cached,
                                        )
                                    }
                                }
                            }
                        }
                    }
                };
                grouped_nodes.push(node);
            }
        }
    }

    rsx! {
        div { class: "space-y-6",
            if ai_active {
                div { class: "mb-4",
                    h3 { class: "text-12px font-semibold uppercase tracking-wide text-weak mb-2",
                        "AI Results"
                    }
                    if ai_results.is_empty() {
                        p { class: "text-weak text-11px", "No AI matches yet." }
                    } else {
                        div { class: "grid gap-3 grid-cols-[repeat(auto-fill,minmax(120px,1fr))]",
                            for meta in ai_results.iter() {
                                {
                                    icon_card(
                                        meta.path.clone(),
                                        meta.thumb_b64.clone().or(meta.thumbnail_path.clone()),
                                        meta.file_type.clone(),
                                        meta.description.clone(),
                                        meta.category.clone(),
                                        meta.similarity_score,
                                        selected_path,
                                        multi_selected,
                                        ai_desc,
                                        all_cached,
                                    )
                                }
                            }
                        }
                    }
                }
            }
            if group {
                {grouped_nodes.into_iter()}
            } else {
                div { class: "grid gap-3 grid-cols-[repeat(auto-fill,minmax(120px,1fr))]",
                    for r in enriched.iter() {
                        {
                            icon_card(
                                r.path.display().to_string(),
                                r.thumb_data.clone(),
                                r.kind.icon_name().into(),
                                r.description.clone(),
                                r.category.clone(),
                                None,
                                selected_path,
                                multi_selected,
                                ai_desc,
                                all_cached,
                            )
                        }
                    }
                }
            }
        }
    }
}

fn icon_card(path: String, thumb: Option<String>, file_type: String, desc: Option<String>, cat: Option<String>, similarity_val: Option<f32>, mut selected_path: Signal<Option<std::path::PathBuf>>, mut selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>, ai_desc: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String, crate::Thumbnail>>) -> Element {
    // Precompute state
    let selected = selected_path.read().as_ref().map(|p| p.display().to_string() == path).unwrap_or(false);
    let multi_selected_state = selected_paths.read().contains(&std::path::PathBuf::from(&path));
    let ai_desc_map = ai_desc.read();
    let file_records_ctx = dioxus::prelude::try_consume_context::<Memo<Vec<crate::utilities::types::FileRecord>>>();
    let (cat_ctx, desc_ctx) = if let Some(memo) = file_records_ctx.as_ref() {
        if let Some(rec) = memo.read().iter().find(|r| r.path.display().to_string()==path) {
            (rec.category.clone(), rec.description.clone())
        } else { (None, None) }
    } else { (None, None) };
    let desc_final = desc.or(desc_ctx).or(ai_desc_map.get(&path).cloned());
    let cat_final = cat.or(cat_ctx).or(all_cached.read().get(&path).and_then(|t| t.category.clone()));
    let similarity: Option<String> = similarity_val.map(|s| format!("{s:.3}"));
    let style = if multi_selected_state { "border-indigo-400 bg-indigo-500/15" } else if selected { "border-accent selected-item" } else { "border-stroke bg-panel" };

    // Independent clones for each closure to avoid moving the same String multiple times
    let path_click = path.clone();
    let path_context = path.clone();
    let path_open = path.clone();
    let path_reveal = path.clone();
    let path_select = path.clone();
    let path_copy = path.clone();
    // Context-provided signals for bulk generation (if available)
    let bulk_progress = try_consume_context::<Signal<(usize,usize)>>();
    let bulk_generating = try_consume_context::<Signal<bool>>();
    let ai_engine_sig = try_consume_context::<Signal<Option<crate::ai::AISearchEngine>>>();
    let ui_settings_sig = try_consume_context::<Signal<crate::settings::UiSettings>>();

    rsx! {
        ContextMenu { key: "icon-{path}",
            ContextMenuTrigger { class: "p-0 m-0 border-0 bg-transparent flex",
                div {
                    class: "p-2 rounded-lg border text-center flex flex-col gap-2 cursor-pointer transition hover:border-accent select-none flex-1 {style}",
                    onclick: move |evt| {
                        let pb = std::path::PathBuf::from(path_click.clone());
                        let ctrl = evt.modifiers().ctrl() || evt.modifiers().meta();
                        if ctrl {
                            let mut set = selected_paths.write();
                            if set.contains(&pb) {
                                set.remove(&pb);
                            } else {
                                set.insert(pb.clone());
                            }
                        } else {
                            selected_path.set(Some(pb.clone()));
                            let mut set = selected_paths.write();
                            set.clear();
                            set.insert(pb);
                        }
                    },
                    oncontextmenu: move |_| {
                        if !selected {
                            selected_path.set(Some(std::path::PathBuf::from(path_context.clone())));
                        }
                        if !multi_selected_state {
                            let mut set = selected_paths.write();
                            let pb = std::path::PathBuf::from(path_context.clone());
                            if !set.contains(&pb) {
                                set.clear();
                                set.insert(pb);
                            }
                        }
                    },
                    div { class: "relative w-full aspect-square rounded-md overflow-hidden bg-muted flex items-center justify-center",
                        if let Some(t) = thumb.clone() {
                            img {
                                class: "object-cover w-full h-full max-w-[128px] max-h-[128px] block",
                                src: "{t}",
                            }
                        } else if let Some(cached_thumb) = all_cached
                            .read()
                            .get(&path)
                            .and_then(|t| t.thumbnail_b64.clone())
                        {
                            img {
                                class: "object-cover w-full h-full max-w-[128px] max-h-[128px] block",
                                src: "{cached_thumb}",
                            }
                        } else {
                            div { class: "flex flex-col items-center justify-between text-weak gap-1",
                                i { class: "material-icons", "{file_type}" }
                                span { class: "text-8px animate-pulse", "loading" }
                            }
                        }
                        if let Some(sim) = similarity {
                            span { class: "absolute top-1 right-1 bg-indigo-500/80 text-white text-8px px-1 rounded",
                                "{sim}"
                            }
                        }
                    }
                    if let Some(c) = cat_final.as_ref() {
                        if !c.is_empty() {
                            span { class: "text-10px px-2 py-0.5 rounded-full bg-muted border border-stroke truncate",
                                "{c}"
                            }
                        }
                    }
                    if let Some(d) = desc_final.as_ref() {
                        p { class: "text-10px leading-snug line-clamp-3", "{d}" }
                    }
                    {
                        let fname_raw = std::path::Path::new(&path)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or("");
                        let fname = shorten_middle(fname_raw, 40);
                        rsx! {
                            span { class: "text-10px break-all text-weak", title: "{fname_raw}", "{fname}" }
                        }
                    }
                }
            }
            ContextMenuContent { class: "context-menu-content",
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("open:{path}"),
                    index: 0usize,
                    on_select: move |_| {
                        let _ = open::that(&path_open);
                    },
                    i { class: "material-icons", "open_in_new" }
                    span { "Open" }
                }
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("reveal:{path}"),
                    index: 1usize,
                    on_select: move |_| {
                        let pb = std::path::PathBuf::from(path_reveal.clone());
                        if let Some(parent) = pb.parent() {
                            let _ = open::that(parent);
                        }
                    },
                    i { class: "material-icons", "folder_open" }
                    span { "Show in Folder" }
                }
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("select:{path}"),
                    index: 2usize,
                    on_select: move |_| {
                        let pb = std::path::PathBuf::from(path_select.clone());
                        let mut set = selected_paths.write();
                        if set.contains(&pb) {
                            set.remove(&pb);
                        } else {
                            set.insert(pb.clone());
                        }
                        selected_path.set(Some(pb));
                    },
                    i { class: "material-icons",
                        {
                            if multi_selected_state || selected {
                                "check_box"
                            } else {
                                "check_box_outline_blank"
                            }
                        }
                    }
                    span {
                        if multi_selected_state || selected {
                            "Deselect"
                        } else {
                            "Select"
                        }
                    }
                }
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("copy:{path}"),
                    index: 3usize,
                    on_select: move |_| {
                        log::debug!("copy path: {path_copy}");
                    },
                    i { class: "material-icons", "content_copy" }
                    span { "Copy Path (log)" }
                }
                // Generate for Selected (only show if engine & signals present)
                {
                    let maybe = (
                        bulk_progress.clone(),
                        bulk_generating.clone(),
                        ai_engine_sig.clone(),
                        ui_settings_sig.clone(),
                    );
                    match maybe {
                        (Some(bp), Some(bg), Some(engine_sig), Some(ui_sig)) => {
                            rsx! {
                                ContextMenuItem {
                                    class: "context-menu-item",
                                    value: format!("gen-selected:{path}"),
                                    index: 4usize,
                                    disabled: *bg.read() || engine_sig.read().is_none(),
                                    on_select: move |_| {
                                        if engine_sig.read().is_some() {
                                            let selected_set = selected_paths.read().clone();
                                            let mut rows: Vec<crate::utilities::types::FoundFile> = Vec::new();
                                            if selected_set.is_empty() {
                                                rows.push(crate::utilities::types::FoundFile {
                                                    path: std::path::PathBuf::from(path.clone()),
                                                    modified: None,
                                                    created: None,
                                                    size: None,
                                                    kind: crate::utilities::types::MediaKind::Other,
                                                    thumb_data: None,
                                                });
                                            } else {
                                                for p in selected_set.iter() {
                                                    rows.push(crate::utilities::types::FoundFile {
                                                        path: p.clone(),
                                                        modified: None,
                                                        created: None,
                                                        size: None,
                                                        kind: crate::utilities::types::MediaKind::Other,
                                                        thumb_data: None,
                                                    });
                                                }
                                            }
                                            crate::ai::bulk::spawn_bulk_generate(
                                                engine_sig.read().clone(),
                                                rows,
                                                ui_sig.read().ai_prompt_template.clone(),
                                                bp.clone(),
                                                bg.clone(),
                                                Signal::new(None::<String>),
                                                ui_sig.read().overwrite_descriptions,
                                            );
                                        }
                                    },
                                    i { class: "material-icons", "auto_fix_high" }
                                    span {
                                        if *bg.read() {
                                            "Generating..."
                                        } else {
                                            "Generate for Selected"
                                        }
                                    }
                                }
                            }
                        }
                        _ => rsx! {
                            Fragment {}
                        },
                    }
                }
            }
        }
    }
}

fn render_details(
    props: ResultsProps,
    mut collapsed_cats: Signal<HashSet<String>>,
    enriched: Vec<crate::utilities::types::FileRecord>,
    grouped_opt_new: Option<BTreeMap<String, Vec<crate::utilities::types::FileRecord>>>,
    categories_available: Memo<std::collections::BTreeSet<String>>,
) -> Element {
    let sort = props.sort;
    let _ui = props.ui;
    let selected_path = props.selected_path;
    let multi_selected = props.selected_paths;
    let ai_descriptions = props.ai_descriptions;
    let all_cached = props.all_cached;
    let ai_active = *props.ai_search_active.read();
    let ai_results = props.ai_search_results.read().clone();
    let group = *props.group_by_category.read();
    let grouped_opt = grouped_opt_new.clone();
        // collapsed_cats passed from parent

    // Re-sort locally (enriched already filtered). This mirrors previous logic.
    let mut items: Vec<FoundFile> = enriched.iter().map(|r| FoundFile { path: r.path.clone(), modified: r.modified, created: r.created, size: r.size, kind: r.kind.clone(), thumb_data: r.thumb_data.clone() }).collect();
    log::debug!("[details] render start items={} grouped={} ai_active={} ai_results={}", items.len(), group, ai_active, ai_results.len());
    let sv = sort.read();
    items.sort_by(|a,b| {
        let ord = match sv.by {
            SortBy::Name => a.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase()
                .cmp(&b.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase()),
            SortBy::Category => {
                let ac = all_cached.read().get(&a.path.display().to_string()).and_then(|t| t.category.clone()).unwrap_or_default();
                let bc = all_cached.read().get(&b.path.display().to_string()).and_then(|t| t.category.clone()).unwrap_or_default();
                ac.to_lowercase().cmp(&bc.to_lowercase())
            },
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
        // (inline ext filter test block removed; filters now live in header dropdowns)
    };

    rsx! {
        div { class: "details space-y-6",
            if ai_active && !ai_results.is_empty() {
                div { class: "space-y-1",
                    h3 { class: "text-11px font-semibold uppercase tracking-wide text-weak px-1",
                        "AI Results ({ai_results.len()})"
                    }
                    for meta in ai_results.iter() {
                        {ai_detail_row(meta.clone(), selected_path, ai_descriptions, all_cached)}
                    }
                }
            }
            // existing original listing
            {
                details_header(
                    sort,
                    props.ui,
                    props.detail_column_widths,
                    props.category_col_width,
                    props.resizing_col,
                    &items,
                    show_modified,
                    show_created,
                    show_path_col,
                    *props.group_by_category.read(),
                    props.filters,
                    categories_available.clone(),
                )
            }
            if group {
                if let Some(groups) = grouped_opt.as_ref() {
                    for (cat , list) in groups.iter() {
                        {
                            let cat_name = cat.clone();
                            let is_collapsed = collapsed_cats.read().contains(&cat_name);
                            let rotate_cls = if is_collapsed { " rotate-[-90deg]" } else { "" };
                            rsx! {
                                    div {
                                    key: "g-{cat_name}",
                                    class: "mt-4 border border-stroke rounded-md overflow-hidden",
                                    button {
                                        "data-style": "outline",
                                        class: "button w-full flex items-center justify-between px-2 py-1 cursor-pointer select-none",
                                        onclick: move |_| {
                                            let mut set = collapsed_cats.write();
                                            if set.contains(&cat_name) {
                                                set.remove(&cat_name);
                                            } else {
                                                set.insert(cat_name.clone());
                                            }
                                        },
                                        div { class: "flex items-center gap-2",
                                            i { class: format!("material-icons transition-transform{rotate_cls}"),
                                                "expand_more"
                                            }
                                            span { class: "text-11px font-semibold uppercase tracking-wide text-weak",
                                                "{cat} ({list.len()})"
                                            }
                                        }
                                        if is_collapsed {
                                            span { class: "text-8px text-weak", "{list.len()} items" }
                                        }
                                    }
                                    if !is_collapsed {
                                        div { class: "flex flex-col gap-1 p-1",
                                            for it in list.iter() {
                                                {
                                                    let ff = FoundFile {
                                                        path: it.path.clone(),
                                                        modified: it.modified,
                                                        created: it.created,
                                                        size: it.size,
                                                        kind: it.kind.clone(),
                                                        thumb_data: it.thumb_data.clone(),
                                                    };
                                                    detail_row(
                                                        ff,
                                                        selected_path,
                                                        multi_selected,
                                                        ai_descriptions,
                                                        all_cached,
                                                        props.detail_column_widths,
                                                        &common_root,
                                                        show_modified,
                                                        show_created,
                                                        show_path_col,
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                for it in items.iter() {
                    {
                        detail_row(
                            it.clone(),
                            selected_path,
                            multi_selected,
                            ai_descriptions,
                            all_cached,
                            props.detail_column_widths,
                            &common_root,
                            show_modified,
                            show_created,
                            show_path_col,
                        )
                    }
                }
            }
        }
    }
}

fn ai_detail_row(meta: crate::FileMetadata, mut selected_path: Signal<Option<std::path::PathBuf>>, ai_descriptions: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String, crate::Thumbnail>>) -> Element {
    let path = meta.path.clone();
    let selected = selected_path.read().as_ref().map(|p| p.display().to_string() == path).unwrap_or(false);
    let style = if selected { "border-accent" } else { "border-stroke bg-panel" };
    let desc = meta.description.or(ai_descriptions.read().get(&path).cloned());
    let category = meta.category.or(all_cached.read().get(&path).and_then(|t| t.category.clone()));
    let tags = meta.tags.clone();
    let filename = std::path::Path::new(&path).file_name().and_then(|f| f.to_str()).unwrap_or("");
    let similarity_text = meta.similarity_score.map(|s| format!("{s:.3}"));
    rsx! {
        div {
            key: "ai-row-{path}",
            class: "p-2 rounded-md border flex flex-col gap-1 text-11px cursor-pointer {style}",
            onclick: move |_| {
                selected_path.set(Some(std::path::PathBuf::from(path.clone())));
            },
            div { class: "flex items-center gap-2",
                span { class: "font-medium truncate", "{filename}" }
                if let Some(c) = category {
                    span { class: "px-1 rounded bg-muted border border-stroke text-8px",
                        "{c}"
                    }
                }
                if let Some(score_txt) = similarity_text {
                    span { class: "px-1 rounded bg-accent/20 text-accent text-8px", "{score_txt}" }
                }
            }
            if let Some(d) = desc {
                p { class: "line-clamp-2", "{d}" }
            }
            if !tags.is_empty() {
                div { class: "flex flex-wrap gap-1",
                    for t in tags.iter().take(8) {
                        span { class: "px-1 rounded bg-muted border border-stroke text-8px",
                            "{t}"
                        }
                    }
                }
            }
        }
    }
}

fn details_header(
    sort: Signal<SortSetting>,
    ui: Signal<UiSettings>,
    mut widths: Signal<[f32;6]>,
    cat_width: Signal<f32>,
    resizing: Signal<Option<(usize,i32,f32)>>,
    items: &Vec<crate::utilities::types::FoundFile>,
    show_modified: bool,
    show_created: bool,
    show_path_col: bool,
    grouped: bool,
    mut filters: Signal<crate::utilities::types::Filters>,
    categories_available: Memo<std::collections::BTreeSet<String>>,
) -> Element {
    // Fetch ext filter signals from context now that they're not props
    let ext_filters = use_context::<Signal<std::collections::BTreeSet<String>>>();
    let mut ext_enabled = use_context::<Signal<std::collections::BTreeMap<String,bool>>>();
    let w = widths.read();
    // Column order base indexes: 0 Name, (category pseudo-index 6), 1 Path, 2 Modified, 3 Created, 4 Size, 5 Type
    // If grouped, we hide Category column (category shown as pill in group header rows); if not grouped, show Category after Name.
    let mut col_specs: Vec<(usize,f32)> = Vec::new();
    // Name always first; if path hidden we widen only by its own width (keep category separate)
    col_specs.push((0, if show_path_col { w[0] } else { w[0] }));
    if !grouped { col_specs.push((6, *cat_width.read())); }
    if show_path_col { col_specs.push((1, w[1])); }
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
    // Active filter counts
    let f_snapshot = filters.read().clone();
    let cat_ct = f_snapshot.category_filters.len();
    let modified_active = f_snapshot.modified_after.is_some() || f_snapshot.modified_before.is_some();
    let disabled_types = ext_enabled.read().values().filter(|v| !**v).count();
    let any_filters = cat_ct>0 || modified_active || disabled_types>0 || f_snapshot.only_with_thumb || f_snapshot.only_with_description;
    rsx! {
        div {
            class: "results-header grid gap-0 px-0 py-0 items-stretch text-11px select-none",
            style: format!(
                "display:grid;grid-template-columns:{};align-items:stretch;width:100%;",
                template,
            ),
            onmousemove: move |evt| {
                if let Some((col_idx, start_x, start_w)) = resizing.read().clone() {
                    let dx = evt.client_coordinates().x as i32 - start_x;
                    let mut wcopy = widths.read().clone();
                    let new_w = (start_w + (dx as f32 * 0.15)).clamp(0.25, 18.0);
                    if col_idx < wcopy.len() {
                        wcopy[col_idx] = new_w;
                        widths.set(wcopy);
                    }
                }
            },
            span { class: "flex items-center pl-1",
                if any_filters {
                    button {
                        "data-style": "outline",
                        class: "button text-[10px] px-2 py-0",
                        onclick: move |_| {
                            let mut f = filters.write();
                            f.category_filters.clear();
                            f.modified_after = None;
                            f.modified_before = None;
                            f.only_with_thumb = false;
                            f.only_with_description = false;
                            let exts: Vec<String> = ext_filters.read().iter().cloned().collect();
                            let mut emap = ext_enabled.write();
                            for e in exts {
                                emap.insert(e, true);
                            }
                        },
                        i { class: "material-icons text-[14px] mr-1", "cancel" }
                        span { "Clear Filters" }
                    }
                }
            }
            for (width_idx , _) in col_specs.iter() {
                match *width_idx {
                    0 => {
                        resizable_head(
                            header_with_filter(sortable_col("Name", SortBy::Name, sort, ui), None),
                            *width_idx,
                            widths,
                            resizing,
                            items_ref.clone(),
                        )
                    }
                    6 => {
                        let label = header_with_filter(
                            sortable_col("Category", SortBy::Category, sort, ui),
                            Some(
                                category_filter_dropdown(
                                    filters.clone(),
                                    categories_available.read().clone(),
                                    cat_ct,
                                ),
                            ),
                        );
                        let idx_sent = 6usize;
                        let mut cat_width_sig = cat_width.clone();
                        let mut resizing_sig = resizing.clone();
                        rsx! {
                            div {
                                class: "results-header-col relative flex items-center",
                                style: "min-height:28px;",
                                {label}
                                div {
                                    class: "absolute top-0 right-0 h-full group select-none",
                                    style: "width:8px;touch-action:none;cursor:col-resize;user-select:none;z-index:10;right:0;top:0;",
                                    onmousedown: move |evt| {
                                        let start_x = evt.client_coordinates().x as i32;
                                        let start_w = *cat_width_sig.read();
                                        resizing_sig.set(Some((idx_sent, start_x, start_w)));
                                    },
                                    ondoubleclick: move |_| {
                                        let new_w = 0.9_f32;
                                        cat_width_sig.set(new_w);
                                        if let Some(mut ui_sig) = dioxus::prelude::try_consume_context::<
                                            Signal<UiSettings>,
                                        >() {
                                            let mut s = ui_sig.write();
                                            s.category_col_width = Some(new_w);
                                            crate::settings::save_settings(&s);
                                        }
                                    },
                                    div {
                                        class: "absolute top-0 left-1/2 -translate-x-1/2 h-full w-px",
                                        style: "background:rgba(62,62,70,0.15);",
                                    }
                                }
                            }
                        }
                    }
                    1 => {
                        if show_path_col {
                            resizable_head(
                                plain_col("Path"),
                                *width_idx,
                                widths,
                                resizing,
                                items_ref.clone(),
                            )
                        } else {
                            rsx! {
                                span {}
                            }
                        }
                    }
                    2 => {
                        if show_modified {
                            resizable_head(
                                header_with_filter(
                                    sortable_col("Modified", SortBy::Modified, sort, ui),
                                    Some(modified_filter_dropdown(filters.clone(), modified_active)),
                                ),
                                *width_idx,
                                widths,
                                resizing,
                                items_ref.clone(),
                            )
                        } else {
                            rsx! {
                                span {}
                            }
                        }
                    }
                    3 => {
                        if show_created {
                            resizable_head(
                                sortable_col("Created", SortBy::Created, sort, ui),
                                *width_idx,
                                widths,
                                resizing,
                                items_ref.clone(),
                            )
                        } else {
                            rsx! {
                                span {}
                            }
                        }
                    }
                    4 => {
                        resizable_head(
                            sortable_col("Size", SortBy::Size, sort, ui),
                            *width_idx,
                            widths,
                            resizing,
                            items_ref.clone(),
                        )
                    }
                    5 => {
                        resizable_head(
                            header_with_filter(
                                sortable_col("Type", SortBy::Type, sort, ui),
                                Some(
                                    type_filter_dropdown(
                                        ext_filters.clone(),
                                        ext_enabled.clone(),
                                        disabled_types,
                                    ),
                                ),
                            ),
                            *width_idx,
                            widths,
                            resizing,
                            items_ref.clone(),
                        )
                    }
                    _ => {
                        rsx! {
                            span {}
                        }
                    }
                }
            }
        }
    }
}

fn detail_row(
    item: FoundFile,
    mut selected_path: Signal<Option<std::path::PathBuf>>,
    mut selected_paths: Signal<std::collections::HashSet<std::path::PathBuf>>,
    ai_descriptions: Signal<HashMap<String,String>>,
    all_cached: Signal<HashMap<String, crate::Thumbnail>>,
    widths: Signal<[f32;6]>,
    common_root: &Option<std::path::PathBuf>,
    show_modified: bool,
    show_created: bool,
    show_path_col: bool
) -> Element {
    let abs_path_str = item.path.display().to_string();
    // Independent clones of frequently used values to avoid moving `item` into closures
    let item_path_buf_for_click = item.path.clone();
    let item_path_buf_for_context = item.path.clone();
    let abs_open = abs_path_str.clone();
    let abs_reveal = abs_path_str.clone();
    let abs_select = abs_path_str.clone();
    let abs_copy = abs_path_str.clone();
    let thumb_data = item.thumb_data.clone();
    let icon_name_for_thumb = item.icon_name();
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
    let raw_name = item.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
    let name = shorten_middle(&raw_name, 48);
    let size_txt = item.size.map(|s| format_size(s, DECIMAL)).unwrap_or("-".into());
    let modified_txt = item.modified.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or("-".into());
    let created_txt  = item.created.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or("-".into());
    let ext_txt = item.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    // Attempt enriched metadata via FileRecord context (category/description already resolved upstream for grouping, but description may be used in future)
    let file_records_ctx = dioxus::prelude::try_consume_context::<Memo<Vec<crate::utilities::types::FileRecord>>>();
    let selected = selected_path.read().as_ref().map(|p| p == &item.path).unwrap_or(false);
    let multi_selected_state = selected_paths.read().contains(&item.path);
    let row_style = if multi_selected_state { "selected-item" } else if selected { "selected-item border-accent" } else { "non-selected-item border-stroke" };
    let desc_opt = ai_descriptions.read().get(&abs_path_str).cloned();
    let cat_opt = all_cached.read().get(&abs_path_str).and_then(|t| t.category.clone());
    // Independent render clones so base Options remain intact for closure clones
    let desc_render = desc_opt.clone();
    let cat_render = cat_opt.clone();
    // Pre-clone for closure capture (avoid moving later when rendering cells)
    // Clones for onclick closure capture
    let desc_for_click_outer = desc_opt.clone();
    let cat_for_click_outer = cat_opt.clone();
    // Separate clones for closure (selection) vs context menu/header formatting
    // Separate clone for context menu so the move closure doesn't consume the one we need for formatting menu items
    let abs_path_for_context_menu = abs_path_str.clone();

    let w = widths.read();
    // Determine if grouped by reading context of group_by_category (optional)
    let grouped = dioxus::prelude::try_consume_context::<Signal<bool>>().map(|s| *s.read()).unwrap_or(false);
    // Column order match header: 0 Name, 6 Category (if !grouped), 1 Path?, 2 Modified?, 3 Created?, 4 Size, 5 Type
    let mut col_order: Vec<usize> = Vec::new();
    col_order.push(0); // Name
    if !grouped { col_order.push(6); } // Category
    if show_path_col { col_order.push(1); }
    if show_modified { col_order.push(2); }
    if show_created  { col_order.push(3); }
    col_order.push(4);
    col_order.push(5);

    let mut template = "56px ".to_string();
    // Obtain category width via context (category_col_width provided through ResultsProps? stored separately in header). For rows we recompute same grid.
    let cat_w = dioxus::prelude::try_consume_context::<Signal<f32>>().map(|s| *s.read()).unwrap_or(0.9);
    for idx in &col_order {
        let fr = match *idx {
            0 => w[0],
            1 => w[1],
            2 => w[2],
            3 => w[3],
            4 => w[4],
            5 => w[5],
            6 => cat_w,
            _ => 0.8
        };
        template.push_str(&format!("{fr}fr "));
    }

    // Use stable key clone (not moved into onclick closure)
    let abs_path_for_key = abs_path_str.clone();
    rsx! {
        ContextMenu { key: "det-{abs_path_for_key}",
            ContextMenuTrigger { class: "flex p-0 m-0 border-0 bg-transparent",
                div {
                    class: "detail-row grid items-center gap-2 rounded-md border px-2 h-[50px] cursor-pointer text-11px select-none {row_style}",
                    style: format!("display:grid;grid-template-columns:{};width:100%;height:50px", template),
                    onclick: move |evt| {
                        let pb = item_path_buf_for_click.clone();
                        let ctrl = evt.modifiers().ctrl() || evt.modifiers().meta();
                        let shift = evt.modifiers().shift();
                        if ctrl {
                            let mut set = selected_paths.write();
                            if set.contains(&pb) {
                                set.remove(&pb);
                            } else {
                                set.insert(pb.clone());
                            }
                        } else if shift {
                            selected_path.set(Some(pb.clone()));
                        } else {
                            selected_path.set(Some(pb.clone()));
                            let mut set = selected_paths.write();
                            set.clear();
                            set.insert(pb.clone());
                        }
                        let path_for_save = pb.clone();
                        let thumb_clone = thumb_data
                            .clone()
                            .or(
                                all_cached
                                    .read()
                                    .get(&abs_path_str)
                                    .and_then(|t| t.thumbnail_b64.clone()),
                            );
                        let desc_for_row = desc_for_click_outer.clone();
                        let cat_for_row = cat_for_click_outer.clone();
                        spawn(async move {
                            let meta = std::fs::metadata(&path_for_save).ok();
                            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                            let modified = meta
                                .and_then(|m| m.modified().ok())
                                .map(|st| chrono::DateTime::<chrono::Utc>::from(st));
                            let ext = path_for_save
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|s| s.to_ascii_lowercase());
                            let file_type = if let Some(ext) = ext.clone() {
                                if IMAGE_EXTS.iter().any(|e| *e == ext) {
                                    "image".to_string()
                                } else if VIDEO_EXTS.iter().any(|e| *e == ext) {
                                    "video".to_string()
                                } else {
                                    ext
                                }
                            } else {
                                "other".into()
                            };
                            let row = crate::Thumbnail {
                                db_created: Utc::now().into(),
                                path: path_for_save.display().to_string(),
                                filename: path_for_save
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("")
                                    .to_string(),
                                file_type,
                                size,
                                description: desc_for_row,
                                caption: None,
                                tags: Vec::new(),
                                category: cat_for_row,
                                embedding: None,
                                thumbnail_b64: thumb_clone,
                                modified: if let Some(date) = modified {
                                    Some(date.into())
                                } else {
                                    Some(Utc::now().into())
                                },
                                hash: None,
                            };
                            if let Err(e) = crate::database::save_thumbnail_row(row).await {
                                log::error!("save_thumbnail_row (detail) failed: {e}");
                            }
                        });
                    },
                    oncontextmenu: move |_| {
                        let pb = item_path_buf_for_context.clone();
                        if !selected {
                            selected_path.set(Some(pb.clone()));
                        }
                        if !multi_selected_state {
                            let mut set = selected_paths.write();
                            set.clear();
                            set.insert(pb);
                        }
                    },
                    // Thumb
                    div { class: "thumb flex items-center justify-center rounded bg-muted",
                        if let Some(img) = thumb_data.clone() {
                            img {
                                src: "{img}",
                                class: "object-cover w-full h-full max-w-[48px] max-h-[48px] block",
                            }
                        } else if let Some(cached) = all_cached
                            .read()
                            .get(&abs_path_str)
                            .and_then(|t| t.thumbnail_b64.clone())
                        {
                            img {
                                src: "{cached}",
                                class: "object-cover w-full h-full max-w-[48px] max-h-[48px] block",
                            }
                        } else {
                            div { class: "flex flex-col items-center justify-center text-weak gap-0.5 w-full h-full",
                                i { class: "material-icons text-base", "{icon_name_for_thumb}" }
                                span { class: "text-[9px] animate-pulse", "loading" }
                            }
                        }
                    }
                    // Name
                    div { class: "truncate font-semibold", title: "{name}", "{name}" }
                    // Category column (only when not grouped)
                    if !grouped {
                        div {
                            class: "truncate text-weak",
                            title: cat_render.clone().unwrap_or_default(),
                            {cat_render.clone().unwrap_or_default()}
                        }
                    }
                    if show_path_col {
                        div {
                            class: "truncate text-weak",
                            title: "{abs_path_str}",
                            "{shorten_middle(&display_rel, 60)}"
                        }
                    }
                    if show_modified {
                        span { "{modified_txt}" }
                    }
                    if show_created {
                        span { "{created_txt}" }
                    }
                    span { "{size_txt}" }
                    div { class: "flex items-center gap-1 truncate",
                        span { "{ext_txt}" }
                        if let Some(ref cat) = cat_render {
                            span { class: "px-1 rounded bg-muted border border-stroke text-8px",
                                "{cat}"
                            }
                        }
                        if let Some(ref desc) = desc_render {
                            span {
                                class: "px-1 rounded bg-accent-weak text-8px truncate",
                                title: "{desc}",
                                "AI"
                            }
                        }
                    }
                } // end inner row div
            }
            ContextMenuContent { class: "context-menu-content",
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("open:{abs_path_for_context_menu}"),
                    index: 0usize,
                    on_select: move |_| {
                        let _ = open::that(&abs_open);
                    },
                    i { class: "material-icons", "open_in_new" }
                    span { "Open" }
                }
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("reveal:{abs_path_for_context_menu}"),
                    index: 1usize,
                    on_select: move |_| {
                        let pb = std::path::PathBuf::from(&abs_reveal);
                        if let Some(parent) = pb.parent() {
                            let _ = open::that(parent);
                        }
                    },
                    i { class: "material-icons", "folder_open" }
                    span { "Show in Folder" }
                }
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("select:{abs_path_for_context_menu}"),
                    index: 2usize,
                    on_select: move |_| {
                        let pb = std::path::PathBuf::from(&abs_select);
                        let mut set = selected_paths.write();
                        if set.contains(&pb) {
                            set.remove(&pb);
                        } else {
                            set.insert(pb.clone());
                        }
                        selected_path.set(Some(pb));
                    },
                    i { class: "material-icons",
                        {
                            if multi_selected_state || selected {
                                "check_box"
                            } else {
                                "check_box_outline_blank"
                            }
                        }
                    }
                    span {
                        if multi_selected_state || selected {
                            "Deselect"
                        } else {
                            "Select"
                        }
                    }
                }
                ContextMenuItem {
                    class: "context-menu-item",
                    value: format!("copy:{abs_path_for_context_menu}"),
                    index: 3usize,
                    on_select: move |_| {
                        log::debug!("copy path: {abs_copy}");
                    },
                    i { class: "material-icons", "content_copy" }
                    span { "Copy Path (log)" }
                }
                {
                    let bulk_progress = try_consume_context::<Signal<(usize, usize)>>();
                    let bulk_generating = try_consume_context::<Signal<bool>>();
                    let ai_engine_sig = try_consume_context::<
                        Signal<Option<crate::ai::AISearchEngine>>,
                    >();
                    let ui_settings_sig = try_consume_context::<
                        Signal<crate::settings::UiSettings>,
                    >();
                    if let (Some(bp), Some(bg), Some(engine_sig), Some(ui_sig)) = (
                        bulk_progress,
                        bulk_generating,
                        ai_engine_sig,
                        ui_settings_sig,
                    ) {
                        rsx! {
                            ContextMenuItem {
                                class: "context-menu-item",
                                value: format!("gen-selected:{abs_path_for_context_menu}"),
                                index: 4usize,
                                disabled: *bg.read() || engine_sig.read().is_none(),
                                on_select: move |_| {
                                    if engine_sig.read().is_some() {
                                        let selected_set = selected_paths.read().clone();
                                        let mut rows: Vec<crate::utilities::types::FoundFile> = Vec::new();
                                        if selected_set.is_empty() {
                                            rows.push(crate::utilities::types::FoundFile {
                                                path: std::path::PathBuf::from(&abs_path_for_context_menu),
                                                modified: None,
                                                created: None,
                                                size: None,
                                                kind: crate::utilities::types::MediaKind::Other,
                                                thumb_data: None,
                                            });
                                        } else {
                                            for p in selected_set.iter() {
                                                rows.push(crate::utilities::types::FoundFile {
                                                    path: p.clone(),
                                                    modified: None,
                                                    created: None,
                                                    size: None,
                                                    kind: crate::utilities::types::MediaKind::Other,
                                                    thumb_data: None,
                                                });
                                            }
                                        }
                                        crate::ai::bulk::spawn_bulk_generate(
                                            engine_sig.read().clone(),
                                            rows,
                                            ui_sig.read().ai_prompt_template.clone(),
                                            bp.clone(),
                                            bg.clone(),
                                            Signal::new(None::<String>),
                                            ui_sig.read().overwrite_descriptions,
                                        );
                                    }
                                },
                                i { class: "material-icons", "auto_fix_high" }
                                span {
                                    if *bg.read() {
                                        "Generating..."
                                    } else {
                                        "Generate for Selected"
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {
                            span {}
                        }
                    }
                }
            }
        }
    }

}

fn shorten_middle(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    if max <= 3 { return s[..max.min(s.len())].to_string(); }
    let keep = max - 3;
    let front = keep / 2;
    let back = keep - front;
    format!("{}...{}", &s[..front], &s[s.len()-back..])
}

// Modified details_header still calls plain_col -> re-add helper (was removed)
fn plain_col(label: &str) -> Element {
    rsx! {
        span { class: "truncate", "{label}" }
    }
}

// Wrap a header main element with optional filter dropdown trigger
fn header_with_filter(main: Element, dropdown: Option<Element>) -> Element {
    if dropdown.is_none() { return main; }
    let dd = dropdown.unwrap();
    // Use relative container; place filter trigger absolutely at far right to prevent width shift.
    rsx! {
        div { class: "relative flex items-center w-full pr-5", // extra right padding so text not overlapped
            div {
                class: "flex items-center gap-1 overflow-hidden",
                style: "max-width:100%;",
                {main}
            }
            div { class: "absolute top-1/2 -translate-y-1/2 right-0 flex items-center",
                {dd}
            }
        }
    }.into()
}

fn category_filter_dropdown(mut filters: Signal<crate::utilities::types::Filters>, categories: std::collections::BTreeSet<String>, active_ct: usize) -> Element {
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

fn modified_filter_dropdown(mut filters: Signal<crate::utilities::types::Filters>, active: bool) -> Element {
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

fn set_date_range(filters: &mut Signal<crate::utilities::types::Filters>, days: i64) {
    use chrono::{Local, Duration};
    let now = Local::now().date_naive();
    let after = now - Duration::days(days);
    let mut f = filters.write();
    f.modified_after = Some(after.to_string());
    f.modified_before = Some(now.to_string());
}

fn clear_dates(filters: &mut Signal<crate::utilities::types::Filters>) {
    let mut f = filters.write();
    f.modified_after = None;
    f.modified_before = None;
}

fn type_filter_dropdown(ext_filters: Signal<std::collections::BTreeSet<String>>, mut ext_enabled: Signal<std::collections::BTreeMap<String,bool>>, disabled_ct: usize) -> Element {
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
fn sortable_col(label: &str, col: SortBy, mut sort: Signal<SortSetting>, _ui: Signal<UiSettings>) -> Element {
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
fn resizable_head(content: Element, width_idx: usize, mut widths: Signal<[f32;6]>, mut resizing: Signal<Option<(usize,i32,f32)>>, items: Vec<crate::utilities::types::FoundFile>) -> Element {
    // let active = resizing.read().clone().map(|(i,_,_)| i == width_idx).unwrap_or(false);
    rsx! {
        div { class: "results-header-col relative flex items-center",
            {content}
            div {
                class: "absolute top-0 right-0 h-full group select-none",
                style: "width:8px;touch-action:none;cursor:col-resize;user-select:none;z-index:10;right:0;top:0;",
                onmousedown: move |evt| {
                    let start_x = evt.client_coordinates().x as i32;
                    let start_w = widths.read()[width_idx];
                    resizing.set(Some((width_idx, start_x, start_w)));
                },
                ondoubleclick: move |_| {
                    let mut wcopy = widths.read().clone();
                    let target = if items.is_empty() {
                        1.0
                    } else {
                        match width_idx {
                            0 => {
                                let max_len = items
                                    .iter()
                                    .take(500)
                                    .filter_map(|f| f.path.file_name().and_then(|n| n.to_str()))
                                    .map(|s| s.len())
                                    .max()
                                    .unwrap_or(8);
                                (max_len as f32 / 18.0).clamp(0.4, 6.0)
                            }
                            1 => {
                                let max_len = items
                                    .iter()
                                    .take(300)
                                    .map(|f| {
                                        f
                                            .path
                                            .parent()
                                            .map(|p| p.display().to_string().len())
                                            .unwrap_or(1)
                                    })
                                    .max()
                                    .unwrap_or(12);
                                (max_len as f32 / 30.0).clamp(0.6, 6.0)
                            }
                            2 => 0.9,
                            3 => 0.9,
                            4 => 0.7,
                            5 => 0.6,
                            _ => 1.0,
                        }
                    };
                    wcopy[width_idx] = target;
                    let sum: f32 = wcopy.iter().sum();
                    if sum > 0.0 {
                        let desired = 7.2_f32;
                        let scale = (desired / sum).clamp(0.7, 1.3);
                        for wv in &mut wcopy {
                            *wv = (*wv * scale).clamp(0.35, 6.0);
                        }
                    }
                    widths.set(wcopy);
                    if let Some(mut ui_sig) = dioxus::prelude::try_consume_context::<
                        Signal<UiSettings>,
                    >() {
                        let mut settings = ui_sig.write();
                        settings.detail_column_widths = Some(widths.read().clone());
                        crate::settings::save_settings(&settings);
                    }
                },
                div {
                    class: "absolute top-0 left-1/2 -translate-x-1/2 h-full w-px",
                    style: "background:rgba(62, 62, 70, 0.15);",
                }
            }
        }
    }
}
