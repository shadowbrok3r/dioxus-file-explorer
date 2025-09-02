use dioxus::prelude::*;

/// Access enriched FileRecord vector (provided higher in the tree).
pub fn use_file_records() -> Memo<Vec<crate::utilities::types::FileRecord>> {
    use_context::<Memo<Vec<crate::utilities::types::FileRecord>>>()
}

pub struct FilteredRecords {
    pub records: Memo<Vec<crate::utilities::types::FileRecord>>,
    pub categories_available: Memo<std::collections::BTreeSet<String>>,
    pub grouped: Memo<Option<std::collections::BTreeMap<String, Vec<crate::utilities::types::FileRecord>>>>,
}

/// Derive filtered and optionally grouped records based on global filter contexts.
pub fn use_filtered_records(group_by_category: Signal<bool>) -> FilteredRecords {
    let file_records = use_file_records();
    let filters = use_context::<Signal<crate::utilities::types::Filters>>();
    let ext_enabled = use_context::<Signal<std::collections::BTreeMap<String,bool>>>();
    let excluded_dirs = use_context::<Signal<std::collections::BTreeSet<std::path::PathBuf>>>();
    let search_text = use_context::<Signal<String>>();
    let records_memo = use_memo(move || {
        let enabled = ext_enabled.read().clone();
        let excluded = excluded_dirs.read().clone();
    let needle = search_text.read().to_ascii_lowercase();
    let needle_is_pathlike = needle.contains('\\') || needle.contains('/') || needle.contains(':');
        let f = filters.read().clone();
        let base_len = file_records.read().len();
        let mut kept = 0usize;
        let v: Vec<_> = file_records.read().iter().filter(|rec| {
            if f.only_with_thumb && rec.thumb_data.is_none() { return false; }
            if let Some(ext) = rec.path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                if let Some(flag) = enabled.get(&ext) { if !*flag { return false; } }
            }
            if excluded.iter().any(|ex| rec.path.starts_with(ex)) { return false; }
            // Only apply name substring filter if needle is non-empty and not path-like.
            if !needle.is_empty() && !needle_is_pathlike {
                let name_lc = rec.path.file_name().and_then(|f| f.to_str()).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
                if !name_lc.contains(&needle) { return false; }
            }
            if f.only_with_description && rec.description.is_none() { return false; }
            if !f.category_filters.is_empty() {
                if let Some(cat) = rec.category.as_ref() {
                    if !f.category_filters.contains(cat) { return false; }
                } else { return false; }
            } else if let Some(single) = f.category_filter.as_ref() {
                if rec.category.as_ref() != Some(single) { return false; }
            }
            kept += 1;
            true
        }).cloned().collect();
        log::warn!("[filter-debug] base={base_len} kept={kept} only_with_thumb={} only_with_desc={} needle='{}' pathlike={} enabled_exts={}",
            f.only_with_thumb, f.only_with_description, needle, needle_is_pathlike, enabled.len());
        v
    });
    let categories_available = use_memo(move || {
        let mut set = std::collections::BTreeSet::new();
        for r in records_memo.read().iter() { if let Some(c) = r.category.as_ref() { if !c.is_empty() { set.insert(c.clone()); } } }
        set
    });
    let grouped = use_memo(move || {
        if !*group_by_category.read() { None } else {
            let mut map = std::collections::BTreeMap::<String, Vec<crate::utilities::types::FileRecord>>::new();
            for r in records_memo.read().iter() { let cat = r.category.clone().unwrap_or_else(|| "Uncategorized".into()); map.entry(cat).or_default().push(r.clone()); }
            Some(map)
        }
    });
    FilteredRecords { records: records_memo, categories_available, grouped }
}
