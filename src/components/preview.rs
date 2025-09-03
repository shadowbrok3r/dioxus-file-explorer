use dioxus::prelude::*;
use dioxus_primitives::separator::Separator;
use humansize::{format_size, DECIMAL};
use std::path::{PathBuf, Path};
use crate::{ai::generate::VisionDescription, utilities::types::{IMAGE_EXTS, VIDEO_EXTS}};

// Attempt to extract a "description" value progressively from a (possibly partial) JSON stream.
// Strategy:
// 1. Find the earliest occurrence of the key pattern "description" (allowing optional quotes & whitespace).
// 2. After the colon, detect whether the value starts with a quote. If so, collect characters until an unescaped closing quote.
// 3. If JSON object fully extracted (balanced braces + closing quote), we still only return the description substring.
// 4. Return Some(partial_or_full_description) or None if not even the key has appeared.
fn extract_partial_description(stream: &str) -> Option<String> {
    // Fast path: look for key token
    let lower = stream.to_ascii_lowercase();
    let key_pos = lower.find("\"description\"")?; // require quoted form to reduce false positives
    let after_key = &stream[key_pos + "\"description\"".len()..];
    // Skip whitespace and optional colon
    let mut chars = after_key.chars().peekable();
    // Skip whitespace
    while let Some(c) = chars.peek() { if c.is_whitespace() { chars.next(); } else { break; } }
    if chars.peek().copied() != Some(':') { return None; }
    chars.next(); // consume ':'
    // Skip whitespace after colon
    while let Some(c) = chars.peek() { if c.is_whitespace() { chars.next(); } else { break; } }
    // Expect opening quote for string value
    if chars.peek().copied() != Some('"') { return None; }
    chars.next(); // consume opening quote
    let mut desc = String::new();
    let mut escaped = false;
    for c in chars {
        if escaped { desc.push(c); escaped = false; continue; }
        if c == '\\' { escaped = true; continue; }
        if c == '"' { // closing quote -> full description captured
            return Some(desc);
        }
        desc.push(c);
    }
    // If we reach here, string not closed yet -> return partial if non-empty
    if desc.is_empty() { None } else { Some(desc) }
}

// Helper structure for memoized AI description display state
#[derive(PartialEq, Clone, Debug)]
struct AiDisplayState {
    display: String,      // Text to show (possibly truncated)
    long: bool,           // Whether original text exceeded truncation threshold
    streaming: bool,      // Currently streaming (interim)
    has_text: bool,       // Any text present
}

#[derive(Props, PartialEq, Clone)]
pub struct PreviewPaneProps {
    pub ui: Signal<crate::settings::UiSettings>,
    pub preview_collapsed: Signal<bool>,
    pub preview_width: Signal<u32>,
    pub resizing_preview: Signal<Option<(i32,u32)>>,
    pub selected_path: Signal<Option<PathBuf>>,
    pub results: Signal<crate::utilities::types::ScanResults>,
    pub ai_search_active: Signal<bool>,
    pub ai_search_results: Signal<Vec<crate::database::FileMetadata>>,
    pub ai_descriptions: Signal<std::collections::HashMap<String,String>>,
    pub selected_ai_meta: Signal<Option<crate::FileMetadata>>,
    pub ai_search_engine: Signal<Option<crate::ai::AISearchEngine>>,
    pub ai_model_ready: Signal<bool>,
    pub ai_generating: Signal<bool>,
}

#[allow(non_snake_case)]
#[component]
pub fn PreviewPane(props: PreviewPaneProps) -> Element {
    let PreviewPaneProps { mut ui, mut preview_collapsed, mut preview_width, mut resizing_preview, selected_path, results, ai_search_active, ai_search_results, ai_descriptions, selected_ai_meta, ai_search_engine, ai_model_ready, ai_generating: _ } = props;
    // Local UI toggle state
    let mut show_tags = use_signal(|| false);
    let mut show_full_desc = use_signal(|| false);
    // Streaming interim description (separate from persisted ai_descriptions map so we can show partial tokens immediately
    // without polluting final stored text until completion). Keyed by path for safety if selection changes mid-stream.
    let streaming_interim = use_signal(|| std::collections::HashMap::<String,String>::new());

    // Memo: currently selected path key string (prevents recomputing to_string chains)
    let selected_key = {
        let selected_sig = selected_path.clone();
        use_memo(move || selected_sig.read().as_ref().map(|p| p.display().to_string()))
    };

    // Memo: AI description display logic (truncation + streaming indicator) derived from signals
    let ai_display_memo = {
        let ai_desc_sig = ai_descriptions.clone();
        let interim_sig = streaming_interim.clone();
        let show_full_sig = show_full_desc.clone();
        let selected_key_m = selected_key.clone();
        use_memo(move || {
            if let Some(key) = selected_key_m.read().clone() {
                let interim_opt = interim_sig.read().get(&key).cloned();
                let stored_opt = ai_desc_sig.read().get(&key).cloned();
                let expanded = *show_full_sig.read();
                let source_opt = interim_opt.clone().or(stored_opt.clone());
                if let Some(full) = source_opt {
                    let long = full.len() > 250;
                    let display = if !expanded && long { format!("{}…", full.chars().take(250).collect::<String>()) } else { full.clone() };
                    return Some(AiDisplayState { display, long, streaming: interim_opt.is_some(), has_text: true });
                }
            }
            None
        })
    };

    // Helper (fallback) relative to process cwd
    let compute_relative = |p: &Path| -> String {
        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(rel) = p.strip_prefix(&cwd) {
                return rel.display().to_string();
            }
        }
        p.display().to_string()
    };

    // Derive a common root of all scanned items (so we can show paths relative to scan root)
    let common_root: Option<PathBuf> = {
        let r = results.read();
        if r.items.is_empty() {
            None
        } else {
            let mut comps: Vec<_> = r.items[0].path.components().collect();
            for item in r.items.iter().skip(1) {
                let mut keep = 0;
                for (a, b) in comps.iter().zip(item.path.components()) {
                    if a == &b { keep += 1 } else { break; }
                }
                comps.truncate(keep);
                if comps.is_empty() { break; }
            }
            if comps.is_empty() {
                None
            } else {
                let mut p = PathBuf::new();
                for c in comps {
                    p.push(c.as_os_str());
                }
                Some(p)
            }
        }
    };

    let style = if *preview_collapsed.read() {
        "width:0; overflow:hidden; transition: width .08s ease; position: relative; padding:0; border:none;".to_string()
    } else {
        format!("width: {}px; overflow:auto; transition: width .08s ease; position: relative; border-radius:16px", *preview_width.read())
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
                        log::info!("Getting thumbnail");
                        let ext = path_clone.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
                        if let Some(ext) = ext {
                            let is_img = IMAGE_EXTS.iter().any(|e| *e == ext);
                            let is_vid = VIDEO_EXTS.iter().any(|e| *e == ext);
                            if is_img || is_vid {
                                let path_for_block = path_clone.clone();
                                let thumb_res = tokio::spawn(async move {
                                    if is_img {
                                        if let Ok(data) = crate::utilities::thumbs::generate_image_thumb_data(&path_for_block) {
                                            return Some(data);
                                        }
                                    } else if is_vid {
                                        if let Ok(data) = crate::utilities::thumbs::generate_video_thumb_data(&path_for_block) {
                                            return Some(data);
                                        }
                                    }
                                    None
                                }).await;

                                if let Ok(thumb) = thumb_res {
                                    let mut write = results_sig.write();
                                    if let Some(found) = write.items.iter_mut().find(|f| f.path == path_clone) { found.thumb_data = thumb; }
                                    log::info!("Found file");
                                }
                            }
                        }
                    });
                }
            }
        });
    }

    rsx! {
        aside {
            // Converted to semantic panel container with outline style
            class: "panel p-2",
            "data-style": "outline",
            style: "{style}",
            onmouseleave: move |_| {
                // Safety: if mouse leaves while resizing keep logic consistent
            },
            // Added global-ish mouse handlers on the aside to manage resize end
            onpointermove: move |evt| {
                if let Some((start_x, start_w)) = *resizing_preview.read() {
                    let dx = evt.client_coordinates().x as i32 - start_x;
                    let mut new_w = (start_w as i32 + dx).max(160).min(1600) as u32;
                    if new_w < 160 { new_w = 160; }
                    preview_width.set(new_w);
                }
            },
            onpointerup: move |_| {
                if resizing_preview.read().is_some() {
                    resizing_preview.set(None);
                    let mut s = ui.write();
                    s.preview_width = *preview_width.read();
                    crate::settings::save_settings(&s);
                }
            },
            if !*preview_collapsed.read() {
                div { class: "h-full flex flex-col",
                    // Header (now shows current file name instead of static 'Preview')
                    div { class: "flex items-center justify-between px-3 py-2 border-b border-stroke",
                        {
                            let current_name = selected_path.read();
                            let name = current_name
                                .as_ref()
                                .and_then(|p| p.file_name().and_then(|f| f.to_str()))
                                .unwrap_or("Preview");
                            rsx! {
                                h4 { class: "text-md font-semibold truncate flex-1 ", title: "{name}", "{name}" }
                            }
                        }
                        button {
                            class: "button",
                            "data-style": "outline",
                            style: "max-height: 25px; max-width: 10px padding: 0, margin: 0",
                            title: "Close preview",
                            onclick: move |_| {
                                preview_collapsed.set(true);
                                let mut s = ui.write();
                                s.preview_collapsed = true;
                                crate::settings::save_settings(&s);
                            },
                            i { class: "material-icons", "close" }
                        }
                    }
                    // Content
                    div {
                        class: "flex-1 p-4 overflow-y-auto",
                        style: "min-height:0;",
                        if let Some(selected) = selected_path.read().clone() {
                            div { class: "space-y-4",
                                // (Removed inner h4 filename – now in header)
                                {
                                    let rel = if let Some(root) = &common_root {
                                        if selected.starts_with(root) {
                                            selected.strip_prefix(root).unwrap().display().to_string()
                                        } else {
                                            compute_relative(&selected)
                                        }
                                    } else {
                                        compute_relative(&selected)
                                    };
                                    let show_path = rel.contains('/') || rel.contains('\\');
                                    rsx! {
                                        if show_path {
                                            p { class: "text-sm text-weak break-all", title: "{selected.display()}", "{rel}" }
                                        }
                                    }
                                }
                                // Preview thumbnail area converted to panel glass surface
                                div { class: "panel flex flex-col items-center gap-4 py-4",
                                    "data-style": "glass",
                                    if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                        if let Some(thumb) = &item.thumb_data {
                                            img {
                                                class: "max-w-full max-h-48 rounded-lg border border-stroke",
                                                style: "display:block;",
                                                src: "{thumb}",
                                                alt: "Preview",
                                            }
                                        } else {
                                            i { class: "material-icons text-6xl text-weak",
                                                "{item.icon_name()}"
                                            }
                                        }
                                    } else if *ai_search_active.read() {
                                        if let Some(ai_item) = ai_search_results
                                            .read()
                                            .iter()
                                            .find(|f| f.path == selected.display().to_string())
                                        {
                                            if let Some(t) = ai_item.thumb_b64.clone().or(ai_item.thumbnail_path.clone()) {
                                                img {
                                                    class: "object-cover w-full h-full max-w-[48px] max-h-[48px]  rounded-lg border border-stroke",
                                                    style: "display:block;",
                                                    src: "{t}",
                                                    alt: "Preview",
                                                }
                                            } else {
                                                i { class: "material-icons text-6xl text-weak",
                                                    {
                                                        match ai_item.file_type.as_str() {
                                                            "image" => "photo",
                                                            "video" => "smart_display",
                                                            _ => "insert_drive_file",
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            i { class: "material-icons text-6xl text-weak",
                                                "insert_drive_file"
                                            }
                                        }
                                    } else {
                                        i { class: "material-icons text-6xl text-weak",
                                            "insert_drive_file"
                                        }
                                    }
                                    // Action buttons moved directly under thumbnail
                                    div { class: "w-full flex items-center justify-between gap-2 py-1",
                                        button {
                                            class: "button",
                                            "data-style": "outline",
                                            onclick: {
                                                let selected = selected.clone();
                                                move |_| {
                                                    let _ = open::that(&selected);
                                                }
                                            },
                                            i { class: "material-icons mr-2", "open_in_new" }
                                            "Open File"
                                        }
                                        button {
                                            class: "button",
                                            "data-style": "outline",
                                            onclick: {
                                                let selected = selected.clone();
                                                move |_| {
                                                    if let Some(parent) = selected.parent() {
                                                        let _ = open::that(parent);
                                                    }
                                                }
                                            },
                                            i { class: "material-icons mr-2", "folder_open" }
                                            "Show in Folder"
                                        }
                                    }
                                }
                                // Metadata
                                if let Some(item) = results.read().items.iter().find(|f| f.path == selected) {
                                    div { class: "space-y-2 text-sm py-2",
                                        // Compute if we already have a description (used later for separator)
                                        {
                                            let _has_desc_block = ai_descriptions
                                                .read()
                                                .get(&selected.display().to_string())
                                                .is_some()
                                                || (ai_descriptions.read().get(&selected.display().to_string()).is_none()
                                                    && ai_search_engine.read().is_some() && *ai_model_ready.read());
                                            rsx! {
                                                // Unified description section: always show regenerate buttons (even if description exists)
                                                if ai_search_engine.read().is_some() && *ai_model_ready.read() {
                                                    {
                                                        let path_for_gen = selected.display().to_string();
                                                        let path_for_semantic = path_for_gen.clone();
                                                        let existing_desc_opt = ai_descriptions.read().get(&path_for_gen).cloned();
                                                        rsx! {
                                                            div { class: "panel p-2 text-11px leading-snug flex flex-col gap-2",
                                                                "data-style": "outline",
                                                                span { class: "font-semibold text-accent", "AI Description:" }
                                                                {
                                                                    let disp_opt = ai_display_memo.read().clone();
                                                                    rsx! {
                                                                        if let Some(disp) = disp_opt {
                                                                            if disp.has_text {
                                                                                div { class: "flex flex-col gap-1",
                                                                                    p { class: if disp.streaming { "whitespace-pre-wrap text-accent" } else { "whitespace-pre-wrap" },
                                                                                        "{disp.display}"
                                                                                    }
                                                                                    if disp.long {
                                                                                        button {
                                                                                            class: "button self-start text-10px px-2 py-0.5",
                                                                                            "data-style": "outline",
                                                                                            onclick: move |_| {
                                                                                                let new_val = !*show_full_desc.read();
                                                                                                show_full_desc.set(new_val);
                                                                                            },
                                                                                            if *show_full_desc.read() {
                                                                                                "Show less"
                                                                                            } else {
                                                                                                "Show more"
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                    if disp.streaming {
                                                                                        span { class: "text-8px text-weak", "(streaming...)" }
                                                                                    }
                                                                                }
                                                                            }
                                                                        } else {
                                                                            span { class: "text-weak", "No description yet." }
                                                                        }
                                                                    }
                                                                }
                                                                div { class: "w-full flex items-center gap-2 flex-wrap justify-between",
                                                                    // Regenerate (vision) description
                                                                    button {
                                                                        class: "button text-10px w-min",
                                                                        title: "Generate/Regenerate detailed vision description",
                                                                        onclick: move |_| {
                                                                            let path_target = path_for_semantic.clone();
                                                                            let mut desc_map2 = ai_descriptions.clone();
                                                                            let engine_opt = ai_search_engine.read().clone();
                                                                            let selected_path_sig = selected_path.clone();
                                                                            let mut selected_ai_meta_sig = selected_ai_meta.clone();
                                                                            let mut streaming_interim_sig = streaming_interim.clone();
                                                                            streaming_interim_sig.write().remove(&path_target);
                                                                            desc_map2.write().insert(path_target.clone(), String::new());
                                                                            spawn(async move {
                                                                                #[cfg(feature = "joycaption")]
                                                                                {
                                                                                    if crate::ai::joycaption_adapter::is_enabled() {
                                                                                        if let Ok(bytes) = tokio::fs::read(&path_target).await {
                                                                                            let instruction = {
                                                                                                let tmpl = ui.read().ai_prompt_template.clone();
                                                                                                if tmpl.trim().is_empty() {
                                                                                                    "Analyze the supplied image and return ONLY JSON with keys: description, caption, tags (array), category."
                                                                                                        .to_string()
                                                                                                } else {
                                                                                                    tmpl
                                                                                                }
                                                                                            };
                                                                                            let mut interim = String::new();
                                                                                            let mut desc_map_stream = desc_map2.clone();
                                                                                            let mut interim_map_stream = streaming_interim_sig.clone();
                                                                                            let path_clone_stream = path_target.clone();
                                                                                            let engine_clone_outer = engine_opt.clone();
                                                                                            let instruction_owned = instruction.clone();
                                                                                            let applied_flag = std::sync::Arc::new(
                                                                                                std::sync::atomic::AtomicBool::new(false),
                                                                                            );
                                                                                            let applied_flag_cb = applied_flag.clone();
                                                                                            let selected_path_sig_cb = selected_path_sig.clone();
                                                                                            let selected_ai_meta_sig_cb = selected_ai_meta_sig.clone();
                                                                                            let _ = crate::ai::joycaption_adapter::stream_describe_bytes_with_callback(
                                                                                                    bytes,
                                                                                                    &instruction_owned,
                                                                                                    |frag| {
                                                                                                        interim.push_str(frag);
                                                                                                        if let Some(partial_desc) = extract_partial_description(
                                                                                                            &interim,
                                                                                                        ) {
                                                                                                            interim_map_stream
                                                                                                                .write()
                                                                                                                .insert(path_clone_stream.clone(), partial_desc);
                                                                                                        } else {
                                                                                                            interim_map_stream
                                                                                                                .write()
                                                                                                                .insert(path_clone_stream.clone(), String::new());
                                                                                                        }
                                                                                                        if !applied_flag_cb
                                                                                                            .load(std::sync::atomic::Ordering::Relaxed)
                                                                                                        {
                                                                                                            if let Some(val) = crate::ai::joycaption_adapter::extract_json_vision(
                                                                                                                &interim,
                                                                                                            ) {
                                                                                                                if let Ok(vd_parsed) = serde_json::from_value::<
                                                                                                                    VisionDescription,
                                                                                                                >(val.clone()) {
                                                                                                                    if applied_flag_cb
                                                                                                                        .compare_exchange(
                                                                                                                            false,
                                                                                                                            true,
                                                                                                                            std::sync::atomic::Ordering::SeqCst,
                                                                                                                            std::sync::atomic::Ordering::SeqCst,
                                                                                                                        )
                                                                                                                        .is_ok()
                                                                                                                    {
                                                                                                                        let desc_clone_for_map = vd_parsed.description.clone();
                                                                                                                        if let Some(engine_mid) = engine_clone_outer.clone() {
                                                                                                                            let path_for_apply = path_clone_stream.clone();
                                                                                                                            let mut selected_ai_meta_sig_mid = selected_ai_meta_sig_cb
                                                                                                                                .clone();
                                                                                                                            let selected_path_sig_mid = selected_path_sig_cb.clone();
                                                                                                                            let vd_for_task = vd_parsed.clone();
                                                                                                                            spawn(async move {
                                                                                                                                let _ = engine_mid
                                                                                                                                    .apply_vision_description(&path_for_apply, &vd_for_task)
                                                                                                                                    .await;
                                                                                                                                if let Some(updated_meta) = engine_mid
                                                                                                                                    .get_file_metadata(&path_for_apply)
                                                                                                                                    .await
                                                                                                                                {
                                                                                                                                    if selected_path_sig_mid
                                                                                                                                        .read()
                                                                                                                                        .as_ref()
                                                                                                                                        .map(|p| p.display().to_string() == path_for_apply)
                                                                                                                                        .unwrap_or(false)
                                                                                                                                    {
                                                                                                                                        selected_ai_meta_sig_mid.set(Some(updated_meta));
                                                                                                                                    }
                                                                                                                                }
                                                                                                                            });
                                                                                                                        }
                                                                                                                        desc_map_stream
                                                                                                                            .write()
                                                                                                                            .insert(path_clone_stream.clone(), desc_clone_for_map);
                                                                                                                    }
                                                                                                                }
                                                                                                            }
                                                                                                        }
                                                                                                    },
                                                                                                )
                                                                                                .await
                                                                                                .map(|final_full| {
                                                                                                    if let Some(val) = crate::ai::joycaption_adapter::extract_json_vision(
                                                                                                        &final_full,
                                                                                                    ) {
                                                                                                        if let Ok(vd_final) = serde_json::from_value::<
                                                                                                            VisionDescription,
                                                                                                        >(val.clone()) {
                                                                                                            desc_map_stream
                                                                                                                .write()
                                                                                                                .insert(path_target.clone(), vd_final.description.clone());
                                                                                                        } else {
                                                                                                            log::warn!(
                                                                                                                "Final JSON did not parse to VisionDescription; discarding result for {}",
                                                                                                                path_target
                                                                                                            );
                                                                                                        }
                                                                                                    } else {
                                                                                                        log::warn!(
                                                                                                            "No VisionDescription JSON extracted for {}; discarding.",
                                                                                                            path_target
                                                                                                        );
                                                                                                    }
                                                                                                    interim_map_stream.write().remove(&path_target);
                                                                                                    if !applied_flag.load(std::sync::atomic::Ordering::Relaxed)
                                                                                                    {
                                                                                                        if let Some(engine) = engine_clone_outer.clone() {
                                                                                                            let path_clone_final = path_target.clone();
                                                                                                            let selected_path_sig_final = selected_path_sig.clone();
                                                                                                            let mut selected_ai_meta_sig_final = selected_ai_meta_sig
                                                                                                                .clone();
                                                                                                            let interim_final = interim.clone();
                                                                                                            spawn(async move {
                                                                                                                if let Some(val) = crate::ai::joycaption_adapter::extract_json_vision(
                                                                                                                    &interim_final,
                                                                                                                ) {
                                                                                                                    if let Ok(vd) = serde_json::from_value::<
                                                                                                                        VisionDescription,
                                                                                                                    >(val) {
                                                                                                                        let _ = engine
                                                                                                                            .apply_vision_description(&path_clone_final, &vd)
                                                                                                                            .await;
                                                                                                                    } else {
                                                                                                                        log::warn!(
                                                                                                                            "Final parsed JSON invalid VisionDescription for {}",
                                                                                                                            path_clone_final
                                                                                                                        );
                                                                                                                    }
                                                                                                                    if let Some(updated_meta) = engine
                                                                                                                        .get_file_metadata(&path_clone_final)
                                                                                                                        .await
                                                                                                                    {
                                                                                                                        if selected_path_sig_final
                                                                                                                            .read()
                                                                                                                            .as_ref()
                                                                                                                            .map(|p| p.display().to_string() == path_clone_final)
                                                                                                                            .unwrap_or(false)
                                                                                                                        {
                                                                                                                            selected_ai_meta_sig_final.set(Some(updated_meta));
                                                                                                                        }
                                                                                                                    }
                                                                                                                } else {
                                                                                                                    log::warn!(
                                                                                                                        "No JSON vision description produced for {}",
                                                                                                                        path_clone_final
                                                                                                                    );
                                                                                                                }
                                                                                                            });
                                                                                                        }
                                                                                                    }
                                                                                                });
                                                                                            return;
                                                                                        }
                                                                                    }
                                                                                }
                                                                                if let Some(engine) = engine_opt {
                                                                                    if let Some(vd) = engine
                                                                                        .generate_vision_description(&std::path::PathBuf::from(&path_target))
                                                                                        .await
                                                                                    {
                                                                                        desc_map2
                                                                                            .write()
                                                                                            .insert(path_target.clone(), vd.description.clone());
                                                                                        let _ = engine.apply_vision_description(&path_target, &vd).await;
                                                                                        if let Some(updated_meta) = engine
                                                                                            .get_file_metadata(&path_target)
                                                                                            .await
                                                                                        {
                                                                                            if selected_path_sig
                                                                                                .read()
                                                                                                .as_ref()
                                                                                                .map(|p| p.display().to_string() == path_target)
                                                                                                .unwrap_or(false)
                                                                                            {
                                                                                                selected_ai_meta_sig.set(Some(updated_meta));
                                                                                            }
                                                                                        }
                                                                                    } else {
                                                                                        log::warn!(
                                                                                            "VisionDescription generation returned None for {}", path_target
                                                                                        );
                                                                                    }
                                                                                }
                                                                            });
                                                                        },
                                                                        i { class: "material-icons text-sm", "bolt" }
                                                                        span {
                                                                            if existing_desc_opt.is_some() {
                                                                                " Regenerate"
                                                                            } else {
                                                                                " Generate"
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
                                        Separator {
                                            class: "separator",
                                            horizontal: true,
                                        }
                                        // Metadata ordering already Size, Modified, Created, Type (size precedes type as requested)
                                        if let Some(item_size) = item.size {
                                            div { class: "flex justify-between",
                                                span { class: "text-weak", "Size:" }
                                                span { "{format_size(item_size, DECIMAL)}" }
                                            }
                                        }
                                        if let Some(modified) = &item.modified {
                                            {
                                                let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();
                                                rsx! {
                                                    div { class: "flex justify-between",
                                                        span { class: "text-weak", "Modified:" }
                                                        span { "{modified_str}" }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(created) = &item.created {
                                            {
                                                let created_str = created.format("%Y-%m-%d %H:%M").to_string();
                                                rsx! {
                                                    div { class: "flex justify-between",
                                                        span { class: "text-weak", "Created:" }
                                                        span { "{created_str}" }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(ext) = selected.extension() {
                                            {
                                                let ext_str = ext.to_str().unwrap_or("").to_string();
                                                rsx! {
                                                    div { class: "flex justify-between",
                                                        span { class: "text-weak", "Type:" }
                                                        span { "{ext_str}" }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(meta_full) = selected_ai_meta.read().as_ref() {
                                            if let Some(caption) = &meta_full.caption {
                                                div { class: "text-11px",
                                                    span { class: "text-weak", "Caption:" }
                                                    p { class: "mt-0.5 truncate",
                                                        "{caption}"
                                                    }
                                                }
                                            }
                                            if let Some(h) = &meta_full.hash {
                                                {
                                                    let short = if h.len() > 5 {
                                                        format!("…{}", &h[h.len() - 5..])
                                                    } else {
                                                        h.clone()
                                                    };
                                                    rsx! {
                                                        div { class: "flex justify-between text-11px",
                                                            span { class: "text-weak", "Hash:" }
                                                            span { class: "font-mono", "{short}" }
                                                        }
                                                    }
                                                }
                                            }
                                            if let Some(embed) = &meta_full.embedding {
                                                div { class: "flex justify-between text-11px",
                                                    span { class: "text-weak", "Embedding dims:" }
                                                    span { "{embed.len()}" }
                                                }
                                            }
                                            // Category & Tags moved to end with toggle
                                            if meta_full.category.as_ref().map(|c| !c.is_empty()).unwrap_or(false)
                                                || !meta_full.tags.is_empty()
                                            {
                                                {
                                                    let tag_count = meta_full.tags.len();
                                                    let expanded = *show_tags.read();
                                                    rsx! {
                                                        div { class: "text-11px mt-2 border-t border-stroke pt-2 flex flex-col gap-1",
                                                            button {
                                                                class: "button self-start text-10px px-2 py-0.5",
                                                                "data-style": "outline",
                                                                onclick: move |_| {
                                                                    let new_val = !*show_tags.read();
                                                                    show_tags.set(new_val);
                                                                },
                                                                if expanded {
                                                                    "Hide tags"
                                                                } else {
                                                                    "Show tags"
                                                                }
                                                                if tag_count > 0 {
                                                                    span { class: "ml-1 text-weak", "({tag_count})" }
                                                                }
                                                            }
                                                            if expanded {
                                                                if let Some(cat) = &meta_full.category {
                                                                    if !cat.is_empty() {
                                                                        span { class: "font-semibold", "{cat}" }
                                                                    }
                                                                }
                                                                ul { class: "list-disc list-inside space-y-0.5 max-h-40 overflow-y-auto ml-1 pl-1",
                                                                    for t in meta_full.tags.iter() {
                                                                        li { key: "tag-{t}", "{t}" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else if *ai_search_active.read() {
                                    if let Some(ai_item) = ai_search_results
                                        .read()
                                        .iter()
                                        .find(|f| f.path == selected.display().to_string())
                                    {
                                        div { class: "space-y-2 text-sm",
                                            if let Some(desc) = ai_item.description.clone() {
                                                div { class: "panel p-2 text-11px leading-snug",
                                                    "data-style": "outline",
                                                    span { class: "font-semibold text-accent",
                                                        "AI Description:"
                                                    }
                                                    p { class: "mt-1", "{desc}" }
                                                }
                                            }
                                            div { class: "flex justify-between",
                                                span { class: "text-weak", "Type:" }
                                                span { "{ai_item.file_type}" }
                                            }
                                            div { class: "flex justify-between",
                                                span { class: "text-weak", "Path:" }
                                                span { class: "break-all",
                                                    "{compute_relative(&selected)}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "text-center py-8 text-weak",
                                i { class: "material-icons text-4xl mb-2 opacity-50",
                                    "preview"
                                }
                                p { "Select a file to preview" }
                            }
                        }
                    }
                }
            }
            if !*preview_collapsed.read() {
                div { class: "resize-handle", style: "position:absolute; top:0; left:-3px; width:6px; height:100%; cursor: ew-resize; user-select:none;",
                    onpointerdown: move |evt| { resizing_preview.set(Some((evt.client_coordinates().x as i32, *preview_width.read()))); }
                }
            }
        }
    }
}

