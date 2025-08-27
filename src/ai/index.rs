use base64::Engine;

use crate::ai::FileMetadata;
use std::fs;

impl super::AISearchEngine {
    // Internal generalized indexer with optional force flag (bypass hash/description skip logic)
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
        // Generate AI description for images (manual unless auto flag enabled or force)
        let auto_desc = self.auto_descriptions_enabled.load(std::sync::atomic::Ordering::Relaxed);
        if metadata.file_type == "image" && path.exists() && (auto_desc || force) {
            log::info!(
                "[AI] Generating description inline during indexing for {}",
                metadata.path
            );
            let start = std::time::Instant::now();
            if let Some(vd) = self.generate_vision_description(&path).await {
                metadata.description = Some(vd.description);
                metadata.caption = Some(vd.caption);
                if !vd.category.trim().is_empty() { metadata.category = Some(vd.category); }
                metadata.tags = vd.tags; // ensure tags from struct (in case not already set)
            }
            let ms = start.elapsed().as_millis();
            match &metadata.description {
                Some(d) => log::info!(
                    "[AI] Description generated ({} chars, {} ms) for {}",
                    d.len(),
                    ms,
                    metadata.path
                ),
                None => log::warn!(
                    "[AI] Description generation returned None for {} ({} ms)",
                    metadata.path,
                    ms
                ),
            }

            
            let auto_clip = self.auto_clip_embeddings_enabled.load(std::sync::atomic::Ordering::Relaxed);
            if metadata.clip_embedding.is_none() && (auto_clip || force) {
                if let Err(e) = self.ensure_clip_engine().await { log::error!("[CLIP] ensure failed: {e}"); }
                if let Some(engine) = self.clip_engine.lock().await.as_mut() {
                    match engine.embed_image_path(&metadata.path) {
                        Ok(vec) => {
                            metadata.clip_embedding = Some(vec.clone());
                            if metadata.tags.len() < 2 {
                                let mut tags = engine.zero_shot_tags(metadata.clip_embedding.as_ref().unwrap(), 3);
                                for t in tags.drain(..) { if !metadata.tags.iter().any(|et| et == &t) { metadata.tags.push(t); } }
                            }
                            if metadata.category.is_none() { metadata.category = engine.zero_shot_category(metadata.clip_embedding.as_ref().unwrap()); }
                        }
                        Err(e) => log::error!("[CLIP] embedding failed for {}: {e}", metadata.path),
                    }
                }
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

    // Tags: if an image description supplied tags they are already set. Non-image files currently remain with existing tags vector (may be empty).

        // Store file in document table for semantic search
        self.ensure_document_table().await?;
        if let Some(document_table) = self.document_table.lock().await.as_ref() {
            let searchable_content = self.get_searchable_text(&metadata);
            if !searchable_content.is_empty() {
                // Build enriched metadata header lines (machine & AI data) then body searchable content.
                let header = format!(
                    concat!(
                        "FILE_PATH:{}\n",
                        "HASH:{}\n",
                        "FILE_TYPE:{}\n",
                        "FILE_SIZE:{}\n",
                        "CATEGORY:{}\n",
                        "CAPTION:{}\n",
                        "TAGS:{}\n",
                        "SEGMENTS:{}\n",
                        "DESCRIPTION:{}\n",
                        "OCR:{}\n"
                    ),
                    metadata.path,
                    metadata.hash.clone().unwrap_or_default(),
                    metadata.file_type,
                    metadata.size,
                    metadata.category.clone().unwrap_or_default().replace('\n', " "),
                    metadata.caption.clone().unwrap_or_default().replace('\n', " "),
                    metadata.tags.join("|"),
                    metadata
                        .segments
                        .clone()
                        .map(|v| v.join("|"))
                        .unwrap_or_default(),
                    metadata
                        .description
                        .clone()
                        .unwrap_or_default()
                        .replace('\n', " "),
                    metadata
                        .text_content
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

        // Cache thumbnail & AI metadata in Surreal (best-effort)
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

    // Force reindex a path even if hash unchanged (refresh description & tags)
    pub async fn force_reindex_path(
        &self,
        path: &str,
    ) -> anyhow::Result<(), anyhow::Error> {
        if let Some(existing) = self.get_file_metadata(path).await {
            let mut meta = existing.clone();
            // Clear description so a fresh one is generated
            meta.description = None;
            self.index_file_internal(meta, true).await
        } else {
            Err(anyhow::anyhow!("File not previously indexed"))
        }
    }
}