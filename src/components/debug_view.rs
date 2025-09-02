use dioxus::prelude::*;
use dioxus_primitives::separator::Separator;

#[derive(Props, PartialEq, Clone)]
pub struct DebugViewProps {
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub ai_descriptions: Signal<std::collections::HashMap<String,String>>,
    pub debug_thumb_rows: Signal<Vec<crate::Thumbnail>>,
    pub debug_doc_snips: Signal<Vec<crate::DebugDocumentSnippet>>,
    pub debug_loaded_at: Signal<Option<std::time::Instant>>,
    pub selected_path: Signal<Option<std::path::PathBuf>>,
}

#[allow(non_snake_case)]
#[component]
pub fn DebugView(props: DebugViewProps) -> Element {
    let engine = props.ai_search_engine;
    let thumbs = props.debug_thumb_rows;
    let docs = props.debug_doc_snips;
    let loaded_at = props.debug_loaded_at;
    let mut selected_path = props.selected_path;
    let ai_descriptions = props.ai_descriptions; // might display aggregate counts in future
    let missing_desc_count = use_signal(|| None::<usize>);
    let bulk_op_in_progress = use_signal(|| false);

    rsx! {
        div {
            class: "p-10 m-10 overflow-y-auto",
            // style: "height: calc(100vh - 56px);",
            div {
                class: "flex",
                h1 { 
                    class: "flex items-center gap-3", 
                    style: "margin: 10px",
                    i { class: "material-icons text-fuchsia-500", "storage" }
                    "Database Debug View"
                }
                span { class: "bg-muted rounded border border-stroke", style: "padding: 5px 20px; margin: 10px 10px",
                    "Thumb rows: {thumbs.read().len()}"
                }
                span { class: "bg-muted rounded border border-stroke", style: "padding: 5px 20px; margin: 10px 10px",
                    "In-memory files: {engine.read().as_ref().map(|_| ai_descriptions.read().len()).unwrap_or(0)}"
                }
                span { class: "bg-muted rounded border border-stroke", style: "padding: 5px 20px; margin: 10px 10px",
                    "Docs: {docs.read().len()}"
                }
                if let Some(ts) = loaded_at.read().as_ref() {
                    span { class: "bg-muted rounded border border-stroke", style: "padding: 5px 20px; margin: 10px 10px",
                        "Loaded {ts.elapsed().as_secs()}s ago"
                    }
                }
                
                button {
                    class: "btn",
                    style: "padding: 5px 20px; margin: 10px 10px",
                    "data-style": "outline",
                    onclick: move |_| {
                        if let Some(engine_inst) = engine.read().as_ref() {
                            let engine_clone = engine_inst.clone();
                            let mut thumb_sig = thumbs.clone();
                            let _doc_sig = docs.clone();
                            let mut ts_sig = loaded_at.clone();
                            spawn(async move {
                                let t = engine_clone.list_thumbnail_rows(1000).await;
                                #[cfg(feature = "surreal")]
                                {
                                    let d = engine_clone.list_document_snippets(500).await;
                                    _doc_sig.set(d);
                                }
                                thumb_sig.set(t);
                                ts_sig.set(Some(std::time::Instant::now()));
                            });
                        }
                    },
                    i { class: "material-icons mr-1", "refresh" }
                    "Refresh"
                }
                // Count missing descriptions
                button {
                    class: "btn",
                    style: "padding: 0px 20px; margin: 10px 10px",
                    "data-style": "outline",
                    disabled: *bulk_op_in_progress.read() || engine.read().is_none(),
                    onclick: move |_| {
                        if let Some(engine_inst) = engine.read().as_ref() {
                            let engine_clone = engine_inst.clone();
                            let mut cnt_sig = missing_desc_count.clone();
                            spawn(async move {
                                cnt_sig.set(Some(engine_clone.count_missing_descriptions().await));
                            });
                        }
                    },
                    i { class: "material-icons mr-1", "find_in_page" }
                    "Count Missing"
                }
                if let Some(c) = *missing_desc_count.read() {
                    span { class: "px-2 py-1 bg-muted rounded border border-stroke",
                        "Missing: {c}"
                    }
                }
                // Enrich missing descriptions
                button {
                    class: "btn",
                    style: "padding: 0px 20px; margin: 10px 10px",
                    "data-style": "outline",
                    disabled: *bulk_op_in_progress.read() || engine.read().is_none(),
                    onclick: move |_| {
                        if let Some(engine_inst) = engine.read().as_ref() {
                            if *bulk_op_in_progress.read() {
                                return;
                            }
                            let engine_clone = engine_inst.clone();
                            let mut cnt_sig = missing_desc_count.clone();
                            let mut in_prog_sig = bulk_op_in_progress.clone();
                            in_prog_sig.set(true);
                            spawn(async move {
                                let generated = engine_clone.enrich_missing_descriptions().await;
                                cnt_sig.set(Some(engine_clone.count_missing_descriptions().await));
                                log::info!("[UI] Enriched {generated} descriptions");
                                in_prog_sig.set(false);
                            });
                        }
                    },
                    i { class: "material-icons pr-1",
                        {if *bulk_op_in_progress.read() { "hourglass_top" } else { "auto_fix_high" }}
                    }
                    "Enrich Missing"
                }
                // Generate semantic embeddings recursively
                button {
                    class: "btn",
                    style: "padding: 0px 20px; margin: 10px 10px",
                    "data-style": "outline",
                    disabled: *bulk_op_in_progress.read() || engine.read().is_none(),
                    onclick: move |_| {
                        #[cfg(feature = "surreal")]
                        {
                            if let Some(engine_inst) = engine.read().as_ref() {
                                if *bulk_op_in_progress.read() {
                                    return;
                                }
                                let mut in_prog_sig = bulk_op_in_progress.clone();
                                in_prog_sig.set(true);
                                let engine_clone = engine_inst.clone();
                                spawn(async move {
                                    let added = engine_clone.generate_semantic_recursive().await;
                                    log::info!("[UI] Semantic embeddings added {added}");
                                    in_prog_sig.set(false);
                                });
                            }
                        }
                    },
                    i { class: "material-icons mr-1",
                        {if *bulk_op_in_progress.read() { "hourglass_top" } else { "schema" }}
                    }
                    "Semantic All"
                }
            
            }
            Separator { class: "separator", horizontal: true }

            // Thumbnails table
            h2 { class: "text-lg font-medium flex justify-center", style: "margin: 15px 5px;", "Cached Thumbnails & AI Metadata" }
            if thumbs.read().is_empty() {
                p { class: "text-weak text-sm", "No rows." }
            } else {
                {
                    let rows = thumbs.read().clone();
                    rsx! {
                        div {
                            class: "overflow-auto border border-stroke rounded-md",
                            style: "max-height:50%; overflow-y:auto;",
                            table { class: "min-w-full text-11px table-fixed table",
                                thead {
                                    tr { class: "text-left",
                                        th { "Path" }
                                        th { "Type" }
                                        th { "Size" }
                                        th { "Desc?" }
                                        th { "Caption" }
                                        th { "Tags" }
                                        th { "Hash" }
                                    }
                                }
                                Separator { class: "separator", horizontal: true }
                                tbody {
                                    for row in rows.iter() {
                                        {
                                            let row_clone = row.clone();
                                            let path_for_click = row_clone.path.clone();
                                            rsx! {
                                                tr {
                                                    class: "border-b border-stroke hover:bg-accent-weak/40 cursor-pointer",
                                                    onclick: move |_| {
                                                        selected_path.set(Some(std::path::PathBuf::from(path_for_click.clone())));
                                                    },
                                                    td { class: "pr-2 py-1 max-w-[240px] truncate", title: "{row_clone.path}",
                                                        "{row_clone.filename}"
                                                    }
                                                    td { "{row_clone.file_type}" }
                                                    td { "{row_clone.size}" }
                                                    td {
                                                        if row_clone.description.as_ref().map(|d| d.len()).unwrap_or(0) > 0 {
                                                            "Y"
                                                        } else {
                                                            ""
                                                        }
                                                    }
                                                    td {
                                                        class: "truncate max-w-[140px]",
                                                        title: row_clone.caption.clone().unwrap_or_default(),
                                                        {row_clone.caption.clone().unwrap_or_default()}
                                                    }
                                                    td { class: "truncate max-w-[160px]", title: row_clone.tags.join(", "), "{row_clone.tags.len()}" }
                                                    td { class: "truncate max-w-[140px]",
                                                        {row_clone.hash.clone().unwrap_or_default().chars().take(12).collect::<String>()}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Documents list
            h3 { class: "text-lg font-medium mb-2", "Semantic Documents" }
            if docs.read().is_empty() {
                p { class: "text-weak text-sm", "No documents (index not built)." }
            } else {
                ul { class: "space-y-2 text-11px",
                    for d in docs.read().iter() {
                        {
                            let d_clone = d.clone();
                            rsx! {
                                li { class: "p-2 rounded border border-stroke bg-panel hover:border-accent transition",
                                    {
                                        let title = d_clone.title.clone().unwrap_or_else(|| "(no title)".into());
                                        rsx! {
                                            div { class: "flex justify-between text-10px text-weak mb-1",
                                                span { "{title}" }
                                                span { "{d_clone.len} chars" }
                                            }
                                        }
                                    }
                                    pre { class: "whitespace-pre-wrap break-all text-[10px] leading-snug", "{d_clone.preview}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
