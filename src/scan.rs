#![allow(dead_code)]
use crate::thumbs::generate_image_thumb_data; // image thumbnail generation
#[cfg(windows)]
use crate::thumbs::generate_video_thumb_data;
use crate::types::{DateField, Filters, FoundFile, MediaKind};
use chrono::{DateTime, Local}; // time conversion for metadata
use crossbeam::channel::{Sender, Receiver, unbounded};
use once_cell::sync::OnceCell; // global singletons for channel & pools
use rayon::iter::{IntoParallelRefIterator, ParallelIterator}; // extension matching parallel search
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering, AtomicU64};
use std::time::SystemTime;
use jwalk::{WalkDirGeneric, Parallelism};

#[derive(Debug)]
pub enum ScanMsg {
    Found(FoundFile),
    FoundBatch(Vec<FoundFile>),
    UpdateThumb {
        path: std::path::PathBuf,
        thumb: String,
    },
    Progress {
        scanned: usize,
        total: usize,
    },
    Error(String),
    Done,
}

fn systemtime_to_local(st: SystemTime) -> DateTime<Local> {
    DateTime::<Local>::from(st)
}

fn is_media_kind(path: &Path) -> MediaKind {
    use crate::types::{IMAGE_EXTS, VIDEO_EXTS};
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) if IMAGE_EXTS.par_iter().find_any(|e| **e == ext).is_some() => MediaKind::Image,
        Some(ext) if VIDEO_EXTS.par_iter().find_any(|e| **e == ext).is_some() => MediaKind::Video,
        _ => MediaKind::Other,
    }
}

static CANCEL_SCAN: AtomicBool = AtomicBool::new(false); 

// Global scanning channel (stable for entire app lifetime) so we don't swap Receivers in and out of scopes.
// We wrap each message with a scan generation id allowing us to ignore stale messages from previous scans.
#[derive(Debug)]
pub struct ScanEnvelope {
    pub scan_id: u64,
    pub msg: ScanMsg,
}

static SCAN_CHANNEL: OnceCell<(Sender<ScanEnvelope>, Receiver<ScanEnvelope>)> = OnceCell::new();
static SCAN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn global_scan_channel() -> &'static (Sender<ScanEnvelope>, Receiver<ScanEnvelope>) {
    SCAN_CHANNEL.get_or_init(|| unbounded())
}


pub fn next_scan_id() -> u64 { SCAN_ID_COUNTER.fetch_add(1, Ordering::Relaxed) }

pub fn global_scan_sender() -> Sender<ScanEnvelope> { global_scan_channel().0.clone() }
pub fn global_scan_receiver() -> &'static Receiver<ScanEnvelope> { &global_scan_channel().1 }

pub fn cancel_scan() {
    CANCEL_SCAN.store(true, Ordering::Relaxed);
}

// Main entry used by UI resource: offloads whole scan onto blocking thread pool.
pub async fn spawn_scan(filters: Filters, tx: Sender<ScanEnvelope>, recursive: bool, scan_id: u64) {
    CANCEL_SCAN.store(false, Ordering::Relaxed);
    // Await join so caller tasks don't instantly assume completion.
    if let Err(e) = tokio::task::spawn_blocking(move || perform_scan_blocking(filters, tx, recursive, scan_id)).await {
        log::warn!("spawn_blocking scan join error: {e:?}");
    }
}

fn perform_scan_blocking(filters: Filters, tx: Sender<ScanEnvelope>, recursive: bool, scan_id: u64) {
    let root = if filters.root.as_os_str().is_empty() {
        match std::env::current_dir().and_then(|p| std::path::absolute(p)) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Error(e.to_string()) });
                let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done });
                return;
            }
        }
    } else { filters.root.clone() };
    if !root.exists() {
        let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Error(format!("Root does not exist: {}", root.display())) });
        let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done });
        return;
    }

    // For recursive scans allow override date range
    let after = if recursive { filters.recursive_modified_after.as_ref().or(filters.modified_after.as_ref()) } else { filters.modified_after.as_ref() }
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .map(|d| d.and_hms_opt(0,0,0).unwrap());
    let before = if recursive { filters.recursive_modified_before.as_ref().or(filters.modified_before.as_ref()) } else { filters.modified_before.as_ref() }
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .map(|d| d.and_hms_opt(23,59,59).unwrap());

    let mut scanned = 0usize;
    let total = if recursive { 0 } else { estimate_total_shallow(&root, &filters) };
    let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned: 0, total } });

    if recursive {
        // Use jwalk for fast parallel recursive traversal.
        let filters_clone = filters.clone();
        let logical = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(16);
        let mut batch: Vec<FoundFile> = Vec::with_capacity(64);
        let mut scanned_paths: usize = 0;
        let parallelism = Parallelism::RayonNewPool(std::cmp::max(2, std::cmp::min(logical, 16)));

        let excluded_dirs = filters_clone.recursive_excluded_dirs.clone();
        let excluded_exts = filters_clone.recursive_excluded_exts.clone();
        // Precompute lowercase excluded extensions into a small HashSet for faster matching.
        use std::collections::HashSet as _HashSet;
        let excluded_exts_set: _HashSet<String> = excluded_exts.iter().map(|s| s.to_ascii_lowercase()).collect();
        let walker = WalkDirGeneric::<((), Option<u64>)>::new(&root)
            .skip_hidden(false)
            .follow_links(false)
            .process_read_dir(move |_depth, dir_path, _state, entries| {
                // If this directory itself is under an excluded directory prefix, skip reading children by clearing vector.
                if excluded_dirs.iter().any(|d| dir_path.starts_with(d)) {
                    entries.clear();
                    return;
                }
                // Filter & optionally sort entries in-place.
                entries.retain(|res| {
                    if let Ok(entry) = res {
                        // Skip hidden like ".DS_Store" style resource forks early.
                        if let Some(name) = entry.file_name.to_str() { if name.starts_with("._") { return false; } }
                        let p = entry.path();
                        if entry.file_type.is_dir() {
                            // If the dir is excluded, we want to yield it (so UI can see?) but skip children.
                            if excluded_dirs.iter().any(|d| p.starts_with(d)) {
                                // We cannot mutate entry in-place (immutable reference). Instead, drop it entirely
                                // so neither the dir nor its children are processed.
                                return false;
                            }
                            return true; // keep directory (will descend)
                        }
                        if entry.file_type.is_file() {
                            if let Some(ext) = p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                                // Quick media gate
                                let is_media = matches!(ext.as_str(), "jpg"|"jpeg"|"png"|"gif"|"bmp"|"webp"|"tiff"|"mp4"|"mov"|"mkv"|"avi"|"webm"|"m4v");
                                if !is_media { return false; }
                                if excluded_exts_set.contains(&ext) { return false; }
                                return true;
                            } else { return false; }
                        }
                        return false; // skip symlinks/other types
                    }
                    // Retain errors so they bubble and can be logged downstream.
                    true
                });
                // Optional: stable sort by file name for deterministic UI; modest cost.
                entries.sort_by(|a,b| match (a,b) { (Ok(ae), Ok(be)) => ae.file_name.cmp(&be.file_name), _ => std::cmp::Ordering::Equal });
            })
            .parallelism(parallelism);
        let mut idx: usize = 0;
        for dir_entry_result in walker {
            if CANCEL_SCAN.load(Ordering::Relaxed) { finish(&tx, scan_id); return; }
            match dir_entry_result {
                Ok(entry) => {
                    if entry.file_type.is_dir() { continue; }
                    let path_buf = entry.path();
                    let path = path_buf.as_path();
                    if let Some(found) = process_path_collect(scan_id, path, &filters_clone, after, before) {
                        batch.push(found);
                        if batch.len() >= 64 {
                            let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::FoundBatch(std::mem::take(&mut batch)) });
                        }
                    }
                    scanned_paths += 1;
                    if scanned_paths % 400 == 0 { let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned: scanned_paths, total: 0 } }); }
                }
                Err(err) => {
                    let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Error(format!("walk error: {err}")) });
                }
            }
            if idx % 1000 == 0 { std::thread::yield_now(); }
            idx += 1;
        }
        if !batch.is_empty() { let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::FoundBatch(batch) }); }
        let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned: scanned_paths, total: 0 } });
        finish(&tx, scan_id); return;
    } else {
        if let Ok(rd) = std::fs::read_dir(&root) {
            let mut batch: Vec<FoundFile> = Vec::with_capacity(64);
            for dent in rd.flatten() {
                if CANCEL_SCAN.load(Ordering::Relaxed) { finish(&tx, scan_id); return; }
                if let Ok(ft) = dent.file_type() { if !ft.is_file() { continue; } } else { continue; }
                let path = dent.path();
                if let Some(found) = process_path_collect(scan_id, &path, &filters, after, before) { batch.push(found); }
                if batch.len() >= 64 { let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::FoundBatch(std::mem::take(&mut batch)) }); }
                scanned += 1;
                let display_scanned = if total > 0 { scanned.min(total) } else { scanned };
                if scanned % 25 == 0 { let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned: display_scanned, total } }); }
                if scanned % 200 == 0 { std::thread::yield_now(); }
            }
            if !batch.is_empty() { let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::FoundBatch(batch) }); }
        }
    }
    let final_scanned = if total > 0 { scanned.min(total) } else { scanned };
    let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned: final_scanned, total } });
    finish(&tx, scan_id);
}

fn estimate_total_shallow(root: &Path, filters: &Filters) -> usize {
    if let Ok(rd) = std::fs::read_dir(root) {
        let mut count = 0usize;
        for dent in rd.flatten() {
            if let Ok(ft) = dent.file_type() { if !ft.is_file() { continue; } } else { continue; }
            let ext = dent.path().extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase());
            if let Some(ext) = ext {
                let is_img = matches!(ext.as_str(), "jpg"|"jpeg"|"png"|"gif"|"bmp"|"webp"|"tiff");
                let is_vid = matches!(ext.as_str(), "mp4"|"mov"|"mkv"|"avi"|"webm"|"m4v");
                if (filters.include_images && is_img) || (filters.include_videos && is_vid) { count += 1; }
            }
        }
        count
    } else { 0 }
}

fn finish(tx: &Sender<ScanEnvelope>, scan_id: u64) { let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done }); }

// (Removed process_path_blocking with single Found messages; batching only now.)

// ---------------- Thumbnail worker pool & failure cache ----------------
use std::collections::{HashSet, HashMap};
use std::sync::Mutex;

struct ThumbJob { scan_id: u64, path: std::path::PathBuf, kind: MediaKind, tx: Sender<ScanEnvelope> }

static THUMB_SENDER: OnceCell<crossbeam::channel::Sender<ThumbJob>> = OnceCell::new();
static FAILED_THUMBS: OnceCell<Mutex<HashSet<std::path::PathBuf>>> = OnceCell::new();
static INFLIGHT_THUMBS: OnceCell<Mutex<HashSet<std::path::PathBuf>>> = OnceCell::new();
static SUCCESS_THUMBS: OnceCell<Mutex<HashMap<std::path::PathBuf, String>>> = OnceCell::new();

fn failed_set() -> &'static Mutex<HashSet<std::path::PathBuf>> { FAILED_THUMBS.get_or_init(|| Mutex::new(HashSet::new())) }
fn inflight_set() -> &'static Mutex<HashSet<std::path::PathBuf>> { INFLIGHT_THUMBS.get_or_init(|| Mutex::new(HashSet::new())) }
fn success_map() -> &'static Mutex<HashMap<std::path::PathBuf,String>> { SUCCESS_THUMBS.get_or_init(|| Mutex::new(HashMap::new())) }
pub fn record_failed_thumb(path: &std::path::PathBuf) { if let Ok(mut m) = failed_set().lock() { m.insert(path.clone()); } }
pub fn is_failed_thumb(path: &std::path::PathBuf) -> bool { failed_set().lock().map(|m| m.contains(path)).unwrap_or(false) }
pub fn record_success_thumb(path: &std::path::PathBuf, data: &str) { if let Ok(mut m) = success_map().lock() { m.insert(path.clone(), data.to_string()); } }
pub fn get_success_thumb(path: &std::path::PathBuf) -> Option<String> { success_map().lock().ok().and_then(|m| m.get(path).cloned()) }

fn thumb_sender() -> crossbeam::channel::Sender<ThumbJob> {
    THUMB_SENDER.get_or_init(|| {
    let (tx, rx) = crossbeam::channel::unbounded::<ThumbJob>();
    let logical = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let workers = std::cmp::max(2, std::cmp::min(4, logical / 2));
        for i in 0..workers {
            let rx = rx.clone();
            std::thread::Builder::new().name(format!("thumb-worker-{i}")).spawn(move || {
                while let Ok(job) = rx.try_recv() {
                    if is_failed_thumb(&job.path) { if let Ok(mut set) = inflight_set().lock() { set.remove(&job.path); } continue; }
                    let thumb_res = if job.kind == MediaKind::Image {
                        generate_image_thumb_data(&job.path)
                    } else {
                        #[cfg(windows)]
                        { generate_video_thumb_data(&job.path) }
                        #[cfg(not(windows))]
                        { Err("video thumbs unsupported".into()) }
                    };
                    match thumb_res {
                        Ok(thumb) => {
                            record_success_thumb(&job.path, &thumb);
                            let _ = job.tx.try_send(ScanEnvelope { scan_id: job.scan_id, msg: ScanMsg::UpdateThumb { path: job.path.clone(), thumb } });
                        }
                        Err(e) => { log::debug!("thumb fail {}: {e}", job.path.display()); record_failed_thumb(&job.path); }
                    }
                    // Remove from inflight set after completion (success or failure)
                    if let Ok(mut set) = inflight_set().lock() { set.remove(&job.path); }
                }
            }).expect("spawn thumb worker");
        }
        tx
    }).clone()
}

fn spawn_thumb_task(scan_id: u64, kind: MediaKind, path_thumb: std::path::PathBuf, tx: Sender<ScanEnvelope>) {
    if is_failed_thumb(&path_thumb) { return; }
    if let Some(name) = path_thumb.file_name().and_then(|s| s.to_str()) { if name.starts_with("._") { return; } }
    // Reuse existing successful cached thumbnail immediately
    if let Some(existing) = get_success_thumb(&path_thumb) {
        let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::UpdateThumb { path: path_thumb, thumb: existing } });
        return;
    }
    // Deduplicate concurrent scheduling
    {
        if let Ok(mut set) = inflight_set().lock() {
            if set.contains(&path_thumb) { return; }
            set.insert(path_thumb.clone());
        }
    }
    let _ = thumb_sender().try_send(ThumbJob { scan_id, path: path_thumb, kind, tx });
}

fn process_path_collect(
    scan_id: u64,
    path: &Path,
    filters: &Filters,
    after: Option<chrono::NaiveDateTime>,
    before: Option<chrono::NaiveDateTime>,
    // tx removed for collect variant
) -> Option<FoundFile> {
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) { if name.starts_with("._") { return None; } }
    let kind = is_media_kind(path);
    if !((filters.include_images && kind == MediaKind::Image)
        || (filters.include_videos && kind == MediaKind::Video))
    {
        return None;
    }
    let md = match path.metadata() {
        Ok(m) => m,
        Err(_) => return None,
    };
    let modified = md.modified().ok().map(systemtime_to_local);
    let created = md.created().ok().map(systemtime_to_local);
    match filters.date_field {
        DateField::Modified => {
            if let Some(m) = modified {
                if let Some(a) = after {
                    if m.naive_local() < a {
                        return None;
                    }
                }
                if let Some(b) = before {
                    if m.naive_local() > b {
                        return None;
                    }
                }
            }
        }
        DateField::Created => {
            if let Some(c) = created {
                if let Some(a) = after {
                    if c.naive_local() < a {
                        return None;
                    }
                }
                if let Some(b) = before {
                    if c.naive_local() > b {
                        return None;
                    }
                }
            }
        }
    }
    let size = Some(md.len());
    let item = FoundFile {
        path: path.to_path_buf(),
        modified,
        created,
        size,
        kind: kind.clone(),
        thumb_data: None,
    };
    let item_for_thumb = item.clone();
    if kind == MediaKind::Image || kind == MediaKind::Video {
        spawn_thumb_task(scan_id, kind.clone(), path.to_path_buf(), global_scan_sender());
    }
    Some(item_for_thumb)
}
