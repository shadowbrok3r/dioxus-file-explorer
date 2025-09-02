use dioxus::prelude::*;
use std::collections::HashMap;
use crate::Thumbnail;

pub struct ScanChannelState {
    pub results: Signal<crate::utilities::types::ScanResults>,
    pub progress: Signal<Option<(usize,usize)>>,
    pub scanning: Signal<bool>,
    pub generation: Signal<u64>,
}

/// Drain the global scan receiver and update reactive signals.
pub fn use_scan_channel() -> ScanChannelState {
    // Pull required signals from contexts established by ScanBlock, FiltersBlock, and App.
    let results = use_context::<Signal<crate::utilities::types::ScanResults>>();
    log::info!("Results: {:?}", results);
    let progress = use_context::<Signal<Option<(usize,usize)>> >();
    let scanning = use_context::<Signal<bool>>();
    let generation = use_context::<Signal<u64>>();
    let all_cached = use_context::<Signal<HashMap<String, Thumbnail>>>();
    let ext_filters = use_context::<Signal<std::collections::BTreeSet<String>>>();
    let ext_enabled = use_context::<Signal<std::collections::BTreeMap<String,bool>>>();
    let error = use_context::<Signal<Option<String>>>();
    let scan_finished = use_context::<Signal<Option<std::time::Instant>>>();
    use_future(move || {
        let mut results_sig = results.clone();
        let mut progress_sig = progress.clone();
        let mut scanning_sig = scanning.clone();
        let generation_sig = generation.clone();
        let all_cached_sig = all_cached.clone();
        let mut ext_filters_sig = ext_filters.clone();
        let mut ext_enabled_sig = ext_enabled.clone();
        let mut error_sig = error.clone();
        let mut scan_finished_sig = scan_finished.clone();
        async move {
            use tokio::time::{sleep, Duration};
            let mut pending_thumb_rows: Vec<crate::Thumbnail> = Vec::with_capacity(120);
            const BATCH_THRESHOLD: usize = 100;
            loop {
                let active_gen = *generation_sig.read();
                let rx = crate::utilities::scan::global_scan_receiver();
                let mut processed = 0usize;
                let mut latest_progress: Option<(usize,usize)> = None;
                let mut new_items: Vec<crate::utilities::types::FoundFile> = Vec::new();
                let mut thumb_updates: Vec<(std::path::PathBuf,String)> = Vec::new();
                while let Ok(env) = rx.try_recv() {
                    log::info!("Results: {:?}", results);
                    if env.scan_id != active_gen {
                        if processed % 200 == 0 { log::warn!("[scan-channel] discarding msg for old_gen={} active_gen={}", env.scan_id, active_gen); }
                        continue;
                    }
                    match env.msg {
                        crate::utilities::scan::ScanMsg::Found(mut item) => {
                            if let Some(ext) = item.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                                if !ext_filters_sig.read().contains(&ext) { ext_filters_sig.write().insert(ext.clone()); ext_enabled_sig.write().entry(ext.clone()).or_insert(true); }
                            }
                            let path_str = item.path.display().to_string();
                            if let Some(row) = all_cached_sig.read().get(&path_str) {
                                if item.thumb_data.is_none() { item.thumb_data = row.thumbnail_b64.clone(); }
                            } else if let Some(row) = crate::file_to_thumbnail(&item) { pending_thumb_rows.push(row); }
                            new_items.push(item);
                        }
                        crate::utilities::scan::ScanMsg::FoundBatch(batch) => {
                            for mut item in batch.into_iter() {
                                if let Some(ext) = item.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                                    if !ext_filters_sig.read().contains(&ext) { ext_filters_sig.write().insert(ext.clone()); ext_enabled_sig.write().entry(ext.clone()).or_insert(true); }
                                }
                                let path_str = item.path.display().to_string();
                                if let Some(row) = all_cached_sig.read().get(&path_str) {
                                    if item.thumb_data.is_none() { item.thumb_data = row.thumbnail_b64.clone(); }
                                } else if let Some(row) = crate::file_to_thumbnail(&item) { pending_thumb_rows.push(row); }
                                new_items.push(item);
                                if pending_thumb_rows.len() >= BATCH_THRESHOLD {
                                    let batch = std::mem::take(&mut pending_thumb_rows);
                                    spawn(async move { if let Err(e) = crate::database::save_thumbnail_batch(batch).await { log::warn!("scan batch persistence failed: {e}"); }});
                                }
                            }
                        }
                        crate::utilities::scan::ScanMsg::UpdateThumb { path, thumb } => { thumb_updates.push((path, thumb)); }
                        crate::utilities::scan::ScanMsg::Progress { scanned, total } => { latest_progress = Some((scanned,total)); }
                        crate::utilities::scan::ScanMsg::Error(e) => { error_sig.set(Some(e)); scanning_sig.set(false); }
                        crate::utilities::scan::ScanMsg::Done => {
                            if *generation_sig.read() == active_gen { scanning_sig.set(false); scan_finished_sig.set(Some(std::time::Instant::now())); }
                            if !pending_thumb_rows.is_empty() {
                                let batch = std::mem::take(&mut pending_thumb_rows);
                                spawn(async move { if let Err(e) = crate::database::save_thumbnail_batch(batch).await { log::warn!("final scan batch persistence failed: {e}"); }});
                            }
                        }
                    }
                    processed += 1; if processed > 1200 { break; }
                }
                if !new_items.is_empty() {
                    let mut w = results_sig.write();
                    use std::collections::HashSet;
                    let mut existing: HashSet<String> = w.items.iter().map(|f| f.path.display().to_string()).collect();
                    for it in new_items.into_iter() {
                        let key = it.path.display().to_string();
                        if existing.insert(key.clone()) { w.items.push(it); } else if let Some(row) = all_cached_sig.read().get(&key) {
                            if let Some(existing_item) = w.items.iter_mut().find(|f| f.path.display().to_string()==key) {
                                if existing_item.thumb_data.is_none() { existing_item.thumb_data = row.thumbnail_b64.clone(); }
                            }
                        }
                    }
                } else {
                    if results_sig.read().items.is_empty() {
                        log::info!("[scan-channel] loop idle: no new_items, total remains 0 (gen={})", *generation_sig.read());
                    }
                }
                if !thumb_updates.is_empty() {
                    let mut w = results_sig.write();
                    for (p,t) in thumb_updates { if let Some(f) = w.items.iter_mut().find(|f| f.path == p) { f.thumb_data = Some(t); } }
                }
                if let Some(p) = latest_progress.take() { progress_sig.set(Some(p)); }
                sleep(Duration::from_millis(if processed > 0 { 20 } else { 80 })).await;
            }
        }
    });
    ScanChannelState { results, progress, scanning, generation }
}
