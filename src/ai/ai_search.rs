use std::{path::PathBuf, sync::Arc, collections::HashMap};
use surrealdb::{Surreal, engine::local::SurrealKv};
use kalosm::language::*;
use tokio::sync::Mutex;

impl super::AISearchEngine {
    pub async fn new() -> anyhow::Result<Self, anyhow::Error> {
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
            

            auto_descriptions_enabled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            index_tx: Arc::new(Mutex::new(None)),
            index_queue_len: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            index_active: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            index_completed: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    // Convenience: build inside an Arc directly (part of Arc refactor start)
    // pub async fn new_shared() -> anyhow::Result<std::sync::Arc<Self>, anyhow::Error> {
    //     Ok(std::sync::Arc::new(Self::new().await?))
    // }

    pub async fn ensure_vision_model(
        &self,
    ) -> Result<(), anyhow::Error> {
        // If joycaption feature is enabled and local adapter available, skip loading GGUF vision model.
        #[cfg(feature = "joycaption")]
        {
            if crate::ai::joycaption_adapter::is_enabled() {
                // Ensure the joycaption worker is started (best-effort) and bail out.
                if let Err(e) = crate::ai::joycaption_adapter::ensure_loaded().await { log::warn!("[AI] joycaption ensure_loaded failed: {e}"); }
                return Ok(());
            }
        }
        let mut model_guard = self.vision_model.lock().await;
        // let model_name = "gpt-5-nano"; // "gpt-4.1-mini";
        if model_guard.is_none() {
            // log::info!("[AI] Loading {model_name}");
            // let openai = OpenAICompatibleChatModelBuilder::new()
            //     .with_model(model_name)
            //     .build();

            // *model_guard = Some(openai);
            // log::info!("Loaded {model_name}");
            // *model_guard = None;
            let path = r#"G:\Users\Owner\Desktop\llama-joycaption-beta-one-hf-llava-mmproj-gguf\"#;
            let model_name = "Llama-Joycaption-Beta-One-Hf-Llava-Q4_K.gguf";
            match Llama::builder()
                // .with_flash_attn(true)
                .with_source(
                    /**/
                    LlamaSource::new(FileSource::Local(
                        format!("{path}{model_name}").into() // IQ4_XS // Q4_K_S
                    ))
                    // .with_vision_model(FileSource::Local(
                    //     r#"G:\Users\Owner\Desktop\llama-joycaption-beta-one-hf-llava-mmproj-gguf\llama-joycaption-beta-one-llava-mmproj-model-f16.gguf"#.into()
                    // )) 
                    // LlamaSource::new(FileSource::Local(
                    //     r#"G:\Users\Owner\Downloads\llama-joycaption-beta-one-hf-llava.i1-Q4_K_S.gguf"#.into() // IQ4_XS // Q4_K_S
                    // )) // .with_vision_model(model)
                )
                .build_with_loading_handler(|progress| match progress {
                    ModelLoadingProgress::Downloading { source, progress } => {
                        let progress_percent = (progress.progress * 100) as u32;
                        let elapsed = progress.start_time.elapsed().as_secs_f32();
                        log::info!("Downloading file {source} {progress_percent}% ({elapsed}s)");
                    }
                    ModelLoadingProgress::Loading { progress } => {
                        let progress = (progress * 100.0) as u32;
                        log::warn!("Loading model {progress}%");
                    }
                })
                .await
            {
                // qwen_2_5_7b_vl_chat_f16
                Ok(model) => {
                    *model_guard = Some(model);
                    log::info!("[AI] Vision model {model_name} loaded successfully");
                }
                Err(e) => log::error!("[AI] Failed to load {model_name} model ({e})"),
            }
        }
        Ok(())
    }

    pub async fn ensure_document_table(&self) -> Result<(), anyhow::Error> {
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
        // Collect snapshot of paths needing enrichment.
        let snapshot: Vec<std::path::PathBuf> = {
            let files = self.files.lock().await;
            files.iter().filter(|f| {
                f.file_type == "image" && (f.description.is_none() || f.description.as_ref().map(|d| d.trim().len() < 12).unwrap_or(true))
            }).map(|f| std::path::PathBuf::from(&f.path)).collect()
        };
        if snapshot.is_empty() { return 0; }
        log::info!("[AI] Scheduling enrichment for {} images", snapshot.len());
        // Ensure model once (async wait) before spawning individual tasks; if this fails we return 0 scheduled.
        if let Err(e) = self.ensure_vision_model().await {
            log::error!("Failed to ensure vision model before scheduling enrichment: {}", e);
            return 0;
        }
        let arc_self = std::sync::Arc::new(self.clone());
        for pb in snapshot.iter() { arc_self.clone().spawn_generate_vision_description(pb.clone()); }
        snapshot.len()
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

    pub fn compute_file_hash(&self, path: &PathBuf) -> anyhow::Result<String, std::io::Error> {
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

    // Generate semantic (document) embeddings for provided file paths (if missing)
    pub async fn generate_semantic_for_paths(&self, paths: &[String]) -> usize {
        if self.ensure_document_table().await.is_err() { return 0; }
        let mut added = 0usize;
        for p in paths {
            // Find existing metadata
            let meta_opt = self.get_file_metadata(p).await;
            if let Some(meta) = meta_opt {
                if meta.embedding.is_some() { continue; }
                // Reindex forced so description stays untouched (manual mode may choose to skip description generation elsewhere)
                if let Err(e) = self.index_file_internal(meta.clone(), true).await { log::warn!("[AI] semantic embed failed for {}: {e}", p); } else { added += 1; }
            }
        }
        added
    }

    pub async fn generate_semantic_recursive(&self) -> usize {
        let targets: Vec<String> = {
            let files = self.files.lock().await;
            files.iter().filter(|f| f.embedding.is_none()).map(|f| f.path.clone()).collect()
        };
        self.generate_semantic_for_paths(&targets).await
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

    /// Update (or insert) a description for a file already tracked in self.files.
    /// Also persists (best-effort) to the cached thumbnail/metadata row if full metadata can be retrieved.
    pub async fn set_file_description(&self, path: &str, desc: &str) -> anyhow::Result<(), anyhow::Error> {
        {
            let mut files = self.files.lock().await;
            if let Some(entry) = files.iter_mut().find(|f| f.path == path) {
                entry.description = Some(desc.to_string());
            } else {
                // If we don't have it yet, create a minimal placeholder so enrichment won't re-trigger.
                files.push(super::FileMetadata {
                    id: None,
                    path: path.to_string(),
                    filename: std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
                    file_type: "other".into(),
                    size: 0,
                    modified: None,
                    created: None,
                    thumbnail_path: None,
                    thumb_b64: None,
                    hash: None,
                    description: Some(desc.to_string()),
                    caption: None,
                    tags: Vec::new(),
                    category: None,
                    text_content: None,
                    embedding: None,
                    similarity_score: None,
                    segments: None,
                    segment_objects: None,
                    object_counts: None,
                });
            }
        }
        // Persist updated row if full metadata available.
        if let Some(updated) = self.get_file_metadata(path).await {
            let _ = self.cache_thumbnail_and_metadata(&updated).await;
        }
        Ok(())
    }
}

impl super::AISearchEngine {
    /// Spawn a background task to generate a vision description for an image path.
    /// On success, updates in-memory metadata & persists (best-effort) without blocking caller.
    pub fn spawn_generate_vision_description(self: std::sync::Arc<Self>, path: std::path::PathBuf) {
        // Only spawn for existing image files.
        if !path.exists() { return; }
        // Fire-and-forget task.
        tokio::spawn(async move {
            let p_str = path.to_string_lossy().to_string();
            let start = std::time::Instant::now();
            match self.generate_vision_description(&path).await {
                Some(vd) => {
                    // Update metadata
                    {
                        let mut files = self.files.lock().await;
                        if let Some(f) = files.iter_mut().find(|f| f.path == p_str) {
                            f.description = Some(vd.description.clone());
                            f.caption = Some(vd.caption.clone());
                            f.tags = vd.tags.clone();
                            f.category = if vd.category.trim().is_empty() { None } else { Some(vd.category.clone()) };
                        }
                    }
                    if let Some(meta) = self.get_file_metadata(&p_str).await {
                        if let Err(e) = self.cache_thumbnail_and_metadata(&meta).await {
                            log::warn!("[AI] spawn persist failed for {}: {}", p_str, e);
                        }
                    }
                    log::info!("[AI] spawn vision description ok {} in {}ms", p_str, start.elapsed().as_millis());
                }
                None => {
                    log::warn!("[AI] spawn vision description returned None for {} ({}ms)", p_str, start.elapsed().as_millis());
                }
            }
        });
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
