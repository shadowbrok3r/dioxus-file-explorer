pub mod ai_search;
pub mod index;
pub mod data_extraction;
pub mod generate;
pub mod cache;
#[cfg(feature = "joycaption")]
pub mod joycaption_adapter;
#[cfg(feature = "joycaption")]
#[path = "candle-llava/mod.rs"]
#[cfg(feature = "joycaption")]
pub mod candle_llava;
// pub mod 
// pub mod gpt;

pub use ai_search::*;

// Shared Arc alias for ergonomics while we transition more async background tasks.
pub type SharedAISearchEngine = std::sync::Arc<AISearchEngine>;

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
    pub category: Option<String>,      // Single high-level AI category
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
    pub category: Option<String>,
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
    pub vision_model: std::sync::Arc<tokio::sync::Mutex<Option<kalosm::language::Llama>>>, // Llama
    // pub gpt_model: std::sync::Arc<tokio::sync::Mutex<Option<OpenAICompatibleChatModel>>>,
    pub db: std::sync::Arc<surrealdb::Surreal<surrealdb::engine::local::Db>>,
    pub document_table: std::sync::Arc<tokio::sync::Mutex<Option<kalosm::language::DocumentTable<surrealdb::engine::local::Db>>>>,
    pub files: std::sync::Arc<tokio::sync::Mutex<Vec<FileMetadata>>>,
    pub path_to_id: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
    pub indexing_in_progress: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, usize>>>, // path -> reentry count
    

    // Control flags for manual vs automatic behaviors
    pub auto_descriptions_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,

    // Async indexing queue (fire-and-forget). UI enqueues metadata; background worker performs heavy work on Tokio runtime.
    pub index_tx: std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<FileMetadata>>>>,
    // Progress metrics (atomics for cheap cross-thread reads)
    pub index_queue_len: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub index_active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub index_completed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl AISearchEngine {
    /// Start background indexing worker if not already started.
    pub async fn ensure_index_worker(&self) {
        let mut guard = self.index_tx.lock().await;
        if guard.is_some() { return; }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<FileMetadata>();
        *guard = Some(tx);
        let engine = self.clone();
        tokio::spawn(async move {
            while let Some(meta) = rx.recv().await {
                let path = meta.path.clone();
                engine.index_active.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Err(e) = engine.index_file(meta).await {
                    log::warn!("[AI] queue index failed for {}: {}", path, e);
                }
                engine.index_active.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                engine.index_completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // Decrement queue len (saturating)
                engine.index_queue_len.fetch_update(std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed, |v| Some(v.saturating_sub(1))).ok();
            }
            log::info!("[AI] indexing worker channel closed");
        });
    }

    /// Enqueue a file metadata record for background indexing. Returns false if queue not ready yet.
    pub async fn enqueue_index(&self, meta: FileMetadata) -> bool {
        if self.index_tx.lock().await.is_none() { self.ensure_index_worker().await; }
        let sent = if let Some(tx) = self.index_tx.lock().await.as_ref() { tx.send(meta).is_ok() } else { false };
        if sent { self.index_queue_len.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
        sent
    }

    #[cfg(feature = "joycaption")]
    pub fn joycaption_model(&self) -> Option<crate::ai::joycaption_adapter::JoyCaptionChatModel> {
        crate::ai::joycaption_adapter::joycaption_chat_model()
    }
}
