use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
use kalosm::language::*;
use surrealdb::{engine::local::SurrealKv, Surreal};
use tokio::time::{timeout, Duration};
// pub static DATABASE: Lazy<Surreal<SurrealKv>> = Lazy::new(Surreal::init);

// NOTE: We purposefully use the kalosm::language::Document type, not a custom one.
// Each indexed file is turned into a Document whose title is the filename and whose body
// starts with metadata lines we can parse back (FILE_PATH / FILE_TYPE) followed by
// the searchable content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: Option<String>,
    pub path: String,
    pub filename: String,
    pub file_type: String,
    pub size: u64,
    pub modified: Option<chrono::DateTime<chrono::Local>>,
    pub created: Option<chrono::DateTime<chrono::Local>>,
    pub thumbnail_path: Option<String>,
    // In-memory/base64 thumbnail data (data URL) when available. This lets AI search results
    // render thumbnails even if the underlying FoundFile list isn't currently visible.
    pub thumb_b64: Option<String>,
    // BLAKE3 hex hash of file contents to detect if content changed and re-embedding is needed.
    pub hash: Option<String>,
    // AI-powered metadata
    pub description: Option<String>, // AI-generated description
    pub tags: Vec<String>, // AI-extracted tags
    pub text_content: Option<String>, // OCR or extracted text
    pub embedding: Option<Vec<f32>>, // AI embedding vector
    pub similarity_score: Option<f32>, // For search ranking
    pub segments: Option<Vec<String>>, // Detected segments/objects (image segmentation)
    pub segment_objects: Option<Vec<SegmentObject>>, // detailed objects w/ confidence
    pub object_counts: Option<HashMap<String, u32>>, // aggregated label counts (normalized singular)
}

// Detailed segmentation object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentObject {
    pub label: String,
    pub confidence: f32,
    pub bbox: Option<[f32;4]>, // normalized x,y,w,h
}

// (Removed separate OCR & segmentation engine traits; unified vision model handles descriptions.)

#[derive(Serialize, Deserialize)]
pub struct ThumbRow {
    // Own all string data so we can build rows from ephemeral metadata without lifetime issues
    path: String,
    filename: String,
    file_type: String,
    size: u64,
    description: Option<String>,
    tags: Vec<String>,
    ocr: Option<String>,
    segments: Option<Vec<String>>,
    embedding: Option<Vec<f32>>,
    thumbnail_b64: Option<String>,
    modified: Option<String>,
    hash: Option<String>,
}

// AI Search Engine with full Kalosm integration
#[derive(Clone)]
pub struct AISearchEngine {
    vision_model: Arc<Mutex<Option<Llama>>>,
    db: Arc<Surreal<surrealdb::engine::local::Db>>,
    document_table: Arc<Mutex<Option<kalosm::language::DocumentTable<surrealdb::engine::local::Db>>>>,
    files: Arc<Mutex<Vec<FileMetadata>>>,
    path_to_id: Arc<Mutex<HashMap<String, String>>>,
    indexing_in_progress: Arc<Mutex<HashMap<String, usize>>>, // path -> reentry count
}

// (Removed stub OCR & segmentation engines.)

impl AISearchEngine {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing AI Search Engine with full Kalosm integration...");
        
        // Create SurrealDB connection
        let db: Surreal<surrealdb::engine::local::Db> = Surreal::new::<SurrealKv>("./db/ai_search.db").await?;
        db.use_ns("file_explorer").use_db("ai_search").await?;
        // let vision_model = match Llama::builder().with_source(LlamaSource::qwen_2_5_32b_vl_chat_q4()).build().await { // qwen_2_5_7b_vl_chat_f16
        //     Ok(model) => {
        //         log::info!("[AI] Vision model qwen_2_5_32b_vl_chat_f16 loaded successfully");
        //         Arc::new(Mutex::new(Some(model)))
        //     }
        //     Err(e) => {
        //         log::error!("[AI] Failed to load qwen_2_5_32b_vl_chat_f16 model ({e})");
        //         Arc::new(Mutex::new(None))
        //     }
        // };
        Ok(Self {
            vision_model: Arc::new(Mutex::new(None)),
            db: Arc::new(db),
            document_table: Arc::new(Mutex::new(None)),
            files: Arc::new(Mutex::new(Vec::new())),
            path_to_id: Arc::new(Mutex::new(HashMap::new())),
            indexing_in_progress: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    // Load previously cached thumbnail/meta rows from Surreal into memory so we don't
    // re-index (and especially don't re-run expensive vision description) every launch.
    // Returns number of records loaded.
    pub async fn load_cached(&self) -> usize {
        #[derive(Deserialize)]
        struct CachedRow {
            path: String,
            filename: String,
            file_type: String,
            size: u64,
            description: Option<String>,
            tags: Vec<String>,
            ocr: Option<String>,
            segments: Option<Vec<String>>,
            embedding: Option<Vec<f32>>,
            thumbnail_b64: Option<String>,
            modified: Option<String>,
            hash: Option<String>,
        }
        let rows: Result<Vec<CachedRow>, _> = self.db.select("thumbnails").await;
        let mut loaded = 0usize;
        match rows {
            Ok(list) => {
                if list.is_empty() { return 0; }
                let mut files_guard = self.files.lock().await;
                for r in list.into_iter() {
                    // Attempt parse of modified timestamp
                    let modified_dt = r.modified.as_ref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.with_timezone(&chrono::Local));
                    let meta = FileMetadata {
                        id: None, // document id not restored (not needed for search mapping)
                        path: r.path.clone(),
                        filename: r.filename.clone(),
                        file_type: r.file_type.clone(),
                        size: r.size,
                        modified: modified_dt,
                        created: modified_dt, // fallback
                        // We persist only the base64 thumbnail (thumbnail_b64). Older rows may have stored
                        // a path in thumbnail_b64 erroneously if a previous bug existed; we detect a likely
                        // data URL by prefix. If it's not a data URL we keep it in thumbnail_path so later
                        // code can try to load & convert it.
                        thumbnail_path: r.thumbnail_b64.as_ref().and_then(|s| if s.starts_with("data:image") { None } else { Some(s.clone()) }),
                        thumb_b64: r.thumbnail_b64.as_ref().and_then(|s| if s.starts_with("data:image") { Some(s.clone()) } else { None }),
                        hash: r.hash.clone(),
                        description: r.description.clone(),
                        tags: r.tags.clone(),
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

    // Return list of currently loaded (cached) file paths.
    pub async fn list_indexed_paths(&self) -> Vec<String> {
        let files = self.files.lock().await;
        files.iter().map(|f| f.path.clone()).collect()
    }
    // (Removed pluggable engine setup; only Qwen VL model is used.)
    
    pub async fn ensure_vision_model(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut model_guard = self.vision_model.lock().await;
        if model_guard.is_none() {
            log::info!("[AI] Loading qwen_2_5_32b_vl_chat_f16");
            // Attempt large model first with timeout to avoid hanging silently
            match Llama::builder()
                .with_flash_attn(true)
                .with_source(LlamaSource::deepseek_r1_distill_llama_8b()
                    // LlamaSource::new(FileSource::Local(
                    //     r#"C:\Users\darkm\AppData\Roaming\kalosm\cache\ggml-org\Qwen2.5-VL-32B-Instruct-GGUF\main\Qwen2.5-VL-32B-Instruct-Q4_K_M.gguf"#
                    // ))
                )
                .build()
                .await
            { // qwen_2_5_7b_vl_chat_f16
                Ok(model) => {
                    *model_guard = Some(model);
                    log::info!("[AI] Vision model qwen_2_5_32b_vl_chat_f16 loaded successfully");
                }
                Err(e) => log::error!("[AI] Failed to load qwen_2_5_32b_vl_chat_f16 model ({e})")
            }
        }
        Ok(())
    }
    
    async fn ensure_document_table(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut table_guard = self.document_table.lock().await;
        if table_guard.is_none() {
            log::info!("Initializing document table for semantic search...");
            
            let chunker = SemanticChunker::new();
            let document_table = self.db
                .document_table_builder("file_documents")
                .with_chunker(chunker)
                .at("./db/file_embeddings.db")
                .build::<kalosm::language::Document>()
                .await?;
                
            *table_guard = Some(document_table);
            log::info!("Document table initialized successfully");
        }
        Ok(())
    }
    
    // Internal generalized indexer with optional force flag (bypass hash/description skip logic)
    async fn index_file_internal(&self, mut metadata: FileMetadata, force: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Reentrancy / duplicate guard
        {
            let mut guard = self.indexing_in_progress.lock().await;
            if let Some(count) = guard.get_mut(&metadata.path) {
                *count += 1;
                log::warn!("[AI] Skipping duplicate indexing request for {} (active reentry count={})", metadata.path, count);
                return Ok(());
            } else {
                guard.insert(metadata.path.clone(), 1);
            }
        }
        let path = PathBuf::from(&metadata.path);
        log::info!("Indexing file: {} (type: {})", metadata.path, metadata.file_type);
        // Compute hash to detect changes
        metadata.hash = self.compute_file_hash(&path).ok();
        if !force {
            if let Some(existing) = self.get_file_metadata(&metadata.path).await {
                if existing.hash.is_some() && existing.hash == metadata.hash {
                    if existing.description.is_some() || metadata.file_type != "image" {
                        log::info!("[AI] Skipping re-index (unchanged hash) for {}", metadata.path);
                        return Ok(());
                    }
                }
            }
        }
        // Generate AI description for images using the Qwen VL model
        if metadata.file_type == "image" && path.exists() {
            log::info!("[AI] Generating description inline during indexing for {}", metadata.path);
            let start = std::time::Instant::now();
            metadata.description = self.generate_vision_description(&path).await;
            let ms = start.elapsed().as_millis();
            match &metadata.description {
                Some(d) => log::info!("[AI] Description generated ({} chars, {} ms) for {}", d.len(), ms, metadata.path),
                None => log::warn!("[AI] Description generation returned None for {} ({} ms)", metadata.path, ms),
            }
        }
        // Normalize thumbnail fields: If thumb_b64 already contains a data URL, leave it.
        // If thumbnail_path references an on-disk file (not data URL) and we lack thumb_b64, encode it.
        if metadata.thumb_b64.as_ref().map(|s| s.starts_with("data:image")).unwrap_or(false) == false {
            if let Some(tp) = &metadata.thumbnail_path {
                if !tp.starts_with("data:image") {
                    if let Ok(bytes) = fs::read(tp) {
                        metadata.thumb_b64 = Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
                    }
                } else if metadata.thumb_b64.is_none() {
                    // Mis-assigned earlier code may have put data URL into thumbnail_path
                    metadata.thumb_b64 = Some(tp.clone());
                }
            }
        }
        
        // Extract AI-powered tags from description and content
        metadata.tags = self.extract_ai_tags(&metadata).await;
        
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
                        "TAGS:{}\n",
                        "SEGMENTS:{}\n",
                        "DESCRIPTION:{}\n",
                        "OCR:{}\n"
                    ),
                    metadata.path,
                    metadata.hash.clone().unwrap_or_default(),
                    metadata.file_type,
                    metadata.size,
                    metadata.tags.join("|"),
                    metadata.segments.clone().map(|v| v.join("|")).unwrap_or_default(),
                    metadata.description.clone().unwrap_or_default().replace('\n', " "),
                    metadata.text_content.clone().unwrap_or_default().replace('\n', " "),
                );
                let body = format!("{}\n{}", header, searchable_content);
                log::info!("Body: {body}");
                let doc = kalosm::language::Document::from_parts(metadata.filename.clone(), body);
                match document_table.insert(doc).await {
                    Ok(id) => {
                        metadata.id = Some(format!("{}", id));
                        if let Some(id_str) = &metadata.id { 
                            self.path_to_id.lock().await.insert(metadata.path.clone(), id_str.clone());
                            // Attempt to fetch raw embedding via embedding_model for caching
                            if let Some(embed) = self.try_get_embedding(document_table, id_str).await { metadata.embedding = Some(embed); }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to insert document into semantic index: {}", e);
                    }
                }
            }
        }
        
        // Cache thumbnail & AI metadata in Surreal (best-effort)
        if let Err(e) = self.cache_thumbnail_and_metadata(&metadata).await { log::warn!("Thumbnail cache failed: {}", e); }

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

    pub async fn index_file(&self, metadata: FileMetadata) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.index_file_internal(metadata, false).await
    }

    // Force reindex a path even if hash unchanged (refresh description & tags)
    pub async fn force_reindex_path(&self, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(existing) = self.get_file_metadata(path).await {
            let mut meta = existing.clone();
            // Clear description so a fresh one is generated
            meta.description = None;
            self.index_file_internal(meta, true).await
        } else {
            Err("File not previously indexed".into())
        }
    }
    
    // Remove a file from in-memory index and path map. (Note: semantic index deletion TBD if API exposed.)
    #[allow(dead_code)]
    pub async fn remove_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let id_opt = { self.path_to_id.lock().await.remove(path) };
        {
            let mut files = self.files.lock().await;
            if let Some(idx) = files.iter().position(|f| f.path == path) { files.remove(idx); }
        }
        if let Some(doc_id) = id_opt {
            if let Some(table) = self.document_table.lock().await.as_ref() {
                if let Err(e) = table.delete(doc_id.clone()).await { log::warn!("Failed to delete document_table record {}: {}", doc_id, e); }
            }
        }
        Ok(())
    }

    // Reindex an existing file (remove then index again with fresh metadata)
    #[allow(dead_code)]
    pub async fn reindex_file(&self, metadata: FileMetadata) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // If existing id present, prefer update path
        if let Some(existing_id) = self.path_to_id.lock().await.get(&metadata.path).cloned() {
            self.ensure_document_table().await?;
            if let Some(table) = self.document_table.lock().await.as_ref() {
                // Rebuild document body similarly to index_file
                let searchable_content = self.get_searchable_text(&metadata);
                let header = format!(
                    concat!(
                        "FILE_PATH:{}\n","FILE_TYPE:{}\n","FILE_SIZE:{}\n","TAGS:{}\n","SEGMENTS:{}\n","DESCRIPTION:{}\n","OCR:{}\n"
                    ),
                    metadata.path,
                    metadata.file_type,
                    metadata.size,
                    metadata.tags.join("|"),
                    metadata.segments.clone().map(|v| v.join("|")).unwrap_or_default(),
                    metadata.description.clone().unwrap_or_default().replace('\n', " "),
                    metadata.text_content.clone().unwrap_or_default().replace('\n', " "),
                );
                let body = format!("{}\n{}", header, searchable_content);
                let doc = kalosm::language::Document::from_parts(metadata.filename.clone(), body);
                if let Err(e) = table.update(existing_id.clone(), doc).await { log::warn!("Update failed, falling back to full reindex: {}", e); }
                // refresh embedding cache
                if let Some(embed) = self.try_get_embedding(table, &existing_id).await { 
                    let mut files = self.files.lock().await; 
                    if let Some(idx) = files.iter().position(|f| f.path == metadata.path) { files[idx].embedding = Some(embed); }
                }
            }
            // Replace in-memory metadata
            let mut files = self.files.lock().await;
            if let Some(idx) = files.iter().position(|f| f.path == metadata.path) { files[idx] = metadata; }
            else { files.push(metadata); }
            Ok(())
        } else {
            self.index_file(metadata).await
        }
    }

    // (Removed segment_image public API; segmentation currently disabled.)

    // Generate an image from a prompt (placeholder implementation creates blank image w/ metadata header file)
    #[allow(dead_code)]
    pub async fn generate_image(&self, prompt: &str) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        use image::{ImageBuffer, Rgba};
        // Ensure directory
        let out_dir = PathBuf::from("./generated");
        if !out_dir.exists() { fs::create_dir_all(&out_dir)?; }
        // Create slug filename
        let slug = prompt
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ')
            .collect::<String>()
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join("_")
            .to_lowercase();
        let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
        let filename = if slug.is_empty() { format!("gen_{}.png", ts) } else { format!("{}_{}.png", slug, ts) };
        let out_path = out_dir.join(filename);
        // Simple gradient image as placeholder
        let imgx = 512; let imgy = 512;
        let mut imgbuf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(imgx, imgy);
        for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
            let r = (x as f32 / imgx as f32 * 255.0) as u8;
            let g = (y as f32 / imgy as f32 * 255.0) as u8;
            let b = 128u8;
            *pixel = Rgba([r, g, b, 255]);
        }
        imgbuf.save(&out_path)?;
        log::info!("Generated placeholder image: {:?} for prompt '{}'", out_path, prompt);

        // Index generated image metadata
        let meta = FileMetadata {
            id: None,
            path: out_path.to_string_lossy().to_string(),
            filename: out_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
            file_type: "image".to_string(),
            size: fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0),
            modified: Some(chrono::Local::now()),
            created: Some(chrono::Local::now()),
            thumbnail_path: None,
            thumb_b64: None,
            hash: self.compute_file_hash(&out_path).ok(),
            description: Some(format!("Placeholder generated for prompt: {}", prompt)),
            tags: vec!["generated".into()],
            text_content: None,
            embedding: None,
            similarity_score: None,
            segments: None,
            segment_objects: None,
            object_counts: None,
        };
        // Ignore errors silently for now
        let _ = self.index_file(meta).await;

        Ok(out_path)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<FileMetadata>, Box<dyn std::error::Error + Send + Sync>> {
    log::info!("[AI] Begin semantic search query='{}'", query);
        
        // Ensure document table is initialized
        self.ensure_document_table().await?;
        
    let mut results: Vec<FileMetadata> = Vec::new();
        
        // Use Kalosm document table for semantic search
        if let Some(document_table) = self.document_table.lock().await.as_ref() {
            let search_results = document_table
                .search(query)
                .with_results(20)
                .await?;
            log::debug!("[AI] Raw document_table search returned {} hits (pre-filter)", search_results.len());

            let files = self.files.lock().await;
            for search_result in search_results {
                let text = search_result.text();
                // Parse FILE_PATH line (first few lines) to recover original path
                let mut found_path: Option<String> = None;
                for line in text.lines().take(10) { // limit scan
                    if let Some(rest) = line.strip_prefix("FILE_PATH:") {
                        found_path = Some(rest.trim().to_string());
                        break;
                    }
                }
                if let Some(path) = found_path {
                    if let Some(file) = files.iter().find(|f| f.path == path) {
                        let mut file_with_score = file.clone();
                        file_with_score.similarity_score = Some(1.0 - search_result.distance.min(1.0));
                        results.push(file_with_score);
                    }
                } else {
                    log::warn!("Search result missing FILE_PATH header; skipping");
                }
            }

            // Dedupe by path keeping highest similarity
            use std::collections::HashMap as StdHashMap;
            let mut best: StdHashMap<String, FileMetadata> = StdHashMap::new();
            for r in results.drain(..) {
                let path = r.path.clone();
                match best.get(&path) {
                    Some(existing) => {
                        let es = existing.similarity_score.unwrap_or(0.0);
                        let rs = r.similarity_score.unwrap_or(0.0);
                        if rs > es { best.insert(path, r); }
                    }
                    None => { best.insert(path, r); }
                }
            }
            results = best.into_values().collect();
            log::info!("[AI] Mapped {} unique hits to file metadata", results.len());
        }
        
        // Sort by similarity score (highest first)
        results.sort_by(|a, b| {
            let a_score = a.similarity_score.unwrap_or(0.0);
            let b_score = b.similarity_score.unwrap_or(0.0);
            b_score.partial_cmp(&a_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        log::debug!("[AI] Post-sort top score={:?}", results.first().and_then(|f| f.similarity_score));
        log::info!("[AI] Search complete query='{}' final_results={}", query, results.len());
        
        Ok(results.into_iter().take(50).collect())
    }

    // Background enrichment: generate descriptions for any previously indexed images that are missing one.
    pub async fn enrich_missing_descriptions(&self) -> usize {
        let mut generated = 0usize;
        // Clone list of indices to avoid holding lock while generating each description.
        let snapshot: Vec<String> = {
            let files = self.files.lock().await;
            files.iter()
                .filter(|f| f.file_type == "image" && (f.description.is_none() || f.description.as_ref().map(|d| d.trim().len() < 12).unwrap_or(true)))
                .map(|f| f.path.clone())
                .collect()
        };
        if snapshot.is_empty() { return 0; }
        log::info!("[AI] Enriching descriptions for {} images (missing or too short)", snapshot.len());
        if let Err(e) = self.ensure_vision_model().await { log::error!("Failed to load vision model for enrichment: {}", e); return 0; }
        for path in snapshot {
            let pb = PathBuf::from(&path);
            if !pb.exists() { continue; }
            log::info!("[AI] Enrichment generating description for {}", path);
            if let Some(desc) = self.generate_vision_description(&pb).await {
                // Update in-memory
                {
                    let mut files = self.files.lock().await;
                    if let Some(f) = files.iter_mut().find(|f| f.path == path) {
                        f.description = Some(desc.clone());
                        // Refresh tags based on new description
                        f.tags = self.extract_ai_tags(f).await;
                        log::info!("[AI] Enrichment stored description ({} chars) for {}", desc.len(), f.path);
                    }
                }
                // Persist updated metadata (best-effort)
                if let Some(updated) = self.get_file_metadata(&path).await {
                    if let Err(e) = self.cache_thumbnail_and_metadata(&updated).await { log::warn!("Failed to update cached row for {}: {}", path, e); }
                }
                generated += 1;
            }
        }
        log::info!("[AI] Description enrichment complete (generated {})", generated);
        generated
    }

    // Count images lacking a sufficiently descriptive caption.
    pub async fn count_missing_descriptions(&self) -> usize {
        let files = self.files.lock().await;
        files.iter().filter(|f| f.file_type == "image" && (f.description.is_none() || f.description.as_ref().map(|d| d.trim().len() < 12).unwrap_or(true))).count()
    }

    // Generate (or regenerate if force) description for a single path without re-indexing document table.
    pub async fn generate_description_for_path(&self, path: &str, force: bool) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let pb = PathBuf::from(path);
        if !pb.exists() { return Ok(None); }
        {
            let files = self.files.lock().await;
            if !force {
                if let Some(f) = files.iter().find(|f| f.path == path) {
                    if f.description.is_some() && f.description.as_ref().map(|d| d.trim().len() >= 12).unwrap_or(false) {
                        return Ok(f.description.clone());
                    }
                }
            }
        }
        if let Some(desc) = self.generate_vision_description(&pb).await {
            // Update & persist
            if let Some(mut meta) = self.get_file_metadata(path).await {
                meta.description = Some(desc.clone());
                meta.tags = self.extract_ai_tags(&meta).await;
                // Replace existing metadata in-memory
                {
                    let mut files = self.files.lock().await;
                    if let Some(idx) = files.iter().position(|f| f.path == path) { files[idx] = meta.clone(); }
                }
                if let Err(e) = self.cache_thumbnail_and_metadata(&meta).await { log::warn!("Failed to persist updated description for {}: {}", path, e); }
            }
            Ok(Some(desc))
        } else {
            Ok(None)
        }
    }

    #[allow(dead_code)]
    pub async fn vision_model_loaded(&self) -> bool {
        self.vision_model.lock().await.is_some()
    }

    fn compute_file_hash(&self, path: &PathBuf) -> Result<String, std::io::Error> {
        use std::io::Read;
        if !path.exists() { return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found")); }
        let mut file = std::fs::File::open(path)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 { break; }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }
    
    // AI vision model description generation
    async fn generate_vision_description(&self, image_path: &PathBuf) -> Option<String> {
        if !image_path.exists() {
            log::warn!("Image file does not exist: {:?}", image_path);
            return None;
        }
        
        match self.ensure_vision_model().await {
            Ok(()) => {
                if let Some(model) = self.vision_model.lock().await.as_ref() {
                    log::info!("[AI] Vision model describing image: {:?}", image_path);
                    
                    // Read image bytes directly (avoid file:// URL fetch issues on Windows)
                    let bytes = match std::fs::read(image_path) {
                        Ok(b) => b,
                        Err(e) => { log::warn!("Failed reading image bytes for {:?}: {}", image_path, e); return None; }
                    };
                    let media_source = MediaSource::bytes(bytes);
                    let media_chunk = MediaChunk::new(media_source, MediaType::Image);

                    let mut chat = model.chat();
                    let mut stream = chat(&(media_chunk, "Describe this image in detail. Provide a concise natural language caption (<= 40 words)."));
                    let mut description = String::new();
                    while let Some(token) = stream.next().await {
                        description.push_str(&token.to_string());
                    }
                    // Final await to finish stream (collect any remaining state)
                    if let Err(e) = stream.await {
                        log::warn!("Vision model finalization error (partial description kept): {}", e);
                    }
                    if description.trim().is_empty() {
                        log::warn!("Vision model returned empty description for {:?}; retrying with alternate prompt", image_path);
                        // Retry once with alternate wording
                        // Reconstruct media chunk for retry (previous media_chunk was moved into first chat stream)
                        // Re-read bytes (cheap relative to model invocation; could reuse above via Arc clone if refactored)
                        let retry_bytes = match std::fs::read(image_path) {
                            Ok(b) => b,
                            Err(e) => { log::warn!("Retry read failed for {:?}: {}", image_path, e); Vec::new() }
                        };
                        let media_chunk2 = MediaChunk::new(MediaSource::bytes(retry_bytes), MediaType::Image);
                        let mut chat2 = model.chat();
                        let mut stream2 = chat2(&(media_chunk2, "Caption the image succinctly."));
                        let mut retry = String::new();
                        while let Some(token) = stream2.next().await { retry.push_str(&token.to_string()); }
                        if let Err(e) = stream2.await { log::warn!("Retry finalization error: {}", e); }
                        if retry.trim().is_empty() { log::error!("[AI] Retry also empty for {:?}", image_path); None } else { log::info!("[AI] Retry produced {} chars for {:?}", retry.len(), image_path); Some(retry.trim().to_string()) }
                    } else { log::info!("[AI] Primary description {} chars for {:?}", description.trim().len(), image_path); Some(description.trim().to_string()) }
                } else {
                    log::error!("Vision model not loaded");
                    None
                }
            }
            Err(e) => {
                log::error!("Failed to ensure vision model: {}", e);
                None
            }
        }
    }
    
    // (Removed OCR / segmentation helpers.)

    // AI-powered tag extraction ONLY (no heuristic filename/ext/filetype tags)
    // Strategy:
    // 1. If we already have an AI description, prompt the vision (multimodal) model in text-only mode
    //    to convert that description into up to 8 concise, lowercase search tags.
    // 2. If no description and this is an image, we do NOT attempt heuristics here; description
    //    generation happens elsewhere, so we return an empty vector (caller can re-run after description generation).
    // 3. For non-image files without an AI description, return empty.
    async fn extract_ai_tags(&self, metadata: &FileMetadata) -> Vec<String> {
        // Must have some semantic description to base tags on
        let Some(description) = &metadata.description else { return Vec::new(); };

        // Ensure model is loaded (we reuse the same multimodal model for a pure text prompt)
        if let Err(e) = self.ensure_vision_model().await {
            log::warn!("[AI] Cannot load vision model for tag generation: {}", e);
            return Vec::new();
        }
        let model_guard = self.vision_model.lock().await;
        let Some(model) = model_guard.as_ref() else {
            log::warn!("[AI] Vision model guard empty after ensure_vision_model");
            return Vec::new();
        };

        // Craft a focused instruction to minimize extraneous prose.
        let instruction = format!(
            "You are an assistant that extracts search tags. Given this description of an image or media item:\n\n{}\n\nReturn ONLY a comma-separated list of up to 8 concise, lowercase tags (single words or short hyphenated phrases). No explanations, no numbering.",
            description.replace('\n', " ")
        );

        let mut chat = model.chat();
        let mut stream = chat(&instruction.as_str());
        let mut raw = String::new();
        while let Some(tok) = stream.next().await { raw.push_str(&tok.to_string()); }
        if let Err(e) = stream.await { log::debug!("[AI] Tag stream finalize error (ignoring): {}", e); }

        // Parse comma-separated tags; enforce constraints.
        let tags: Vec<String> = raw
            .lines()
            .next() // take first line in case model added a newline
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .map(|s| s.replace(['#', '.', ';'], ""))
            .take(8)
            .collect();

        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        let mut deduped = Vec::with_capacity(tags.len());
        for t in tags { if seen.insert(t.clone()) { deduped.push(t); } }
        log::info!("[AI] Generated {} AI-only tags", deduped.len());
        deduped
    }
    
    fn get_searchable_text(&self, metadata: &FileMetadata) -> String {
        let mut text_parts = vec![
            metadata.filename.clone(),
        ];
        
        if let Some(desc) = &metadata.description {
            text_parts.push(desc.clone());
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
    
    pub async fn get_all_files(&self) -> Result<Vec<FileMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        let files = self.files.lock().await;
        Ok(files.clone())
    }

    pub async fn get_file_metadata(&self, path: &str) -> Option<FileMetadata> {
        let files = self.files.lock().await;
        files.iter().find(|f| f.path == path).cloned()
    }

    // Return AI-relevant metadata for a separate UI list (lightweight projection)
    #[allow(dead_code)]
    pub async fn get_ai_metadata(&self) -> Vec<(String, Option<String>, Vec<String>, Option<Vec<String>>)> {
        let files = self.files.lock().await;
        files.iter().map(|f| (f.path.clone(), f.description.clone(), f.tags.clone(), f.segments.clone())).collect()
    }

    // Try to pull embedding from underlying model/table if accessible. Placeholder: DocumentTable currently
    // doesn't expose direct per-record embedding in the public API we used, so this returns None until
    // extended. Implementors can adapt by querying table.embedding_model() if an accessor surfaces.
    async fn try_get_embedding(&self, table: &kalosm::language::DocumentTable<surrealdb::engine::local::Db>, _id: &str) -> Option<Vec<f32>> {
        // Attempt strategy: re-embed the searchable text using the table's embedding model.
        // We reconstruct approximate searchable content by selecting from in-memory metadata.
        // Note: This yields a single embedding for the whole file (not per chunk) for quick similarity preview UI.
        let files = self.files.lock().await;
        // Fallback: no per-id retrieval yet; we just ignore _id and derive from path match.
        // (We could parse path from headers if needed for more accuracy.)
        let embedding_model = table.embedding_model(); // assume API exists per docs you provided
        if files.is_empty() { return None; }
        // This simplified approach might not match the inserted chunk-level embeddings (which are chunked),
        // but provides a consistent vector for quick metadata display.
        // Just take the last file (recently inserted) – caller ensures correct ordering when calling after insert/update.
        if let Some(latest) = files.last() {
            let text = self.get_searchable_text(latest);
            match embedding_model.embed(text).await {
                Ok(emb) => Some(emb.vector().to_vec()),
                Err(e) => { log::info!("Embedding regeneration failed: {}", e); None }
            }
        } else { None }
    }

    // Cache thumbnail & AI metadata in surrealdb table `thumbnails` (id = path)
    async fn cache_thumbnail_and_metadata(&self, metadata: &FileMetadata) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Prefer existing in-memory base64 thumbnail if present; else attempt to read from on-disk path.
        let thumb_b64 = if let Some(b64) = &metadata.thumb_b64 { Some(b64.clone().trim().to_string()) } else if let Some(tp) = &metadata.thumbnail_path { 
            if tp.starts_with("data:image") { Some(tp.clone()) } else {
                match fs::read(tp) { 
                    Ok(bytes) => Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                    Err(_) => None,
                }
            }
        } else { None };

        let row = ThumbRow {
            path: metadata.path.clone(),
            filename: metadata.filename.clone(),
            file_type: metadata.file_type.clone(),
            size: metadata.size,
            description: metadata.description.clone(),
            tags: metadata.tags.clone(),
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
        let _: Option<ThumbRow> = self.db.create("thumbnails")
            .content::<ThumbRow>(row)
            .await?
            .take();
        Ok(())
    }
}

// Helper function to extract metadata from FoundFile
pub fn found_file_to_metadata(found_file: &crate::types::FoundFile) -> FileMetadata {
    FileMetadata {
        id: None,
        path: found_file.path.display().to_string(),
        filename: found_file.path.file_name()
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
        description: None, // Will be generated by AI
    tags: Vec::new(), // Will be extracted by AI
    text_content: None, // (OCR disabled; could repurpose for future text extraction)
        embedding: None, // Will be generated by AI
        similarity_score: None,
        segments: None,
        segment_objects: None,
        object_counts: None,
    }
}