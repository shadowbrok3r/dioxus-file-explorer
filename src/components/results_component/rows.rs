use dioxus::prelude::*;
use dioxus_primitives::context_menu::{ContextMenu, ContextMenuTrigger, ContextMenuContent, ContextMenuItem};
use std::collections::{HashMap, HashSet};
use humansize::{format_size, DECIMAL};
use crate::utilities::types::{FoundFile, IMAGE_EXTS, VIDEO_EXTS};
use crate::settings::{UiSettings, SortBy, SortSetting};
use chrono::Utc;
use super::utilities::{shorten_middle};

pub fn ai_detail_row(meta: crate::FileMetadata, mut selected_path: Signal<Option<std::path::PathBuf>>, ai_descriptions: Signal<HashMap<String,String>>, all_cached: Signal<HashMap<String, crate::Thumbnail>>) -> Element {
    let path = meta.path.clone();
    let selected = selected_path.read().as_ref().map(|p| p.display().to_string() == path).unwrap_or(false);
    let style = if selected { "is-selected" } else { "" };
    let desc = meta.description.or(ai_descriptions.read().get(&path).cloned());
    let category = meta.category.or(all_cached.read().get(&path).and_then(|t| t.category.clone()));
    let tags = meta.tags.clone();
    let filename = std::path::Path::new(&path).file_name().and_then(|f| f.to_str()).unwrap_or("");
    let similarity_text = meta.similarity_score.map(|s| format!("{s:.3}"));
    rsx! {
        div {
            key: "ai-row-{path}",
            class: "ai-row p-2 rounded-md border flex flex-col gap-1 text-11px cursor-pointer file-card {style}",
            "data-selected": selected.then(|| "true".to_string()),
            onclick: move |_| {
                selected_path.set(Some(std::path::PathBuf::from(path.clone())));
            },
            div { class: "flex items-center gap-2",
                span { class: "font-medium truncate", "{filename}" }
                if let Some(c) = category {
                    span { class: "px-1 rounded file-chip file-chip-cat text-8px",
                        "{c}"
                    }
                }
                if let Some(score_txt) = similarity_text {
                    span { class: "px-1 rounded similarity-badge text-8px", "{score_txt}" }
                }
            }
            if let Some(d) = desc {
                p { class: "line-clamp-2", "{d}" }
            }
            if !tags.is_empty() {
                div { class: "flex flex-wrap gap-1",
                    for t in tags.iter().take(8) {
                        span { class: "px-1 rounded file-chip file-chip-tag text-8px",
                            "{t}"
                        }
                    }
                }
            }
        }
    }
}


pub fn detail_row(
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
    // (Removed file_records_ctx usage; rely on cached metadata only.)
    let selected = selected_path.read().as_ref().map(|p| p == &item.path).unwrap_or(false);
    let multi_selected_state = selected_paths.read().contains(&item.path);
    // Unified row style: base .file-row with selection state via is-selected class + data attribute.
    let row_style = if multi_selected_state || selected { "is-selected" } else { "" };
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
                    class: "detail-row file-row grid items-center gap-2 rounded-md border px-2 h-[50px] cursor-pointer text-11px select-none {row_style}",
                    "data-selected": (multi_selected_state || selected).then(|| "true".to_string()),
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
                    div { class: "thumb flex items-center justify-center rounded file-card-thumb",
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
                            span { class: "px-1 rounded file-chip file-chip-cat text-8px",
                                "{cat}"
                            }
                        }
                        if let Some(ref desc) = desc_render {
                            span {
                                class: "px-1 rounded similarity-badge text-8px truncate",
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
