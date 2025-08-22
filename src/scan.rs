use crate::explorer::list_dir_items;
use crate::thumbs::{generate_image_thumb_data, generate_video_thumb_data};
use crate::types::{DateField, Filters, FoundFile, MediaKind, ScanResults};
use chrono::{DateTime, Local};
use crossbeam::channel::{unbounded, Receiver, Sender};
use dioxus::prelude::*;
use std::path::Path;
use std::time::SystemTime;
use walkdir::{DirEntry, WalkDir};

pub enum ScanMsg { Found(FoundFile), UpdateThumb { path: std::path::PathBuf, thumb: String }, Progress { scanned: usize, total: usize }, Error(String), Done }

fn systemtime_to_local(st: SystemTime) -> DateTime<Local> { DateTime::<Local>::from(st) }

fn is_media_kind(path: &Path) -> MediaKind {
    use crate::types::{IMAGE_EXTS, VIDEO_EXTS};
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        Some(ext) if IMAGE_EXTS.iter().any(|e| *e == ext) => MediaKind::Image,
        Some(ext) if VIDEO_EXTS.iter().any(|e| *e == ext) => MediaKind::Video,
        _ => MediaKind::Other,
    }
}

fn entry_ok(entry: &DirEntry) -> bool { entry.file_type().is_file() }

pub fn begin_scan(
    filters: Signal<Filters>,
    mut rx_state: Signal<Option<Receiver<ScanMsg>>>,
    mut scanning: Signal<bool>,
    mut results: Signal<ScanResults>,
    mut dir_items: Signal<Vec<crate::types::DirItem>>,
    mut progress: Signal<Option<(usize, usize)>>,
    recursive: bool,
) {
    if let Ok(items) = list_dir_items(filters.read().root.clone()) { dir_items.set(items); } else { dir_items.set(Vec::new()); }
    let f = filters.read().clone();
    let (tx, rx) = unbounded();
    rx_state.set(Some(rx));
    scanning.set(true);
    progress.set(None);
    results.set(ScanResults::default());
    spawn_scan(f, tx, recursive);
}

pub fn spawn_scan(filters: Filters, tx: Sender<ScanMsg>, recursive: bool) {
    std::thread::spawn(move || {
        let root = if filters.root.as_os_str().is_empty() {
            match std::path::absolute(std::env::current_dir().unwrap()) { Ok(p) => p, Err(e) => { let _ = tx.send(ScanMsg::Error(e.to_string())); return; } }
        } else { filters.root.clone() };
        if !root.exists() { let _ = tx.send(ScanMsg::Error(format!("Root does not exist: {}", root.display()))); let _ = tx.send(ScanMsg::Done); return; }

        let after = filters.modified_after.as_ref().and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()).map(|d| d.and_hms_opt(0,0,0).unwrap());
        let before = filters.modified_before.as_ref().and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()).map(|d| d.and_hms_opt(23,59,59).unwrap());

        // Sequential streaming for predictable ordering & immediate UI feedback
        let mut scanned: usize = 0;
        // Precompute total for shallow scan only (fast) limited to potential media files; skip for recursive to avoid large directory walk upfront
        let total = if recursive { 0 } else { match std::fs::read_dir(&root) {
            Ok(rd) => rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
                .filter(|e| {
                    let p = e.path();
                    match p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()) {
                        Some(ext) => {
                            let is_img = ["jpg","jpeg","png","gif","bmp","webp","tiff"].contains(&ext.as_str());
                            let is_vid = ["mp4","mov","mkv","avi","webm","m4v"].contains(&ext.as_str());
                            (filters.include_images && is_img) || (filters.include_videos && is_vid)
                        },
                        None => false
                    }
                })
                .count(),
            Err(_) => 0
        }};
        let _ = tx.send(ScanMsg::Progress { scanned: 0, total });

        if recursive {
            for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok).filter(entry_ok) {
                let path = entry.into_path();
                process_path(&path, &filters, after, before, &tx);
                scanned += 1;
                let _ = tx.send(ScanMsg::Progress { scanned, total: 0 });
                if scanned % 100 == 0 { std::thread::yield_now(); }
            }
        } else {
            if let Ok(rd) = std::fs::read_dir(&root) {
                for dent in rd.filter_map(|e| e.ok()) {
                    if dent.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        let path = dent.path();
                        process_path(&path, &filters, after, before, &tx);
                        scanned += 1;
                        let _ = tx.send(ScanMsg::Progress { scanned, total });
                        if scanned % 50 == 0 { std::thread::yield_now(); }
                    }
                }
            }
        }
        let _ = tx.send(ScanMsg::Progress { scanned, total });
        let _ = tx.send(ScanMsg::Done);
    });
}

fn process_path(path: &Path, filters: &Filters, after: Option<chrono::NaiveDateTime>, before: Option<chrono::NaiveDateTime>, tx: &Sender<ScanMsg>) {
    let kind = is_media_kind(path);
    if !((filters.include_images && kind == MediaKind::Image) || (filters.include_videos && kind == MediaKind::Video)) { return; }
    let md = match path.metadata() { Ok(m) => m, Err(_) => return };
    let modified = md.modified().ok().map(systemtime_to_local);
    let created = md.created().ok().map(systemtime_to_local);
    match filters.date_field {
        DateField::Modified => if let Some(m) = modified { if let Some(a) = after { if m.naive_local() < a { return; } } if let Some(b) = before { if m.naive_local() > b { return; } } },
        DateField::Created => if let Some(c) = created { if let Some(a) = after { if c.naive_local() < a { return; } } if let Some(b) = before { if c.naive_local() > b { return; } } },
    }
    let size = Some(md.len());
    let item = FoundFile { path: path.to_path_buf(), modified, created, size, kind: kind.clone(), thumb_data: None };
    let _ = tx.send(ScanMsg::Found(item));
    if kind == MediaKind::Image || kind == MediaKind::Video {
        let tx_thumb = tx.clone();
        let path_thumb = path.to_path_buf();
        rayon::spawn(move || {
            let thumb_opt = if kind == MediaKind::Image { generate_image_thumb_data(&path_thumb).ok() } else { generate_video_thumb_data(&path_thumb).ok() };
            if let Some(thumb) = thumb_opt { let _ = tx_thumb.send(ScanMsg::UpdateThumb { path: path_thumb, thumb }); }
        });
    }
}
