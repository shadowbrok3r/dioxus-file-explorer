#[derive(serde::Deserialize)]
struct CachedRow {
    path: String,
    filename: String,
    file_type: String,
    size: u64,
    description: Option<String>,
    caption: Option<String>,
    tags: Vec<String>,
    category: Option<String>,
    ocr: Option<String>,
    segments: Option<Vec<String>>,
    embedding: Option<Vec<f32>>,
    thumbnail_b64: Option<String>,
    modified: Option<String>,
    hash: Option<String>,
}

impl super::AISearchEngine {
    // Cache thumbnail & AI metadata in surrealdb table `thumbnails` (id = path)
    pub async fn cache_thumbnail_and_metadata(
        &self,
        metadata: &super::FileMetadata,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Prefer existing in-memory base64 thumbnail if present; else attempt to read from on-disk path.
        let thumb_b64 = if let Some(b64) = &metadata.thumb_b64 {
            Some(b64.clone().trim().to_string())
        } else if let Some(tp) = &metadata.thumbnail_path {
            if tp.starts_with("data:image") {
                Some(tp.clone())
            } else {
                use base64::Engine;
                match std::fs::read(tp) {
                    Ok(bytes) => Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                    Err(_) => None,
                }
            }
        } else {
            None
        };

        let row = super::ThumbRow {
            path: metadata.path.clone(),
            filename: metadata.filename.clone(),
            file_type: metadata.file_type.clone(),
            size: metadata.size,
            description: metadata.description.clone(),
            caption: metadata.caption.clone(),
            tags: metadata.tags.clone(),
            category: metadata.category.clone(),
            ocr: metadata.text_content.clone(),
            segments: metadata.segments.clone(),
            embedding: metadata.embedding.clone(),
            thumbnail_b64: thumb_b64,
            modified: metadata.modified.map(|dt| dt.to_rfc3339()),
            hash: metadata.hash.clone(),
        };
        // Upsert semantics: surrealdb SQL style
        // Using Surreal Rust API create (if available) would look like: self.db.create(("thumbnails", row.path.clone())).content(row).await?;
        // Keeping query form for now but adjusting per user suggestion to treat as struct create.
        let _: Option<super::ThumbRow> = self
            .db
            .create("thumbnails")
            .content::<super::ThumbRow>(row)
            .await?
            .take();
        Ok(())
    }

    // Load previously cached thumbnail/meta rows from Surreal into memory so we don't
    // re-index (and especially don't re-run expensive vision description) every launch.
    // Returns number of records loaded.
    pub async fn load_cached(&self) -> usize {
        let rows: Result<Vec<CachedRow>, _> = self.db.select("thumbnails").await;
        let mut loaded = 0usize;
        match rows {
            Ok(list) => {
                if list.is_empty() {
                    return 0;
                }
                let mut files_guard = self.files.lock().await;
                for r in list.into_iter() {
                    // Attempt parse of modified timestamp
                    let modified_dt = r
                        .modified
                        .as_ref()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Local));

                    let meta = super::FileMetadata {
                        id: None, // document id not restored (not needed for search mapping)
                        path: r.path.clone(),
                        filename: r.filename.clone(),
                        file_type: r.file_type.clone(),
                        size: r.size,
                        modified: modified_dt,
                        created: modified_dt,
                        // We persist only the base64 thumbnail (thumbnail_b64). Older rows may have stored
                        // a path in thumbnail_b64 erroneously if a previous bug existed; we detect a likely
                        // data URL by prefix. If it's not a data URL we keep it in thumbnail_path so later
                        // code can try to load & convert it.
                        thumbnail_path: r.thumbnail_b64.as_ref().and_then(|s| {
                            if s.starts_with("data:image") {
                                None
                            } else {
                                Some(s.clone())
                            }
                        }),
                        thumb_b64: r.thumbnail_b64.as_ref().and_then(|s| {
                            if s.starts_with("data:image") {
                                Some(s.clone())
                            } else {
                                None
                            }
                        }),
                        hash: r.hash.clone(),
                        description: r.description.clone(),
                        caption: r.caption.clone(),
                        tags: r.tags.clone(),
                        category: r.category.clone(),
                        text_content: r.ocr.clone(),
                        embedding: r.embedding.clone(),
                        similarity_score: None,
                        segments: r.segments.clone(),
                        segment_objects: None,
                        object_counts: None,
                    };
                    files_guard.push(meta);
                    loaded += 1;
                }
                log::info!("Loaded {} cached AI metadata rows", loaded);
            }
            Err(e) => {
                log::warn!("Failed to load cached AI metadata: {}", e);
            }
        }
        loaded
    }

}