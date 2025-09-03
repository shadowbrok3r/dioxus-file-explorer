use dioxus::prelude::*;
use std::path::PathBuf;
use once_cell::sync::Lazy;
use crossbeam::channel::{unbounded, Sender, Receiver};
use std::collections::HashMap;
use crate::utilities::types::{FoundFile, IMAGE_EXTS, VIDEO_EXTS};

use super::utilities::enqueue_thumb;

#[derive(Clone, Debug)]
pub struct ThumbTask { pub path: PathBuf, pub is_image: bool, pub is_video: bool }

// New helper component: enqueues thumbnail tasks & polls global result channel
#[derive(Props, PartialEq, Clone)]
pub struct BulkThumbLoaderProps {
    items: Vec<FoundFile>,
    all_cached: Signal<HashMap<String, crate::Thumbnail>>, // full cached rows
}

#[allow(non_snake_case)]
pub fn BulkThumbLoader(props: BulkThumbLoaderProps) -> Element {
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


pub static THUMB_CHANNELS: Lazy<(Sender<ThumbTask>, Receiver<ThumbTask>, Sender<(PathBuf,String)>, Receiver<(PathBuf,String)>)> = Lazy::new(|| {
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
