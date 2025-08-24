pub mod ai_search;
pub mod index;
pub mod data_extraction;
pub mod generate;
pub mod cache;
// pub mod 
// pub mod gpt;

pub use ai_search::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    pub description: Option<String>,   // AI-generated description (multi-sentence)
    pub caption: Option<String>,       // Short caption/alt text
    pub tags: Vec<String>,             // AI-extracted tags
    pub text_content: Option<String>,  // OCR or extracted text
    pub embedding: Option<Vec<f32>>,   // AI embedding vector
    pub similarity_score: Option<f32>, // For search ranking
    pub segments: Option<Vec<String>>, // Detected segments/objects (image segmentation)
    pub segment_objects: Option<Vec<SegmentObject>>, // detailed objects w/ confidence
    pub object_counts: Option<std::collections::HashMap<String, u32>>, // aggregated label counts (normalized singular)
}

// Detailed segmentation object
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SegmentObject {
    pub label: String,
    pub confidence: f32,
    pub bbox: Option<[f32; 4]>, // normalized x,y,w,h
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ThumbRow {
    // Own all string data so we can build rows from ephemeral metadata without lifetime issues
    pub path: String,
    pub filename: String,
    pub file_type: String,
    pub size: u64,
    pub description: Option<String>,
    pub caption: Option<String>,
    pub tags: Vec<String>,
    pub ocr: Option<String>,
    pub segments: Option<Vec<String>>,
    pub embedding: Option<Vec<f32>>,
    pub thumbnail_b64: Option<String>,
    pub modified: Option<String>,
    pub hash: Option<String>,
}

// Lightweight projection for semantic document debug (id + first chars)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DebugDocumentSnippet {
    pub id: String,
    pub title: Option<String>,
    pub preview: String,
    pub len: usize,
}

// AI Search Engine with full Kalosm integration
#[derive(Clone)]
pub struct AISearchEngine {
    pub vision_model: std::sync::Arc<tokio::sync::Mutex<Option<kalosm::language::OpenAICompatibleChatModel>>>, // Llama
    // pub gpt_model: std::sync::Arc<tokio::sync::Mutex<Option<OpenAICompatibleChatModel>>>,
    pub db: std::sync::Arc<surrealdb::Surreal<surrealdb::engine::local::Db>>,
    pub document_table: std::sync::Arc<tokio::sync::Mutex<Option<kalosm::language::DocumentTable<surrealdb::engine::local::Db>>>>,
    pub files: std::sync::Arc<tokio::sync::Mutex<Vec<FileMetadata>>>,
    pub path_to_id: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
    pub indexing_in_progress: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, usize>>>, // path -> reentry count
}