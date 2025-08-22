use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Datelike;

// Metadata structure for files with AI features
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

// AI Search Engine with intelligent rule-based processing
#[derive(Clone)]
pub struct AISearchEngine {
    // In-memory storage for file metadata
    files: Arc<Mutex<Vec<FileMetadata>>>,
}

impl AISearchEngine {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing AI Search Engine with intelligent processing...");
        
        Ok(Self {
            files: Arc::new(Mutex::new(Vec::new())),
        })
    }
    
    pub async fn index_file(&self, mut metadata: FileMetadata) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = PathBuf::from(&metadata.path);
        
        // Generate AI metadata based on file type
        if metadata.file_type == "image" {
            metadata.description = self.generate_smart_image_description(&path).await;
            metadata.text_content = self.extract_text_from_filename(&path);
        }
        
        // Extract smart tags from filename and description
        metadata.tags = self.extract_smart_tags(&metadata).await;
        
        // Generate embedding for semantic search
        if let Some(searchable_text) = self.get_searchable_text(&metadata) {
            metadata.embedding = self.generate_text_embedding(&searchable_text).await;
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
        let files = self.files.lock().await;
        let query_lower = query.to_lowercase();
        
        // Enhanced text search with smart matching
        let mut results: Vec<FileMetadata> = files.iter()
            .filter(|file| {
                self.matches_smart_query(file, &query_lower)
            })
            .cloned()
            .collect();
            
        // Sort by relevance
        results.sort_by(|a, b| {
            let a_score = self.calculate_relevance_score(a, query);
            let b_score = self.calculate_relevance_score(b, query);
            b_score.partial_cmp(&a_score).unwrap_or(std::cmp::Ordering::Equal)
        });
            
        Ok(results.into_iter().take(50).collect())
    }
    
    pub async fn smart_search(&self, query: &str) -> Result<Vec<FileMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        // Enhanced search with smart query expansion
        let basic_results = self.search(query).await?;
        let expanded_query = self.expand_query_intelligently(query);
        
        if expanded_query != query {
            let enhanced_results = self.search(&expanded_query).await?;
            
            // Combine and deduplicate results
            let mut combined = basic_results;
            for result in enhanced_results {
                if !combined.iter().any(|r| r.path == result.path) {
                    combined.push(result);
                }
            }
            Ok(combined)
        } else {
            Ok(basic_results)
        }
    }
    
    // Smart AI-like processing methods
    async fn generate_smart_image_description(&self, image_path: &PathBuf) -> Option<String> {
        let filename = image_path.file_name()?.to_str()?.to_lowercase();
        let extension = image_path.extension()?.to_str()?.to_lowercase();
        
        // Rule-based description generation
        let mut description = String::new();
        
        if filename.contains("screenshot") || filename.contains("screen") {
            description.push_str("Screenshot or screen capture");
        } else if filename.contains("photo") || filename.contains("pic") || filename.contains("img") {
            description.push_str("Photograph or image");
        } else if filename.contains("diagram") || filename.contains("chart") {
            description.push_str("Diagram or chart");
        } else if filename.contains("logo") || filename.contains("icon") {
            description.push_str("Logo or icon");
        } else {
            match extension.as_str() {
                "jpg" | "jpeg" => description.push_str("JPEG image file"),
                "png" => description.push_str("PNG image file"),
                "gif" => description.push_str("GIF animated image"),
                "svg" => description.push_str("SVG vector image"),
                "bmp" => description.push_str("Bitmap image"),
                _ => description.push_str("Image file"),
            }
        }
        
        // Add context from parent directory
        if let Some(parent) = image_path.parent() {
            if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                let parent_lower = parent_name.to_lowercase();
                if parent_lower.contains("vacation") || parent_lower.contains("trip") {
                    description.push_str(" from vacation or trip");
                } else if parent_lower.contains("work") || parent_lower.contains("project") {
                    description.push_str(" related to work or project");
                } else if parent_lower.contains("family") || parent_lower.contains("personal") {
                    description.push_str(" of personal or family nature");
                }
            }
        }
        
        Some(description)
    }
    
    fn extract_text_from_filename(&self, path: &PathBuf) -> Option<String> {
        let filename = path.file_stem()?.to_str()?;
        
        // Extract meaningful text from filename
        let words: Vec<String> = filename
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.len() > 2)
            .map(|word| word.to_lowercase())
            .collect();
            
        if words.is_empty() {
            None
        } else {
            Some(words.join(" "))
        }
    }
    
    async fn extract_smart_tags(&self, metadata: &FileMetadata) -> Vec<String> {
        let mut tags = Vec::new();
        let path = PathBuf::from(&metadata.path);
        let filename_lower = metadata.filename.to_lowercase();
        
        // File type tags
        match metadata.file_type.as_str() {
            "image" => {
                tags.push("image".to_string());
                if filename_lower.contains("screenshot") || filename_lower.contains("screen") {
                    tags.push("screenshot".to_string());
                }
                if filename_lower.contains("photo") || filename_lower.contains("pic") {
                    tags.push("photo".to_string());
                }
            }
            "video" => {
                tags.push("video".to_string());
                if filename_lower.contains("recording") {
                    tags.push("recording".to_string());
                }
            }
            _ => {
                if filename_lower.contains("document") || filename_lower.contains("doc") {
                    tags.push("document".to_string());
                }
            }
        }
        
        // Date-based tags
        if let Some(modified) = metadata.modified {
            let year = modified.year();
            tags.push(year.to_string());
            
            let month_name = match modified.month() {
                1 => "january", 2 => "february", 3 => "march", 4 => "april",
                5 => "may", 6 => "june", 7 => "july", 8 => "august",
                9 => "september", 10 => "october", 11 => "november", 12 => "december",
                _ => "unknown"
            };
            tags.push(month_name.to_string());
        }
        
        // Context from path
        for component in path.components() {
            if let Some(name) = component.as_os_str().to_str() {
                let name_lower = name.to_lowercase();
                if name_lower.len() > 3 && !name_lower.starts_with('.') {
                    // Add meaningful directory names as tags
                    if ["documents", "photos", "pictures", "videos", "downloads", "desktop", 
                        "work", "personal", "projects", "family", "vacation", "trip"].contains(&name_lower.as_str()) {
                        tags.push(name_lower);
                    }
                }
            }
        }
        
        // Extension as tag
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            tags.push(ext.to_lowercase());
        }
        
        // Extract from description
        if let Some(description) = &metadata.description {
            let desc_words: Vec<String> = description
                .split_whitespace()
                .filter(|word| word.len() > 3)
                .map(|word| word.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                .filter(|word| !word.is_empty())
                .collect();
            tags.extend(desc_words.into_iter().take(3));
        }
        
        tags.truncate(15); // Limit to 15 tags
        tags.sort();
        tags.dedup();
        tags
    }
    
    fn matches_smart_query(&self, file: &FileMetadata, query_lower: &str) -> bool {
        // Basic text matching
        if file.filename.to_lowercase().contains(query_lower) ||
           file.description.as_ref().map_or(false, |d| d.to_lowercase().contains(query_lower)) ||
           file.tags.iter().any(|tag| tag.to_lowercase().contains(query_lower)) ||
           file.text_content.as_ref().map_or(false, |t| t.to_lowercase().contains(query_lower)) {
            return true;
        }
        
        // Smart query expansion
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        
        // Check for fuzzy matching on tags and descriptions
        for word in query_words {
            if word.len() > 3 {
                // Partial matching for longer words
                if file.tags.iter().any(|tag| tag.contains(word)) ||
                   file.description.as_ref().map_or(false, |d| d.to_lowercase().contains(word)) {
                    return true;
                }
            }
        }
        
        false
    }
    
    fn expand_query_intelligently(&self, query: &str) -> String {
        let mut expanded = query.to_lowercase();
        
        // Smart query expansion rules
        if expanded.contains("pic") && !expanded.contains("picture") {
            expanded = expanded.replace("pic", "pic picture photo image");
        }
        if expanded.contains("doc") && !expanded.contains("document") {
            expanded = expanded.replace("doc", "doc document");
        }
        if expanded.contains("vid") && !expanded.contains("video") {
            expanded = expanded.replace("vid", "vid video");
        }
        if expanded.contains("shot") && !expanded.contains("screenshot") {
            expanded = expanded.replace("shot", "shot screenshot");
        }
        
        expanded
    }
    
    async fn generate_text_embedding(&self, text: &str) -> Option<Vec<f32>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash_value = hasher.finish();
        
        // Generate a more sophisticated embedding based on text characteristics
        let mut embedding = vec![0.0; 128];
        let words: Vec<&str> = text.split_whitespace().collect();
        
        // Encode text features into embedding
        for (i, &word) in words.iter().enumerate().take(64) {
            let mut word_hasher = DefaultHasher::new();
            word.hash(&mut word_hasher);
            let word_hash = word_hasher.finish();
            embedding[i] = (word_hash % 256) as f32 / 255.0;
        }
        
        // Encode text length and characteristics
        embedding[64] = (text.len() as f32 / 1000.0).min(1.0);
        embedding[65] = words.len() as f32 / 100.0;
        
        // Fill remaining with hash-based values
        for i in 66..128 {
            embedding[i] = ((hash_value >> (i % 64)) & 1) as f32;
        }
        
        Some(embedding)
    }
    
    fn get_searchable_text(&self, metadata: &FileMetadata) -> Option<String> {
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
        
        if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join(" "))
        }
    }
    
    fn calculate_relevance_score(&self, file: &FileMetadata, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        let mut score = 0.0;
        
        // Exact filename match gets highest score
        if file.filename.to_lowercase().contains(&query_lower) {
            score += 10.0;
            
            // Boost if it's an exact filename match
            if file.filename.to_lowercase() == query_lower {
                score += 20.0;
            }
        }
        
        // Description match
        if let Some(desc) = &file.description {
            if desc.to_lowercase().contains(&query_lower) {
                score += 5.0;
            }
        }
        
        // Tag match
        for tag in &file.tags {
            if tag.to_lowercase().contains(&query_lower) {
                score += 3.0;
                
                // Exact tag match gets bonus
                if tag.to_lowercase() == query_lower {
                    score += 5.0;
                }
            }
        }
        
        // Text content match
        if let Some(content) = &file.text_content {
            if content.to_lowercase().contains(&query_lower) {
                score += 2.0;
            }
        }
        
        // Recency bonus (more recent files get slight boost)
        if let Some(modified) = file.modified {
            let days_old = chrono::Utc::now().signed_duration_since(modified).num_days();
            if days_old < 30 {
                score += 1.0;
            }
        }
        
        score
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

// Enhanced AI processing functions using intelligent rules
pub async fn generate_ai_description(file_path: &PathBuf) -> Option<String> {
    // This function can be called independently for processing single files
    // Create a temporary AI engine instance
    if let Ok(engine) = AISearchEngine::new().await {
        engine.generate_smart_image_description(file_path).await
    } else {
        None
    }
}

pub async fn extract_ai_tags(file_path: &PathBuf) -> Vec<String> {
    // Extract tags based on filename and any AI analysis
    let filename = file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let mut tags = Vec::new();
    
    // Basic rule-based extraction
    if filename.contains("screenshot") || filename.contains("screen") {
        tags.push("screenshot".to_string());
    }
    if filename.contains("photo") || filename.contains("img") || filename.contains("pic") {
        tags.push("photo".to_string());
    }
    if filename.contains("document") || filename.contains("doc") {
        tags.push("document".to_string());
    }
    if filename.contains("video") || filename.contains("mov") || filename.contains("mp4") {
        tags.push("video".to_string());
    }
    
    // Add file extension as a tag
    if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
        tags.push(ext.to_lowercase());
    }
    
    tags
}

pub async fn generate_embedding(content: &str) -> Option<Vec<f32>> {
    // Simple hash-based embedding for now
    // In production, this would use the language model's embedding capabilities
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash_value = hasher.finish();
    
    // Convert to a 128-dimensional embedding
    let mut embedding = vec![0.0; 128];
    for i in 0..128 {
        embedding[i] = ((hash_value >> (i % 64)) & 1) as f32;
    }
    
    Some(embedding)
}