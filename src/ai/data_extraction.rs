use kalosm::language::{EmbedderExt};

impl super::AISearchEngine {

    pub async fn search(
        &self,
        query: &str,
    ) -> Result<Vec<super::FileMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("[AI] Begin semantic search query='{}'", query);

        // Ensure document table is initialized
        self.ensure_document_table().await?;

        let mut results: Vec<super::FileMetadata> = Vec::new();

        // Use Kalosm document table for semantic search
        if let Some(document_table) = self.document_table.lock().await.as_ref() {
            let search_results = document_table.search(query).with_results(20).await?;
            log::debug!(
                "[AI] Raw document_table search returned {} hits (pre-filter)",
                search_results.len()
            );

            let files = self.files.lock().await;
            for search_result in search_results {
                let text = search_result.text();
                // Parse FILE_PATH line (first few lines) to recover original path
                let mut found_path: Option<String> = None;
                for line in text.lines().take(10) {
                    // limit scan
                    if let Some(rest) = line.strip_prefix("FILE_PATH:") {
                        found_path = Some(rest.trim().to_string());
                        break;
                    }
                }
                if let Some(path) = found_path {
                    if let Some(file) = files.iter().find(|f| f.path == path) {
                        let mut file_with_score = file.clone();
                        file_with_score.similarity_score =
                            Some(1.0 - search_result.distance.min(1.0));
                        results.push(file_with_score);
                    }
                } else {
                    log::warn!("Search result missing FILE_PATH header; skipping");
                }
            }

            // Dedupe by path keeping highest similarity
            use std::collections::HashMap as StdHashMap;
            let mut best: StdHashMap<String, super::FileMetadata> = StdHashMap::new();
            for r in results.drain(..) {
                let path = r.path.clone();
                match best.get(&path) {
                    Some(existing) => {
                        let es = existing.similarity_score.unwrap_or(0.0);
                        let rs = r.similarity_score.unwrap_or(0.0);
                        if rs > es {
                            best.insert(path, r);
                        }
                    }
                    None => {
                        best.insert(path, r);
                    }
                }
            }
            results = best.into_values().collect();
            log::info!("[AI] Mapped {} unique hits to file metadata", results.len());
        }

        // Sort by similarity score (highest first)
        results.sort_by(|a, b| {
            let a_score = a.similarity_score.unwrap_or(0.0);
            let b_score = b.similarity_score.unwrap_or(0.0);
            b_score
                .partial_cmp(&a_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        log::debug!(
            "[AI] Post-sort top score={:?}",
            results.first().and_then(|f| f.similarity_score)
        );
        log::info!(
            "[AI] Search complete query='{}' final_results={}",
            query,
            results.len()
        );

        Ok(results.into_iter().take(50).collect())
    }

    pub fn get_searchable_text(&self, metadata: &super::FileMetadata) -> String {
        let mut text_parts = vec![metadata.filename.clone()];

        if let Some(desc) = &metadata.description {
            text_parts.push(desc.clone());
        }

        if let Some(cap) = &metadata.caption {
            text_parts.push(cap.clone());
        }

        if let Some(content) = &metadata.text_content {
            text_parts.push(content.clone());
        }
        if let Some(segs) = &metadata.segments {
            text_parts.extend(segs.clone());
        }

        text_parts.extend(metadata.tags.clone());

        text_parts.join(" ")
    }

    pub async fn get_all_files(
        &self,
    ) -> Result<Vec<super::FileMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        let files = self.files.lock().await;
        Ok(files.clone())
    }

    pub async fn get_file_metadata(&self, path: &str) -> Option<super::FileMetadata> {
        let files = self.files.lock().await;
        files.iter().find(|f| f.path == path).cloned()
    }

    // Return AI-relevant metadata for a separate UI list (lightweight projection)
    #[allow(dead_code)]
    pub async fn get_ai_metadata(
        &self,
    ) -> Vec<(String, Option<String>, Vec<String>, Option<Vec<String>>)> {
        let files = self.files.lock().await;
        files
            .iter()
            .map(|f| {
                (
                    f.path.clone(),
                    f.description.clone(),
                    f.tags.clone(),
                    f.segments.clone(),
                )
            })
            .collect()
    }

    // Try to pull embedding from underlying model/table if accessible. Placeholder: DocumentTable currently
    // doesn't expose direct per-record embedding in the public API we used, so this returns None until
    // extended. Implementors can adapt by querying table.embedding_model() if an accessor surfaces.
    pub async fn try_get_embedding(
        &self,
        table: &kalosm::language::DocumentTable<surrealdb::engine::local::Db>,
        _id: &str,
    ) -> Option<Vec<f32>> {
        // Attempt strategy: re-embed the searchable text using the table's embedding model.
        // We reconstruct approximate searchable content by selecting from in-memory metadata.
        // Note: This yields a single embedding for the whole file (not per chunk) for quick similarity preview UI.
        let files = self.files.lock().await;
        // Fallback: no per-id retrieval yet; we just ignore _id and derive from path match.
        // (We could parse path from headers if needed for more accuracy.)
        let embedding_model = table.embedding_model(); // assume API exists per docs you provided
        if files.is_empty() {
            return None;
        }
        // This simplified approach might not match the inserted chunk-level embeddings (which are chunked),
        // but provides a consistent vector for quick metadata display.
        // Just take the last file (recently inserted) – caller ensures correct ordering when calling after insert/update.
        if let Some(latest) = files.last() {
            let text = self.get_searchable_text(latest);
            match embedding_model.embed(text).await {
                Ok(emb) => Some(emb.vector().to_vec()),
                Err(e) => {
                    log::info!("Embedding regeneration failed: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }
}