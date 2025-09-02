use dioxus::prelude::*;
use std::collections::HashMap;
use crate::Thumbnail;

/// Access the globally provided thumbnail/metainfo cache.
pub fn use_all_cached() -> Signal<HashMap<String, Thumbnail>> {
    use_context::<Signal<HashMap<String, Thumbnail>>>()
}
