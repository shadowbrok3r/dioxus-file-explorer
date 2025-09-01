use chrono::{DateTime, Local};
use once_cell::sync::Lazy;
use std::path::PathBuf;

// Supported media extensions
pub static IMAGE_EXTS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp", "heic", "heif", "avif",
    ]
});
pub static VIDEO_EXTS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "mp4", "mov", "avi", "mkv", "webm", "wmv", "m4v", "flv", "mpeg", "mpg", "3gp",
    ]
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Icons,
    Details,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateField {
    Modified,
    #[allow(dead_code)]
    Created,
}

#[derive(Clone, Debug)]
pub struct Filters {
    pub root: PathBuf,
    pub include_images: bool,
    pub include_videos: bool,
    pub modified_after: Option<String>, // YYYY-MM-DD
    pub modified_before: Option<String>,
    pub date_field: DateField,
    pub only_with_thumb: bool, // UI-only filter (applied client-side) to show only items that already have a loaded thumbnail
    pub only_with_description: bool, // UI-only: only show items that have an AI description
    pub category_filter: Option<String>, // If Some(cat) show only that category
    pub category_filters: std::collections::BTreeSet<String>, // Multi-select categories (union filter); empty => all
    // Recursive scan specific settings (ignored in shallow scans unless noted)
    pub recursive_excluded_dirs: std::collections::BTreeSet<PathBuf>,
    pub recursive_excluded_exts: std::collections::BTreeSet<String>, // lowercase extensions w/out dot
    pub recursive_modified_after: Option<String>, // override date range just for recursive scans
    pub recursive_modified_before: Option<String>,
}

impl Default for Filters {
    fn default() -> Self {
        let root = std::path::absolute(std::env::current_dir().unwrap())
            .unwrap_or_else(|_| PathBuf::from("."));
        Self {
            root,
            include_images: true,
            include_videos: true,
            modified_after: None,
            modified_before: None,
            date_field: DateField::Modified,
            only_with_thumb: false,
            only_with_description: false,
            category_filter: None,
            category_filters: std::collections::BTreeSet::new(),
            recursive_excluded_dirs: std::collections::BTreeSet::new(),
            recursive_excluded_exts: std::collections::BTreeSet::new(),
            recursive_modified_after: None,
            recursive_modified_before: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScanResults {
    pub items: Vec<FoundFile>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaKind {
    Image,
    Video,
    Other,
}
impl Default for MediaKind {
    fn default() -> Self {
        MediaKind::Other
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FoundFile {
    pub path: PathBuf,
    pub modified: Option<DateTime<Local>>,
    pub created: Option<DateTime<Local>>,
    pub size: Option<u64>,
    pub kind: MediaKind,
    pub thumb_data: Option<String>, // data URL for image thumbnails
}

impl FoundFile {
    pub fn icon_name(&self) -> &'static str {
        match self.kind {
            MediaKind::Image => "photo",
            MediaKind::Video => "smart_display",
            MediaKind::Other => "insert_drive_file",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirItem {
    pub path: PathBuf,
}

#[derive(Clone)]
pub struct QuickAccess {
    pub label: String,
    pub path: PathBuf,
    #[allow(dead_code)]
    pub include_images: bool,
}