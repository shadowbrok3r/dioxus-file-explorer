use crate::explorer::list_dir_items;
use crate::thumbs::generate_image_thumb_data;
#[cfg(windows)]
use crate::thumbs::generate_video_thumb_data;
use crate::types::{DateField, Filters, FoundFile, MediaKind, ScanResults};
use chrono::{DateTime, Local};
use crossbeam::channel::{Sender, Receiver, unbounded};
use dioxus::prelude::*;
use once_cell::sync::OnceCell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering, AtomicU64};
use std::time::SystemTime;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug)]
pub enum ScanMsg {
    Found(FoundFile),
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
        Some(ext) if IMAGE_EXTS.iter().any(|e| *e == ext) => MediaKind::Image,
        Some(ext) if VIDEO_EXTS.iter().any(|e| *e == ext) => MediaKind::Video,
        _ => MediaKind::Other,
    }
}

fn entry_ok(entry: &DirEntry) -> bool {
    entry.file_type().is_file()
}

static CANCEL_SCAN: AtomicBool = AtomicBool::new(false); // cancellation flag

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

pub fn begin_scan(
    filters: Signal<Filters>,
    mut scan_generation: Signal<u64>,
    mut scanning: Signal<bool>,
    mut results: Signal<ScanResults>,
    mut dir_items: Signal<Vec<crate::types::DirItem>>,
    mut progress: Signal<Option<(usize, usize)>>,
    recursive: bool,
) {
    CANCEL_SCAN.store(false, Ordering::Relaxed);
    if let Ok(items) = list_dir_items(filters.read().root.clone()) { dir_items.set(items); } else { dir_items.set(Vec::new()); }
    let f = filters.read().clone();
    let scan_id = next_scan_id();
    scan_generation.set(scan_id);
    scanning.set(true);
    progress.set(None);
    results.set(ScanResults::default());
    let tx = global_scan_sender();
    spawn_scan(f, tx, recursive, scan_id);
}

pub fn spawn_scan(filters: Filters, tx: Sender<ScanEnvelope>, recursive: bool, scan_id: u64) {
    // Move heavy synchronous filesystem walk into Tokio's blocking pool so the main UI thread isn't blocked.
    // This requires a Tokio runtime (dioxus desktop sets one up). We ignore the JoinHandle result.
    tokio::spawn(async move {
        let root = if filters.root.as_os_str().is_empty() {
            match std::path::absolute(std::env::current_dir().unwrap()) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Error(e.to_string()) });
                    return;
                }
            }
        } else {
            filters.root.clone()
        };
        if !root.exists() {
            let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Error(format!(
                "Root does not exist: {}",
                root.display()
            )) });
            let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done });
            return;
        }

        let after = filters
            .modified_after
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap());
        let before = filters
            .modified_before
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| d.and_hms_opt(23, 59, 59).unwrap());

        // Sequential streaming for predictable ordering & immediate UI feedback
        let mut scanned: usize = 0;
        // Precompute total for shallow scan only (fast) limited to potential media files; skip for recursive to avoid large directory walk upfront
        let total = if recursive {
            0
        } else {
            let mut count = 0;
            match tokio::fs::read_dir(&root).await {
                Ok(mut rd) => {
                    while let Ok(Some(entry)) = rd.next_entry().await {
                        // gracefully skip if file_type() fails
                        match entry.file_type().await {
                            Ok(ft) if ft.is_file() => ft,
                            _ => continue,
                        };

                        let ext = entry
                            .path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_ascii_lowercase());

                        let pass = match ext {
                            Some(ext) => {
                                let is_img = matches!(
                                    ext.as_str(),
                                    "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff"
                                );
                                let is_vid = matches!(
                                    ext.as_str(),
                                    "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v"
                                );
                                (filters.include_images && is_img)
                                    || (filters.include_videos && is_vid)
                            }
                            None => false,
                        };

                        if pass {
                            count += 1;
                        }
                    }
                    count
                }
                Err(_) => 0,
            }
        };
    let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned: 0, total } });

        if recursive {
            for entry in WalkDir::new(&root)
                .into_iter()
                .filter_map(Result::ok)
                .filter(entry_ok)
            {
                if CANCEL_SCAN.load(Ordering::Relaxed) {
                    let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done });
                    return;
                }
                let path = entry.into_path();
                process_path(scan_id, &path, &filters, after, before, &tx);
                scanned += 1;
                let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned, total: 0 } });
                if scanned % 100 == 0 { }
            }
        } else {
            if let Ok(mut rd) = tokio::fs::read_dir(&root).await {
                while let Ok(Some(dent)) = rd.next_entry().await {
                    if CANCEL_SCAN.load(Ordering::Relaxed) {
                        let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done });
                        return;
                    }

                    match dent.file_type().await {
                        Ok(ft) if ft.is_file() => {
                            let path = dent.path();
                            process_path(scan_id, &path, &filters, after, before, &tx);
                            scanned += 1;
                            let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned, total } });

                            if scanned % 50 == 0 {
                                // Yield back to scheduler so other tasks can run
                                tokio::task::yield_now().await;
                            }
                        }
                        _ => continue,
                    }
                }
            }
        }
        if CANCEL_SCAN.load(Ordering::Relaxed) {
            let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done });
            return;
        }
        let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Progress { scanned, total } });
        let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Done });
    });
}

fn process_path(
    scan_id: u64,
    path: &Path,
    filters: &Filters,
    after: Option<chrono::NaiveDateTime>,
    before: Option<chrono::NaiveDateTime>,
    tx: &Sender<ScanEnvelope>,
) {
    let kind = is_media_kind(path);
    if !((filters.include_images && kind == MediaKind::Image)
        || (filters.include_videos && kind == MediaKind::Video))
    {
        return;
    }
    let md = match path.metadata() {
        Ok(m) => m,
        Err(_) => return,
    };
    let modified = md.modified().ok().map(systemtime_to_local);
    let created = md.created().ok().map(systemtime_to_local);
    match filters.date_field {
        DateField::Modified => {
            if let Some(m) = modified {
                if let Some(a) = after {
                    if m.naive_local() < a {
                        return;
                    }
                }
                if let Some(b) = before {
                    if m.naive_local() > b {
                        return;
                    }
                }
            }
        }
        DateField::Created => {
            if let Some(c) = created {
                if let Some(a) = after {
                    if c.naive_local() < a {
                        return;
                    }
                }
                if let Some(b) = before {
                    if c.naive_local() > b {
                        return;
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
    let _ = tx.try_send(ScanEnvelope { scan_id, msg: ScanMsg::Found(item) });
    if kind == MediaKind::Image || kind == MediaKind::Video {
        let tx_thumb = tx.clone();
        let path_thumb = path.to_path_buf();
        rayon::spawn(move || {
            let thumb_opt = if kind == MediaKind::Image {
                generate_image_thumb_data(&path_thumb).ok()
            } else {
                #[cfg(windows)]
                {
                    generate_video_thumb_data(&path_thumb).ok()
                }
                #[cfg(not(windows))]
                {
                    None // Video thumbnails not supported on non-Windows platforms yet
                }
            };
            if let Some(thumb) = thumb_opt {
                log::debug!("[scan] sending UpdateThumb {} ({} chars)", path_thumb.display(), thumb.len());
                let _ = tx_thumb.try_send(ScanEnvelope { scan_id, msg: ScanMsg::UpdateThumb { path: path_thumb, thumb } });
            } else {
                log::debug!("[scan] no thumbnail generated for {}", path_thumb.display());
            }
        });
    }
}
