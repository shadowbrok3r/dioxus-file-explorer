use dioxus::prelude::*;
use humansize::{format_size, DECIMAL}; // format_size used below for size display
use std::path::PathBuf;
use crate::types::{IMAGE_EXTS, VIDEO_EXTS};

#[derive(Props, PartialEq, Clone)]
pub struct PreviewPaneProps {
    pub ui: Signal<crate::settings::UiSettings>,
    pub preview_collapsed: Signal<bool>,
    pub preview_width: Signal<u32>,
    pub resizing_preview: Signal<Option<(i32,u32)>>,
    pub selected_path: Signal<Option<PathBuf>>,
    pub results: Signal<crate::types::ScanResults>,
    pub ai_search_active: Signal<bool>,
    pub ai_search_results: Signal<Vec<crate::ai::FileMetadata>>,
    pub ai_descriptions: Signal<std::collections::HashMap<String,String>>,
    pub selected_ai_meta: Signal<Option<crate::ai::FileMetadata>>,
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub ai_model_ready: Signal<bool>,
    pub ai_generating: Signal<bool>,
}

#[allow(non_snake_case)]
pub fn PreviewPane(props: PreviewPaneProps) -> Element {
    let PreviewPaneProps { mut ui, mut preview_collapsed, preview_width, mut resizing_preview, selected_path, results, ai_search_active, ai_search_results, ai_descriptions, selected_ai_meta, ai_search_engine, ai_model_ready, ai_generating: _ } = props;
    // Local UI toggle state
    let mut show_tags = use_signal(|| false);
    let mut show_full_desc = use_signal(|| false);

    let style = if *preview_collapsed.read() {
        "width:0; overflow:hidden; transition: width .08s ease; position: relative; padding:0; border:none;".to_string()
    } else {
        format!("width: {}px; overflow:hidden; transition: width .08s ease; position: relative;", *preview_width.read())
    };

    // Effect: on-demand thumbnail generation when a file is selected and has no thumbnail yet.
    // Reads selected_path and results; will re-run when either changes (signal tracking).
    {
        let results_for_effect = results.clone();
        let selected_signal = selected_path.clone();
        use_effect(move || {
            let selected = selected_signal.read().clone();
            if let Some(sel_path) = selected {
                // Determine if we need a thumbnail
                let need_thumb = {
                    let r = results_for_effect.read();
                    r.items.iter().find(|f| f.path == sel_path).map(|f| f.thumb_data.is_none()).unwrap_or(false)
                };
                if need_thumb {
                    let path_clone = sel_path.clone();
                    let mut results_sig = results_for_effect.clone();
                    spawn(async move {
                        let ext = path_clone.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
                        if let Some(ext) = ext {
                            let is_img = IMAGE_EXTS.iter().any(|e| *e == ext);
                            let is_vid = VIDEO_EXTS.iter().any(|e| *e == ext);
                            if is_img || is_vid {
                                let thumb_res = if is_img {
                                    crate::thumbs::generate_image_thumb_data(&path_clone).ok()
                                } else {
                                    #[cfg(windows)]
                                    { crate::thumbs::generate_video_thumb_data(&path_clone).ok() }
                                    #[cfg(not(windows))]
                                    { None }
                                };
                                if let Some(t) = thumb_res {
                                    let mut write = results_sig.write();
                                    if let Some(found) = write.items.iter_mut().find(|f| f.path == path_clone) { found.thumb_data = Some(t); }
                                }
                            }
                        }
                    });
                }
            }
        });
    }

    rsx! { aside { class: "bg-panel border-l border-stroke", style: "{style}",
        if !*preview_collapsed.read() {
            // Make entire interior scroll except fixed header
            div { class: "h-full flex flex-col",
                // Header
                div { class: "flex items-center justify-between px-3 py-2 border-b border-stroke",
                    h3 { class: "text-lg font-semibold", "Preview" }
                    button { class: "btn", title: "Close preview", onclick: move |_| {
                        preview_collapsed.set(true); let mut s = ui.write(); s.preview_collapsed = true; crate::settings::save_settings(&s);
                    }, i { class: "material-icons", "close" } }
                }
                // Content
                div { class: "flex-1 p-4 overflow-y-auto", style: "min-height:0;", // min-height:0 ensures child flex scroll
                    if let Some(selected) = selected_path.read().clone() {
                        div { class: "space-y-4",
                            h4 { class: "text-lg font-medium truncate", title: "{selected.file_name().and_then(|f| f.to_str()).unwrap_or(\"\")}", {selected.file_name().and_then(|f| f.to_str()).unwrap_or("")} }
                            p { class: "text-sm text-weak break-all", "{selected.display()}" }
                            div { class: "flex flex-col items-center gap-4 py-4",
                                if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                    if let Some(thumb) = &item.thumb_data { img { class: "max-w-full max-h-48 rounded-lg border border-stroke", style: "display:block;", src: "{thumb}", alt: "Preview" } }
                                    else { i { class: "material-icons text-6xl text-weak", "{item.icon_name()}" } }
                                } else if *ai_search_active.read() {
                                    if let Some(ai_item) = ai_search_results.read().iter().find(|f| f.path == selected.display().to_string()) {
                                        if let Some(t) = ai_item.thumb_b64.clone().or(ai_item.thumbnail_path.clone()) { img { class: "max-w-full max-h-48 rounded-lg border border-stroke", style: "display:block;", src: "{t}", alt: "Preview" } }
                                        else { i { class: "material-icons text-6xl text-weak", { match ai_item.file_type.as_str() { "image" => "photo", "video" => "smart_display", _ => "insert_drive_file" } } } }
                                    } else { i { class: "material-icons text-6xl text-weak", "insert_drive_file" } }
                                } else { i { class: "material-icons text-6xl text-weak", "insert_drive_file" } }
                                // Action buttons moved directly under thumbnail
                                div { class: "w-full flex flex-col gap-2 pt-2",
                                    button { class: "w-full btn bg-accent text-white hover:bg-accent-dark", onclick: { let selected = selected.clone(); move |_| { let _ = open::that(&selected); } }, i { class: "material-icons mr-2", "open_in_new" } "Open File" }
                                    button { class: "w-full btn bg-muted hover:bg-stroke", onclick: { let selected = selected.clone(); move |_| { if let Some(parent) = selected.parent() { let _ = open::that(parent); } } }, i { class: "material-icons mr-2", "folder_open" } "Show in Folder" }
                                }
                            }
                            // Metadata
                            if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                div { class: "space-y-2 text-sm",
                                    if let Some(desc) = ai_descriptions.read().get(&selected.display().to_string()) {
                                        {   // scoped rust
                                            let long = desc.len() > 220;
                                            let expanded = *show_full_desc.read();
                                            let display_txt = if !expanded && long { format!("{}…", desc.chars().take(220).collect::<String>()) } else { desc.clone() };
                                            rsx!{
                                                div { class: "p-2 rounded-md bg-muted border border-stroke text-11px leading-snug flex flex-col gap-1",
                                                    span { class: "font-semibold text-accent", "AI Description:" }
                                                    p { class: "mt-1 whitespace-pre-wrap", "{display_txt}" }
                                                    if long { button { class: "self-start text-10px px-2 py-0.5 rounded bg-panel border border-stroke hover:border-accent transition", onclick: move |_| { let new_val = !*show_full_desc.read(); show_full_desc.set(new_val); }, if expanded { "Show less" } else { "Show more" } } }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(meta_full) = selected_ai_meta.read().as_ref() {
                                        if let Some(caption) = &meta_full.caption { div { class: "text-11px", span { class: "text-weak", "Caption:" } p { class: "mt-0.5 truncate", "{caption}" } } }
                                        if let Some(h) = &meta_full.hash { 
                                            { let short = if h.len() > 5 { format!("…{}", &h[h.len()-5..]) } else { h.clone() }; rsx!{ div { class: "flex justify-between text-11px", span { class: "text-weak", "Hash:" } span { class: "font-mono", "{short}" } } } }
                                        }
                                        if let Some(segs) = &meta_full.segments {
                                            if !segs.is_empty() {
                                                {
                                                    let joined = segs.join(", ");
                                                    rsx! {
                                                        div { class: "text-11px",
                                                            span { class: "text-weak", "Segments:" }
                                                            p { class: "mt-1 break-all", "{joined}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(objs) = &meta_full.segment_objects {
                                            if !objs.is_empty() {
                                                div { class: "text-11px space-y-1",
                                                    span { class: "text-weak", "Objects:" }
                                                    for o in objs.iter().take(12) {
                                                        {
                                                            let pct = format!("{:.0}%", o.confidence * 100.0);
                                                            rsx! {
                                                                div { key: "obj-{o.label}-{o.confidence}", class: "flex gap-2",
                                                                    span { class: "px-1.5 py-0.5 bg-muted rounded text-10px", "{o.label}" }
                                                                    span { class: "text-10px text-weak", "{pct}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(cnts) = &meta_full.object_counts { if !cnts.is_empty() { div { class: "text-11px", span { class: "text-weak", "Counts:" } div { class: "flex flex-wrap gap-1 mt-1", for (k,v) in cnts.iter() { span { key: "cnt-{k}", class: "px-1 py-0.5 bg-muted rounded text-10px", "{k}:{v}" } } } } } }
                                        if let Some(embed) = &meta_full.embedding { div { class: "flex justify-between text-11px", span { class: "text-weak", "Embedding dims:" } span { "{embed.len()}" } } }
                                        // Category & Tags moved to end with toggle
                                        if meta_full.category.as_ref().map(|c| !c.is_empty()).unwrap_or(false) || !meta_full.tags.is_empty() {
                                            {
                                                let tag_count = meta_full.tags.len();
                                                let expanded = *show_tags.read();
                                                rsx!{
                                                    div { class: "text-11px mt-2 border-t border-stroke pt-2 flex flex-col gap-1",
                                                        button { class: "self-start text-10px px-2 py-0.5 rounded bg-panel border border-stroke hover:border-accent transition", onclick: move |_| { let new_val = !*show_tags.read(); show_tags.set(new_val); },
                                                            if expanded { "Hide tags" } else { "Show tags" }
                                                            if tag_count > 0 { span { class: "ml-1 text-weak", "({tag_count})" } }
                                                        }
                                                        if expanded {
                                                            if let Some(cat) = &meta_full.category { if !cat.is_empty() { span { class: "font-semibold", "{cat}" } } }
                                                            ul { class: "list-disc list-inside space-y-0.5 max-h-40 overflow-y-auto pr-1", for t in meta_full.tags.iter() { li { key: "tag-{t}", "{t}" } } }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if ai_descriptions.read().get(&selected.display().to_string()).is_none() && ai_search_engine.read().is_some() && *ai_model_ready.read() {
                                        { let path_for_gen = selected.display().to_string(); rsx!{
                                            div { class: "p-2 rounded-md bg-muted border border-stroke text-11px leading-snug flex flex-col gap-2",
                                                span { class: "font-semibold text-accent", "AI Description:" }
                                                span { class: "text-weak", "No description yet." }
                                                button { class: "btn text-10px w-min", onclick: move |_| {
                                                    if let Some(engine) = ai_search_engine.read().clone() {
                                                        let path_target = path_for_gen.clone(); let mut desc_map2 = ai_descriptions.clone();
                                                        spawn(async move { if let Ok(Some(desc)) = engine.generate_description_for_path(&path_target, true).await { desc_map2.write().insert(path_target.clone(), desc); } });
                                                    }
                                                }, i { class: "material-icons text-sm", "bolt" } span { " Generate" } }
                                            }
                                        }}
                                    }
                                    if let Some(item_size) = item.size { div { class: "flex justify-between", span { class: "text-weak", "Size:" } span { "{format_size(item_size, DECIMAL)}" } } }
                                    if let Some(modified) = item.modified {
                                        {
                                            let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();
                                            rsx! { div { class: "flex justify-between", span { class: "text-weak", "Modified:" } span { "{modified_str}" } } }
                                        }
                                    }
                                    if let Some(created) = item.created {
                                        {
                                            let created_str = created.format("%Y-%m-%d %H:%M").to_string();
                                            rsx! { div { class: "flex justify-between", span { class: "text-weak", "Created:" } span { "{created_str}" } } }
                                        }
                                    }
                                    if let Some(ext) = selected.extension() {
                                        { let ext_str = ext.to_str().unwrap_or("").to_string(); rsx!{ div { class: "flex justify-between", span { class: "text-weak", "Type:" } span { "{ext_str}" } } } }
                                    }
                                }
                            } else if *ai_search_active.read() {
                                if let Some(ai_item) = ai_search_results.read().iter().find(|f| f.path == selected.display().to_string()) {
                                    div { class: "space-y-2 text-sm",
                                        if let Some(desc) = ai_item.description.clone() { div { class: "p-2 rounded-md bg-muted border border-stroke text-11px leading-snug", span { class: "font-semibold text-accent", "AI Description:" } p { class: "mt-1", "{desc}" } } }
                                        div { class: "flex justify-between", span { class: "text-weak", "Type:" } span { "{ai_item.file_type}" } }
                                        div { class: "flex justify-between", span { class: "text-weak", "Path:" } span { class: "break-all", "{selected.display()}" } }
                                    }
                                }
                            }
                            // (Buttons moved under thumbnail above)
                        }
                    } else {
                        div { class: "text-center py-8 text-weak", i { class: "material-icons text-4xl mb-2 opacity-50", "preview" } p { "Select a file to preview" } }
                    }
                }
            }
        }
        if !*preview_collapsed.read() { div { class: "resize-handle", style: "position:absolute; top:0; left:-3px; width:6px; height:100%; cursor: ew-resize;", onmousedown: move |evt| { resizing_preview.set(Some((evt.client_coordinates().x as i32, *preview_width.read()))); } } }
    }}
}
