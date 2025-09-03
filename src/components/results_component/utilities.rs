use std::path::PathBuf;
use dioxus::prelude::*;
use super::thumbnails::{ThumbTask, THUMB_CHANNELS};

pub fn enqueue_thumb(path: &PathBuf, is_image: bool, is_video: bool) {
    let (tx, _, _, _) = &*THUMB_CHANNELS;
    // Best-effort enqueue (ignore send error if channel closed)
    let _ = tx.send(ThumbTask { path: path.clone(), is_image, is_video });
}

pub fn set_date_range(filters: &mut Signal<crate::utilities::types::Filters>, days: i64) {
    use chrono::{Local, Duration};
    let now = Local::now().date_naive();
    let after = now - Duration::days(days);
    let mut f = filters.write();
    f.modified_after = Some(after.to_string());
    f.modified_before = Some(now.to_string());
}

pub fn clear_dates(filters: &mut Signal<crate::utilities::types::Filters>) {
    let mut f = filters.write();
    f.modified_after = None;
    f.modified_before = None;
}

pub fn shorten_middle(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    if max <= 3 { return s[..max.min(s.len())].to_string(); }
    let keep = max - 3;
    let front = keep / 2;
    let back = keep - front;
    format!("{}...{}", &s[..front], &s[s.len()-back..])
}

// Modified details_header still calls plain_col -> re-add helper (was removed)
pub fn plain_col(label: &str) -> Element {
    rsx! {
        span { class: "truncate", "{label}" }
    }
}