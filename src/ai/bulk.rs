use dioxus::prelude::*;
use crate::ai::AISearchEngine;
use crate::types::{FoundFile, IMAGE_EXTS};

/// Spawn an async task to bulk-generate (vision) descriptions for all image rows.
/// Updates `progress` (done,total) and flips `bulk_flag` while running.
/// On failure or empty queue, sets an error message via `err_sig`.
pub fn spawn_bulk_generate(
    engine_opt: Option<AISearchEngine>,
    rows: Vec<FoundFile>,
    prompt_template: String,
    progress: Signal<(usize, usize)>,
    bulk_flag: Signal<bool>,
    err_sig: Signal<Option<String>>,
    overwrite: bool,
) {
    if *bulk_flag.read() { return; }
    // create local mutable copies so we can call set before moving into async
    let mut bulk_flag_local = bulk_flag;
    bulk_flag_local.set(true);

    spawn(async move {
        // Shadow captured signals as mutable inside the async task so we can call .set()
        let mut progress = progress;
        let mut bulk_flag = bulk_flag_local;
        let mut err_sig = err_sig;

    // initialize progress (0 done, total)
    progress.set((0, rows.len()));
        if let Some(engine) = engine_opt {
            for (idx, f) in rows.iter().enumerate() {
                // Skip if description exists and we are not overwriting
                if !overwrite {
                    if let Some(meta) = engine.get_file_metadata(&f.path.display().to_string()).await {
                        if meta.description.is_some() {
                            progress.set((idx + 1, rows.len()));
                            continue;
                        }
                    }
                }
                let ext = f
                    .path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();
                if !IMAGE_EXTS.iter().any(|e| *e == ext) {
                    progress.set((idx + 1, rows.len()));
                    continue;
                }
                #[cfg(feature = "joycaption")]
                {
                    if crate::ai::joycaption_adapter::is_enabled() {
                        if let Ok(bytes) = tokio::fs::read(&f.path).await {
                            let mut interim = String::new();
                            let _ = crate::ai::joycaption_adapter::stream_describe_bytes_with_callback(
                                bytes,
                                &prompt_template,
                                |frag| {
                                    interim.push_str(frag);
                                },
                            )
                            .await;
                            if let Some(val) =
                                crate::ai::joycaption_adapter::extract_json_vision(&interim)
                            {
                                if let Ok(vd) = serde_json::from_value::<
                                    crate::ai::generate::VisionDescription,
                                >(val)
                                {
                                    let _ = engine
                                        .apply_vision_description(
                                            &f.path.display().to_string(),
                                            &vd,
                                        )
                                        .await;
                                } else {
                                    let _ = engine
                                        .set_file_description(
                                            &f.path.display().to_string(),
                                            &interim,
                                        )
                                        .await;
                                }
                            } else {
                                let _ = engine
                                    .set_file_description(
                                        &f.path.display().to_string(),
                                        &interim,
                                    )
                                    .await;
                            }
                        }
                    } else {
                        if let Some(vd) = engine.generate_vision_description(&f.path).await {
                            let _ = engine
                                .apply_vision_description(
                                    &f.path.display().to_string(),
                                    &vd,
                                )
                                .await;
                        }
                    }
                }
                #[cfg(not(feature = "joycaption"))]
                {
                    if let Some(vd) = engine.generate_vision_description(&f.path).await {
                        let _ = engine
                            .apply_vision_description(&f.path.display().to_string(), &vd)
                            .await;
                    }
                }
                progress.set((idx + 1, rows.len()));
            }
        } else {
            err_sig.set(Some("AI engine not initialized".into()));
        }
        bulk_flag.set(false);
    });
}
