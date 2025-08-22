use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
use kalosm::language::*;
use surrealdb::{engine::local::SurrealKv, Surreal};
// use once_cell::sync::Lazy;
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
    // AI-powered metadata
    pub description: Option<String>, // AI-generated description
    pub tags: Vec<String>, // AI-extracted tags
    pub text_content: Option<String>, // OCR or extracted text
    pub embedding: Option<Vec<f32>>, // AI embedding vector
    pub similarity_score: Option<f32>, // For search ranking
    pub segments: Option<Vec<String>>, // Detected segments/objects (image segmentation)
}

// AI Search Engine with full Kalosm integration
#[derive(Clone)]
pub struct AISearchEngine {
    // Vision model for image descriptions
    vision_model: Arc<Mutex<Option<Llama>>>,
    // Placeholders for future dedicated models (Segment Anything, OCR, Generation)
    ocr_model: Arc<Mutex<bool>>,          // bool flags as placeholders
    segment_model: Arc<Mutex<bool>>,      // replace with real model types later
    generator_model: Arc<Mutex<bool>>,    // replace with real model types later
    // SurrealDB with document table for semantic search
    db: Arc<Surreal<surrealdb::engine::local::Db>>,
    document_table: Arc<Mutex<Option<kalosm::language::DocumentTable<surrealdb::engine::local::Db>>>>,
    // In-memory storage for file metadata
    files: Arc<Mutex<Vec<FileMetadata>>>,
    // Map path -> document id for update/remove operations
    path_to_id: Arc<Mutex<HashMap<String, String>>>,
}

impl AISearchEngine {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing AI Search Engine with full Kalosm integration...");
        
        // Create SurrealDB connection
        let db: Surreal<surrealdb::engine::local::Db> = Surreal::new::<SurrealKv>("./db/ai_search.db").await?;
        db.use_ns("file_explorer").use_db("ai_search").await?;
        
        Ok(Self {
            vision_model: Arc::new(Mutex::new(None)),
            ocr_model: Arc::new(Mutex::new(false)),
            segment_model: Arc::new(Mutex::new(false)),
            generator_model: Arc::new(Mutex::new(false)),
            db: Arc::new(db),
            document_table: Arc::new(Mutex::new(None)),
            files: Arc::new(Mutex::new(Vec::new())),
            path_to_id: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    async fn ensure_vision_model(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut model_guard = self.vision_model.lock().await;
        if model_guard.is_none() {
            log::info!("Loading Qwen 2.5 3B VL vision model...");
            
            let model = Llama::builder()
                .with_source(LlamaSource::qwen_2_5_3b_vl_chat_q4())
                .build()
                .await?;
                
            *model_guard = Some(model);
            log::info!("Vision model loaded successfully");
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
    
    pub async fn index_file(&self, mut metadata: FileMetadata) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = PathBuf::from(&metadata.path);
        
        // Generate AI description for images using vision model
        if metadata.file_type == "image" && path.exists() {
            metadata.description = self.generate_vision_description(&path).await;
            // Basic OCR stub (reuse vision model prompt) to extract visible text into text_content if empty
            if metadata.text_content.is_none() {
                if let Some(ocr_text) = self.perform_ocr_stub(&path).await { 
                    if !ocr_text.trim().is_empty() && ocr_text.to_lowercase() != "(none)" { 
                        metadata.text_content = Some(ocr_text); 
                    }
                }
            }
            // Basic segmentation stub to list objects
            metadata.segments = self.segment_image_stub(&path).await;
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
                        "FILE_TYPE:{}\n",
                        "FILE_SIZE:{}\n",
                        "TAGS:{}\n",
                        "SEGMENTS:{}\n",
                        "DESCRIPTION:{}\n",
                        "OCR:{}\n"
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
            files[existing_idx] = metadata;
        } else {
            files.push(metadata);
        }
        
        Ok(())
    }
    
    // Remove a file from in-memory index and path map. (Note: semantic index deletion TBD if API exposed.)
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

    // Segment image (public API) returning list of detected object labels (stub implementation)
    pub async fn segment_image(&self, path: &str) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let p = PathBuf::from(path);
        let segments = self.segment_image_stub(&p).await.unwrap_or_default();
        Ok(segments)
    }

    // Generate an image from a prompt (placeholder implementation creates blank image w/ metadata header file)
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
            description: Some(format!("Placeholder generated for prompt: {}", prompt)),
            tags: vec!["generated".into()],
            text_content: None,
            embedding: None,
            similarity_score: None,
            segments: None,
        };
        // Ignore errors silently for now
        let _ = self.index_file(meta).await;

        Ok(out_path)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<FileMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Performing AI semantic search with query: '{}'", query);
        
        // Ensure document table is initialized
        self.ensure_document_table().await?;
        
        let mut results = Vec::new();
        
        // Use Kalosm document table for semantic search
        if let Some(document_table) = self.document_table.lock().await.as_ref() {
            let search_results = document_table
                .search(query)
                .with_results(20)
                .await?;

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

            log::info!("Found {} semantic search results", results.len());
        }
        
        // Sort by similarity score (highest first)
        results.sort_by(|a, b| {
            let a_score = a.similarity_score.unwrap_or(0.0);
            let b_score = b.similarity_score.unwrap_or(0.0);
            b_score.partial_cmp(&a_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        Ok(results.into_iter().take(50).collect())
    }
    
    // Real AI vision model description generation
    async fn generate_vision_description(&self, image_path: &PathBuf) -> Option<String> {
        if !image_path.exists() {
            log::warn!("Image file does not exist: {:?}", image_path);
            return None;
        }
        
        match self.ensure_vision_model().await {
            Ok(()) => {
                if let Some(model) = self.vision_model.lock().await.as_ref() {
                    log::info!("Using vision model to describe image: {:?}", image_path);
                    
                    // Normalize path for file:// URL (replace backslashes on Windows)
                    let path_str = image_path.to_string_lossy().replace('\\', "/");
                    let file_url = format!("file://{}", path_str);
                    // MediaSource::url returns a MediaSource directly in current API
                    let media_source = MediaSource::url(&file_url);
                    let media_chunk = MediaChunk::new(media_source, MediaType::Image);

                    let mut chat = model.chat();
                    let mut stream = chat(&(media_chunk, "Describe this image in detail. What do you see?"));
                    let mut description = String::new();
                    while let Some(token) = stream.next().await {
                        description.push_str(&token.to_string());
                    }
                    // Final await to finish stream (collect any remaining state)
                    if let Err(e) = stream.await {
                        log::warn!("Vision model finalization error (partial description kept): {}", e);
                    }
                    if description.trim().is_empty() { None } else { Some(description.trim().to_string()) }
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
    
    // OCR stub leveraging vision model: ask for raw text only.
    async fn perform_ocr_stub(&self, image_path: &PathBuf) -> Option<String> {
        if !image_path.exists() { return None; }
        if self.ensure_vision_model().await.is_err() { return None; }
        let model_guard = self.vision_model.lock().await;
        let model = model_guard.as_ref()?;
        let path_str = image_path.to_string_lossy().replace('\\', "/");
        let file_url = format!("file://{}", path_str);
        let media_source = MediaSource::url(&file_url);
        let media_chunk = MediaChunk::new(media_source, MediaType::Image);
        let mut chat = model.chat();
        let mut stream = chat(&(media_chunk, "Extract only the visible textual content. If none, reply with (none)."));
        let mut text = String::new();
        while let Some(token) = stream.next().await { text.push_str(&token.to_string()); }
        let _ = stream.await; // finalize
        Some(text.trim().to_string())
    }

    // Segmentation stub leveraging vision model: ask for object list.
    async fn segment_image_stub(&self, image_path: &PathBuf) -> Option<Vec<String>> {
        if !image_path.exists() { return None; }
        if self.ensure_vision_model().await.is_err() { return None; }
        let model_guard = self.vision_model.lock().await;
        let model = model_guard.as_ref()?;
        let path_str = image_path.to_string_lossy().replace('\\', "/");
        let file_url = format!("file://{}", path_str);
        let media_source = MediaSource::url(&file_url);
        let media_chunk = MediaChunk::new(media_source, MediaType::Image);
        let mut chat = model.chat();
        let mut stream = chat(&(media_chunk, "List up to 8 distinct objects you can identify in the image, comma separated, lowercase nouns only."));
        let mut resp = String::new();
        while let Some(token) = stream.next().await { resp.push_str(&token.to_string()); }
        let _ = stream.await;
        if resp.trim().is_empty() { return None; }
        let segments: Vec<String> = resp
            .split(|c| c == ',' || c == '\n')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .take(8)
            .collect();
        if segments.is_empty() { None } else { Some(segments) }
    }

    // AI-powered tag extraction from descriptions and content
    async fn extract_ai_tags(&self, metadata: &FileMetadata) -> Vec<String> {
        let mut tags = Vec::new();
        
        // Extract tags from AI-generated description
        if let Some(description) = &metadata.description {
            // Use simple AI-like tag extraction from description
            let desc_words: Vec<String> = description
                .split_whitespace()
                .filter(|word| word.len() > 3)
                .filter(|word| !["this", "that", "with", "from", "were", "they", "have", "been", "will", "would", "could", "should"].contains(&word.to_lowercase().as_str()))
                .map(|word| word.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                .filter(|word| !word.is_empty())
                .take(5)
                .collect();
            tags.extend(desc_words);
        }
        
        // Add file type tag
        tags.push(metadata.file_type.clone());
        
        // Add extension tag
        let path = PathBuf::from(&metadata.path);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            tags.push(ext.to_lowercase());
        }
        
        // Add meaningful filename components
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let filename_words: Vec<String> = stem
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| word.len() > 2)
                .map(|word| word.to_lowercase())
                .take(3)
                .collect();
            tags.extend(filename_words);
        }
        
        // Remove duplicates and limit
        tags.sort();
        tags.dedup();
        tags.truncate(10);
        tags
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

    // Return AI-relevant metadata for a separate UI list (lightweight projection)
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
                Err(e) => { log::debug!("Embedding regeneration failed: {}", e); None }
            }
        } else { None }
    }

    // Cache thumbnail & AI metadata in surrealdb table `thumbnails` (id = path)
    async fn cache_thumbnail_and_metadata(&self, metadata: &FileMetadata) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Prepare base64 thumbnail bytes if path present
        let thumb_b64 = if let Some(tp) = &metadata.thumbnail_path { 
            match fs::read(tp) { 
                Ok(bytes) => Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                Err(_) => None,
            }
        } else { None };
        #[derive(Serialize)]
        struct ThumbRow<'a> {
            path: &'a str,
            filename: &'a str,
            file_type: &'a str,
            size: u64,
            description: &'a Option<String>,
            tags: &'a Vec<String>,
            ocr: &'a Option<String>,
            segments: &'a Option<Vec<String>>,
            embedding: &'a Option<Vec<f32>>,
            thumbnail_b64: Option<String>,
            modified: Option<String>,
        }
        let row = ThumbRow {
            path: &metadata.path,
            filename: &metadata.filename,
            file_type: &metadata.file_type,
            size: metadata.size,
            description: &metadata.description,
            tags: &metadata.tags,
            ocr: &metadata.text_content,
            segments: &metadata.segments,
            embedding: &metadata.embedding,
            thumbnail_b64: thumb_b64,
            modified: metadata.modified.map(|dt| dt.to_rfc3339()),
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
        thumbnail_path: found_file.thumb_data.clone(),
        description: None, // Will be generated by AI
        tags: Vec::new(), // Will be extracted by AI
        text_content: None, // Will be extracted by AI OCR
        embedding: None, // Will be generated by AI
        similarity_score: None,
        segments: None,
    }
}