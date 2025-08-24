use dioxus::prelude::*;
use humansize::{format_size, DECIMAL}; // format_size used below for size display
use std::path::PathBuf;

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

    let style = if *preview_collapsed.read() {
        "width:0; overflow:hidden; transition: width .08s ease; position: relative; padding:0; border:none;".to_string()
    } else {
        format!("width: {}px; overflow:hidden; transition: width .08s ease; position: relative;", *preview_width.read())
    };

    rsx! { aside { class: "bg-panel border-l border-stroke", style: "{style}",
        if !*preview_collapsed.read() {
            div { class: "h-full flex flex-col",
                // Header
                div { class: "flex items-center justify-between px-3 py-2 border-b border-stroke",
                    h3 { class: "text-lg font-semibold", "Preview" }
                    button { class: "btn", title: "Close preview", onclick: move |_| {
                        preview_collapsed.set(true); let mut s = ui.write(); s.preview_collapsed = true; crate::settings::save_settings(&s);
                    }, i { class: "material-icons", "close" } }
                }
                // Content
                div { class: "flex-1 p-4 overflow-y-auto",
                    if let Some(selected) = selected_path.read().clone() {
                        div { class: "space-y-4",
                            h4 { class: "text-lg font-medium truncate", title: "{selected.file_name().and_then(|f| f.to_str()).unwrap_or(\"\")}", {selected.file_name().and_then(|f| f.to_str()).unwrap_or("")} }
                            p { class: "text-sm text-weak break-all", "{selected.display()}" }
                            div { class: "flex justify-center py-4",
                                if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                    if let Some(thumb) = &item.thumb_data { img { class: "max-w-full max-h-48 rounded-lg border border-stroke", src: "{thumb}", alt: "Preview" } }
                                    else { i { class: "material-icons text-6xl text-weak", "{item.icon_name()}" } }
                                } else if *ai_search_active.read() {
                                    if let Some(ai_item) = ai_search_results.read().iter().find(|f| f.path == selected.display().to_string()) {
                                        if let Some(t) = ai_item.thumb_b64.clone().or(ai_item.thumbnail_path.clone()) { img { class: "max-w-full max-h-48 rounded-lg border border-stroke", src: "{t}", alt: "Preview" } }
                                        else { i { class: "material-icons text-6xl text-weak", { match ai_item.file_type.as_str() { "image" => "photo", "video" => "smart_display", _ => "insert_drive_file" } } } }
                                    } else { i { class: "material-icons text-6xl text-weak", "insert_drive_file" } }
                                } else { i { class: "material-icons text-6xl text-weak", "insert_drive_file" } }
                            }
                            // Metadata
                            if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                div { class: "space-y-2 text-sm",
                                    if let Some(desc) = ai_descriptions.read().get(&selected.display().to_string()) { div { class: "p-2 rounded-md bg-muted border border-stroke text-11px leading-snug", span { class: "font-semibold text-accent", "AI Description:" } p { class: "mt-1", "{desc}" } } }
                                    if let Some(meta_full) = selected_ai_meta.read().as_ref() {
                                        if let Some(caption) = &meta_full.caption { div { class: "flex justify-between text-11px", span { class: "text-weak", "Caption:" } span { class: "truncate", "{caption}" } } }
                                        if let Some(cat) = &meta_full.category { if !cat.is_empty() { div { class: "flex justify-between text-11px", span { class: "text-weak", "Category:" } span { class: "truncate", "{cat}" } } } }
                                        if !meta_full.tags.is_empty() { div { class: "flex flex-wrap gap-1", for t in meta_full.tags.iter() { span { key: "tag-{t}", class: "px-2 py-0.5 bg-accent-weak text-accent rounded-full text-10px", "{t}" } } } }
                                        if let Some(h) = &meta_full.hash { div { class: "flex justify-between text-11px", span { class: "text-weak", "Hash:" } span { class: "truncate", "{h}" } } }
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
                            // Action buttons
                            div { class: "pt-4 space-y-2",
                                button { class: "w-full btn bg-accent text-white hover:bg-accent-dark", onclick: { let selected = selected.clone(); move |_| { let _ = open::that(&selected); } }, i { class: "material-icons mr-2", "open_in_new" } "Open File" }
                                button { class: "w-full btn bg-muted hover:bg-stroke", onclick: { let selected = selected.clone(); move |_| { if let Some(parent) = selected.parent() { let _ = open::that(parent); } } }, i { class: "material-icons mr-2", "folder_open" } "Show in Folder" }
                            }
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
