
use fastembed::{ImageEmbedding, ImageInitOptions, ImageEmbeddingModel, TextEmbedding, TextInitOptions, EmbeddingModel};
use ort::execution_providers::{cuda::CUDAAttentionBackend, CUDAExecutionProvider, ExecutionProvider};
use rand::{Rng, SeedableRng, rngs::StdRng};


fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 { for x in &mut v { *x /= norm; } }
    v
}


pub struct ClipEngine {
    image_model: ImageEmbedding,
    text_model: TextEmbedding,
    // Precomputed zero-shot label embeddings (label -> embedding)
    labels: Vec<String>,
    label_vectors: Vec<Vec<f32>>, // normalized
}


impl ClipEngine {
    pub fn new_default() -> anyhow::Result<Self> {
        // Use matching CLIP model variant for image & text
        let img_model = ImageEmbedding::try_new(
            ImageInitOptions::new(
                ImageEmbeddingModel::UnicomVitB32
            )
            .with_execution_providers(
                vec![
                    CUDAExecutionProvider::default()
                    .with_attention_backend(CUDAAttentionBackend::CUDNN_FLASH_ATTENTION)
                    .build()
                ]
            )
        )?;
        let txt_model = TextEmbedding::try_new(
            TextInitOptions::new(
                EmbeddingModel::ClipVitB32
            )
            .with_execution_providers(
                vec![
                    CUDAExecutionProvider::default()
                    .with_attention_backend(CUDAAttentionBackend::CUDNN_FLASH_ATTENTION)
                    .build()
                ]
            )
        )?;

        // Placeholder label set (expand later or load from config)
        let labels = vec![
            "person".to_string(),
            "animal".to_string(),
            "landscape".to_string(),
            "food".to_string(),
            "document".to_string(),
            "screenshot".to_string(),
            "vehicle".to_string(),
            "art".to_string(),
        ];
        let mut engine = Self { image_model: img_model, text_model: txt_model, labels, label_vectors: Vec::new() };
        engine.build_label_vectors()?;
        Ok(engine)
    }

    fn build_label_vectors(&mut self) -> anyhow::Result<()> {
        let txts = self.labels.clone();
        let raw = self.text_model.embed(txts.clone(), None)?; // Vec<Vec<f32>>
        self.label_vectors = raw.into_iter().map(l2_normalize).collect();
        Ok(())
    }

    pub fn embed_image_path(&mut self, path: &str) -> anyhow::Result<Vec<f32>> {
        let out = self.image_model.embed(vec![path.to_string()], None)?; // single
        Ok(l2_normalize(out.into_iter().next().unwrap()))
    }

    pub fn embed_text(&mut self, text: &str) -> anyhow::Result<Vec<f32>> {
        let out = self.text_model.embed(vec![text.to_string()], None)?;
        Ok(l2_normalize(out.into_iter().next().unwrap()))
    }

    pub fn zero_shot_tags(&mut self, image_vec: &[f32], top_k: usize) -> Vec<String> {
        let mut scores: Vec<(f32, &str)> = self.label_vectors
            .iter()
            .zip(self.labels.iter())
            .map(|(v,l)| (dot(image_vec, v), l.as_str()))
            .collect();
        scores.sort_by(|a,b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scores.into_iter().take(top_k).map(|(_,l)| l.to_string()).collect()
    }

    pub fn zero_shot_category(&mut self, image_vec: &[f32]) -> Option<String> {
        self.label_vectors
            .iter()
            .zip(self.labels.iter())
            .map(|(v,l)| (dot(image_vec, v), l))
            .max_by(|a,b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_,l)| l.clone())
    }
}


fn dot(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(x,y)| x*y).sum() }


pub(crate) async fn ensure_clip_engine(engine_slot: &std::sync::Arc<tokio::sync::Mutex<Option<ClipEngine>>>) -> anyhow::Result<()> {
    let mut guard = engine_slot.lock().await;
    if guard.is_none() {
        log::info!("[CLIP] Loading fastembed CLIP models (ViT-B/32)...");
        match ClipEngine::new_default() {
            Ok(c) => { *guard = Some(c); log::info!("[CLIP] Loaded."); },
            Err(e) => { log::error!("[CLIP] Failed to init: {e}"); return Err(e); }
        }
    }
    Ok(())
}


pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 { dot(a,b) }
