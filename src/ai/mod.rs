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
pub mod bulk;

pub use ai_search::*;

use crate::database::FileMetadata;

// AI Search Engine with full Kalosm integration
#[derive(Clone)]
pub struct AISearchEngine {
    pub vision_model: std::sync::Arc<tokio::sync::Mutex<Option<kalosm::language::Llama>>>, // Llama
    // pub gpt_model: std::sync::Arc<tokio::sync::Mutex<Option<OpenAICompatibleChatModel>>>,
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
}
