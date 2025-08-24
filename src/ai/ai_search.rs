use std::{path::PathBuf, sync::Arc, collections::HashMap};
use surrealdb::{Surreal, engine::local::SurrealKv};
use kalosm::language::*;
use tokio::sync::Mutex;

impl super::AISearchEngine {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing AI Search Engine with full Kalosm integration...");

        // Create SurrealDB connection
        let db: Surreal<surrealdb::engine::local::Db> =
            Surreal::new::<SurrealKv>("./db/ai_search.db").await?;
        db.use_ns("file_explorer").use_db("ai_search").await?;

        Ok(Self {
            vision_model: Arc::new(Mutex::new(None)),
            // gpt_model: Arc::new(Mutex::new(None)),
            db: Arc::new(db),
            document_table: Arc::new(Mutex::new(None)),
            files: Arc::new(Mutex::new(Vec::new())),
            path_to_id: Arc::new(Mutex::new(HashMap::new())),
            indexing_in_progress: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn ensure_vision_model(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut model_guard = self.vision_model.lock().await;
        let model_name = "gpt-5-nano"; // "gpt-4.1-mini";
        if model_guard.is_none() {
            log::info!("[AI] Loading {model_name}");
            let openai = OpenAICompatibleChatModelBuilder::new()
                .with_model(model_name)
                .build();

            *model_guard = Some(openai);
            log::info!("Loaded {model_name}");
            // match Llama::builder()
            //     .with_flash_attn(true)
            //     .with_source(
            //         LlamaSource::qwen_2_5_3b_vl_chat_q4(), // LlamaSource::new(FileSource::Local(
            //                                                //     r#"C:\Users\darkm\AppData\Roaming\kalosm\cache\ggml-org\Qwen2.5-VL-32B-Instruct-GGUF\main\Qwen2.5-VL-32B-Instruct-Q4_K_M.gguf"#
            //                                                // ))
            //     )
            //     .build()
            //     .await
            // {
            //     // qwen_2_5_7b_vl_chat_f16
            //     Ok(model) => {
            //         *model_guard = Some(model);
            //         log::info!("[AI] Vision model qwen_2_5_32b_vl_chat_f16 loaded successfully");
            //     }
            //     Err(e) => log::error!("[AI] Failed to load qwen_2_5_32b_vl_chat_f16 model ({e})"),
            // }
        }
        Ok(())
    }

    pub async fn ensure_document_table(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut table_guard = self.document_table.lock().await;
        if table_guard.is_none() {
            log::info!("Initializing document table for semantic search...");

            let chunker = SemanticChunker::new();
            let document_table = self
                .db
                .document_table_builder("file_documents")
                .with_chunker(chunker)
                .at("./db/file_embeddings.db")
                .build::<kalosm::language::Document>()
                .await?;

            *table_guard = Some(document_table);
            log::info!("Document table initialized successfully");
        } else {
            log::error!("Vec<Documents>: {:?}", table_guard.as_ref().unwrap().select_all().await?);
        }
        Ok(())
    }

    // Background enrichment: generate descriptions for any previously indexed images that are missing one.
    pub async fn enrich_missing_descriptions(&self) -> usize {
        let mut generated = 0usize;
        // Clone list of indices to avoid holding lock while generating each description.
        let snapshot: Vec<String> = {
            let files = self.files.lock().await;
            files
                .iter()
                .filter(|f| {
                    f.file_type == "image"
                        && (f.description.is_none()
                            || f.description
                                .as_ref()
                                .map(|d| d.trim().len() < 12)
                                .unwrap_or(true))
                })
                .map(|f| f.path.clone())
                .collect()
        };
        if snapshot.is_empty() {
            return 0;
        }
        log::info!(
            "[AI] Enriching descriptions for {} images (missing or too short)",
            snapshot.len()
        );
        if let Err(e) = self.ensure_vision_model().await {
            log::error!("Failed to load vision model for enrichment: {}", e);
            return 0;
        }
        for path in snapshot {
            let pb = PathBuf::from(&path);
            if !pb.exists() {
                continue;
            }
            log::info!("[AI] Enrichment generating description for {}", path);
            if let Some(vd) = self.generate_vision_description(&pb).await {
                // Update in-memory
                {
                    let mut files = self.files.lock().await;
                    if let Some(f) = files.iter_mut().find(|f| f.path == path) {
                        f.description = Some(vd.description.clone());
                        f.caption = Some(vd.caption.clone());
                        // Tags come directly from structured vision response
                        f.tags = vd.tags.clone();
                        f.category = if vd.category.trim().is_empty() { None } else { Some(vd.category.clone()) };
                        log::info!("[AI] Enrichment stored description for {}\nDesc: {:?}\nCaption: {:?}\nTags: {:?}", f.path, f.description, f.caption, f.tags);
                    }
                }
                // Persist updated metadata (best-effort)
                if let Some(updated) = self.get_file_metadata(&path).await {
                    if let Err(e) = self.cache_thumbnail_and_metadata(&updated).await {
                        log::warn!("Failed to update cached row for {}: {}", path, e);
                    }
                }
                generated += 1;
            }
        }
        log::info!(
            "[AI] Description enrichment complete (generated {})",
            generated
        );
        generated
    }

    // Count images lacking a sufficiently descriptive caption.
    pub async fn count_missing_descriptions(&self) -> usize {
        let files = self.files.lock().await;
        files
            .iter()
            .filter(|f| {
                f.file_type == "image"
                    && (f.description.is_none()
                        || f.description
                            .as_ref()
                            .map(|d| d.trim().len() < 12)
                            .unwrap_or(true))
            })
            .count()
    }

    pub fn compute_file_hash(&self, path: &PathBuf) -> Result<String, std::io::Error> {
        use std::io::Read;
        if !path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "file not found",
            ));
        }
        let mut file = std::fs::File::open(path)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    // Return up to 'limit' thumbnail/cache rows directly from Surreal for debug view.
    pub async fn list_thumbnail_rows(&self, limit: usize) -> Vec<super::ThumbRow> {
        let rows: Result<Vec<super::ThumbRow>, _> = self.db.select("thumbnails").await;
        match rows {
            Ok(mut v) => {
                v.sort_by(|a,b| a.path.cmp(&b.path));
                if v.len() > limit { v.truncate(limit); }
                v
            }
            Err(e) => { log::warn!("Debug list_thumbnail_rows failed: {}", e); Vec::new() }
        }
    }

    // List semantic document snippets (id + first 160 chars) for debug.
    pub async fn list_document_snippets(&self, limit: usize) -> Vec<super::DebugDocumentSnippet> {
        let mut out = Vec::new();
        if let Some(table) = self.document_table.lock().await.as_ref() {
            match table.select_all().await { // assuming select_all returns Vec<Document>
                Ok(docs) => {
                    for d in docs.into_iter().take(limit) {
                        let text = d.body();
                        let preview = text.chars().take(160).collect::<String>();
                        let title = Some(d.title().to_string());
                        out.push(super::DebugDocumentSnippet { id: "".into(), title, preview, len: text.len() });
                    }
                }
                Err(e) => log::warn!("Debug list_document_snippets failed: {}", e),
            }
        }
        out
    }
}

// Helper function to extract metadata from FoundFile
pub fn found_file_to_metadata(found_file: &crate::types::FoundFile) -> super::FileMetadata {
    super::FileMetadata {
        id: None,
        path: found_file.path.display().to_string(),
        filename: found_file
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string(),
        file_type: match found_file.kind {
            crate::types::MediaKind::Image => "image".to_string(),
            crate::types::MediaKind::Video => "video".to_string(),
            crate::types::MediaKind::Other => "other".to_string(),
        },
        size: found_file.size.unwrap_or(0),
        modified: found_file.modified,
        created: found_file.created,
        // Only set thumb_b64; do not misuse thumbnail_path for base64 data URLs.
        thumbnail_path: None,
        thumb_b64: found_file.thumb_data.clone(),
        hash: None,
        description: None,  // Will be generated by AI
    caption: None,      // Will be generated by AI
        tags: Vec::new(),   // Will be extracted by AI
    category: None,     // Will be generated by AI
        text_content: None, // (OCR disabled; could repurpose for future text extraction)
        embedding: None,    // Will be generated by AI
        similarity_score: None,
        segments: None,
        segment_objects: None,
        object_counts: None,
    }
}
