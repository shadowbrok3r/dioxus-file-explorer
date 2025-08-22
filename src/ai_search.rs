use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use kalosm::language::*;
use surrealdb::{engine::local::SurrealKv, Surreal};

// Document structure for Kalosm embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub content: String,
    pub file_path: String,
    pub file_type: String,
}
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
}

// AI Search Engine with full Kalosm integration
#[derive(Clone)]
pub struct AISearchEngine {
    // Vision model for image descriptions
    vision_model: Arc<Mutex<Option<Llama>>>,
    // SurrealDB with document table for semantic search
    db: Arc<Surreal<SurrealKv>>,
    document_table: Arc<Mutex<Option<DocumentTable<SurrealKv, Document>>>>,
    // In-memory storage for file metadata
    files: Arc<Mutex<Vec<FileMetadata>>>,
}

impl AISearchEngine {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing AI Search Engine with full Kalosm integration...");
        
        // Create SurrealDB connection
        let db = Surreal::new::<SurrealKv>("./db/ai_search.db").await?;
        db.use_ns("file_explorer").use_db("ai_search").await?;
        
        Ok(Self {
            vision_model: Arc::new(Mutex::new(None)),
            db: Arc::new(db),
            document_table: Arc::new(Mutex::new(None)),
            files: Arc::new(Mutex::new(Vec::new())),
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
                .build::<Document>()
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
        }
        
        // Extract AI-powered tags from description and content
        metadata.tags = self.extract_ai_tags(&metadata).await;
        
        // Store file in document table for semantic search
        self.ensure_document_table().await?;
        if let Some(document_table) = self.document_table.lock().await.as_ref() {
            let searchable_content = self.get_searchable_text(&metadata);
            if !searchable_content.is_empty() {
                let document = Document {
                    content: searchable_content,
                    file_path: metadata.path.clone(),
                    file_type: metadata.file_type.clone(),
                };
                
                // Add document to semantic search index
                document_table.add_document(document).await?;
            }
        }
        
        // Store in memory (replace existing entry if same path)
        let mut files = self.files.lock().await;
        if let Some(existing_idx) = files.iter().position(|f| f.path == metadata.path) {
            files[existing_idx] = metadata;
        } else {
            files.push(metadata);
        }
        
        Ok(())
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
                if let Some(file) = files.iter().find(|f| f.path == search_result.document().file_path) {
                    let mut file_with_score = file.clone();
                    // Convert distance to similarity score (lower distance = higher similarity)
                    file_with_score.similarity_score = Some(1.0 - search_result.distance.min(1.0));
                    results.push(file_with_score);
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
                    
                    // Create MediaSource from file path
                    let file_url = format!("file://{}", image_path.to_string_lossy());
                    if let Ok(media_source) = MediaSource::url(&file_url) {
                        let media_chunk = MediaChunk::new(media_source, MediaType::Image);
                        
                        // Use vision model to generate description
                        let mut chat = model.chat();
                        let prompt = (media_chunk, "Describe this image in detail. What do you see?");
                        
                        match chat(&prompt).await {
                            Ok(mut response) => {
                                // Collect the response
                                let mut description = String::new();
                                while let Some(token) = response.next().await {
                                    description.push_str(&token.to_string());
                                }
                                log::info!("Successfully generated AI description for image");
                                Some(description)
                            }
                            Err(e) => {
                                log::error!("Failed to generate AI description: {}", e);
                                None
                            }
                        }
                    } else {
                        log::error!("Failed to create media source from path: {:?}", image_path);
                        None
                    }
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
        
        text_parts.extend(metadata.tags.clone());
        
        text_parts.join(" ")
    }
    
    pub async fn get_all_files(&self) -> Result<Vec<FileMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        let files = self.files.lock().await;
        Ok(files.clone())
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
    }
}