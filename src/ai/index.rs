use base64::Engine;

use crate::ai::FileMetadata;
use std::fs;

impl super::AISearchEngine {
    // Internal generalized indexer with optional force flag (bypass hash/description skip logic for reindex only).
    // NOTE: Description & embedding generation now rely solely on the corresponding auto_* atomic flags
    // (currently only auto_descriptions_enabled) and no longer use `force` to override.
    pub(crate) async fn index_file_internal(
        &self,
        mut metadata: super::FileMetadata,
        force: bool,
    ) -> anyhow::Result<(), anyhow::Error> {
        // Reentrancy / duplicate guard
        {
            let mut guard = self.indexing_in_progress.lock().await;
            if let Some(count) = guard.get_mut(&metadata.path) {
                *count += 1;
                log::warn!(
                    "[AI] Skipping duplicate indexing request for {} (active reentry count={})",
                    metadata.path,
                    count
                );
                return Ok(());
            } else {
                guard.insert(metadata.path.clone(), 1);
            }
        }
        let path = std::path::PathBuf::from(&metadata.path);
        log::info!(
            "Indexing file: {} (type: {})",
            metadata.path,
            metadata.file_type
        );
        // Compute hash to detect changes
        metadata.hash = self.compute_file_hash(&path).ok();
        if !force {
            if let Some(existing) = self.get_file_metadata(&metadata.path).await {
                if existing.hash.is_some() && existing.hash == metadata.hash {
                    if existing.description.is_some() || metadata.file_type != "image" {
                        log::info!(
                            "[AI] Skipping re-index (unchanged hash) for {}",
                            metadata.path
                        );
                        return Ok(());
                    }
                }
            }
        }
        
        // Image enrichment (description) controlled by auto_descriptions_enabled flag.
        // Now performed asynchronously to avoid blocking indexing on heavy model inference.
        if metadata.file_type == "image" && path.exists() {
            let auto_desc = self
                .auto_descriptions_enabled
                .load(std::sync::atomic::Ordering::Relaxed);
            if auto_desc {
                log::info!("[AI] Scheduling async description generation for {}", metadata.path);
                let arc_self = std::sync::Arc::new(self.clone());
                // Schedule after current indexing finishes storing initial metadata.
                let schedule_path = path.clone();
                tokio::spawn(async move {
                    arc_self.spawn_generate_vision_description(schedule_path);
                });
            }
        }
        // Normalize thumbnail fields: If thumb_b64 already contains a data URL, leave it.
        // If thumbnail_path references an on-disk file (not data URL) and we lack thumb_b64, encode it.
        if metadata
            .thumb_b64
            .as_ref()
            .map(|s| s.starts_with("data:image"))
            .unwrap_or(false)
            == false
        {
            if let Some(tp) = &metadata.thumbnail_path {
                if !tp.starts_with("data:image") {
                    if let Ok(bytes) = fs::read(tp) {
                        metadata.thumb_b64 = Some(format!(
                            "data:image/png;base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(bytes)
                        ));
                    }
                } else if metadata.thumb_b64.is_none() {
                    // Mis-assigned earlier code may have put data URL into thumbnail_path
                    metadata.thumb_b64 = Some(tp.clone());
                }
            }
        }
        // Store file in document table for semantic search
        self.ensure_document_table().await?;
        if let Some(document_table) = self.document_table.lock().await.as_ref() {
            let searchable_content = self.get_searchable_text(&metadata);
            if !searchable_content.is_empty() {
                let header = format!(
                    concat!(
                        "FILE_PATH:{}\n",
                        "HASH:{}\n",
                        "FILE_TYPE:{}\n",
                        "FILE_SIZE:{}\n",
                        "CATEGORY:{}\n",
                        "CAPTION:{}\n",
                        "TAGS:{}\n",
                        "DESCRIPTION:{}\n",
                    ),
                    metadata.path,
                    metadata.hash.clone().unwrap_or_default(),
                    metadata.file_type,
                    metadata.size,
                    metadata.category.clone().unwrap_or_default().replace('\n', " "),
                    metadata.caption.clone().unwrap_or_default().replace('\n', " "),
                    metadata.tags.join("|"),
                    metadata
                        .description
                        .clone()
                        .unwrap_or_default()
                        .replace('\n', " "),
                );
                let body = format!("{}\n{}", header, searchable_content);
                log::info!("Body: {body}");
                let doc = kalosm::language::Document::from_parts(metadata.filename.clone(), body);
                match document_table.insert(doc).await {
                    Ok(id) => {
                        metadata.id = Some(format!("{}", id));
                        if let Some(id_str) = &metadata.id {
                            self.path_to_id
                                .lock()
                                .await
                                .insert(metadata.path.clone(), id_str.clone());
                            // Attempt to fetch raw embedding via embedding_model for caching
                            if let Some(embed) =
                                self.try_get_embedding(document_table, id_str).await
                            {
                                metadata.embedding = Some(embed);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to insert document into semantic index: {}", e);
                    }
                }
            }
        }

        if let Err(e) = self.cache_thumbnail_and_metadata(&metadata).await {
            log::warn!("Thumbnail cache failed: {}", e);
        }

        // Store in memory (replace existing entry if same path)
        let mut files = self.files.lock().await;
        if let Some(existing_idx) = files.iter().position(|f| f.path == metadata.path) {
            files[existing_idx] = metadata.clone();
        } else {
            files.push(metadata.clone());
        }
        log::info!("Finished indexing file");

        // Remove reentrancy marker
        {
            let mut guard = self.indexing_in_progress.lock().await;
            guard.remove(&metadata.path);
        }
        Ok(())
    }

    // Return list of currently loaded (cached) file paths.
    pub async fn list_indexed_paths(&self) -> Vec<String> {
        let files = self.files.lock().await;
        files.iter().map(|f| f.path.clone()).collect()
    }

    pub async fn index_file(
        &self,
        metadata: FileMetadata,
    ) -> anyhow::Result<(), anyhow::Error> {
        self.index_file_internal(metadata, false).await
    }
}