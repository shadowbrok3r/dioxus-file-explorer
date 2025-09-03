
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

    pub async fn search_clip_text(&self, query: &str, top_k: usize) -> Vec<super::FileMetadata> {
        if self.ensure_clip_engine().await.is_err() { return Vec::new(); }
        let query_vec = {
            let mut guard = self.clip_engine.lock().await;
            if let Some(engine) = guard.as_mut() {
                match engine.embed_text(query) { Ok(v) => v, Err(e) => { log::error!("[CLIP] text embed failed: {e}"); return Vec::new(); } }
            } else { return Vec::new(); }
        };
        let mut scored: Vec<(f32, super::FileMetadata)> = {
            let files = self.files.lock().await;
            files.iter().filter_map(|f| f.clip_embedding.as_ref().map(|emb| (Self::dot(&query_vec, emb), f.clone()))).collect()
        };
        scored.sort_by(|a,b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(top_k).map(|(s, mut m)| { m.clip_similarity_score = Some(s); m }).collect()
    }

    
    pub async fn search_clip_image(&self, image_path: &str, top_k: usize) -> Vec<super::FileMetadata> {
        if self.ensure_clip_engine().await.is_err() { return Vec::new(); }
        let image_vec = {
            let mut guard = self.clip_engine.lock().await;
            if let Some(engine) = guard.as_mut() {
                match engine.embed_image_path(image_path) { Ok(v) => v, Err(e) => { log::error!("[CLIP] image embed failed: {e}"); return Vec::new(); } }
            } else { return Vec::new(); }
        };
        let mut scored: Vec<(f32, super::FileMetadata)> = {
            let files = self.files.lock().await;
            files.iter().filter_map(|f| f.clip_embedding.as_ref().map(|emb| (Self::dot(&image_vec, emb), f.clone()))).collect()
        };
        scored.sort_by(|a,b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(top_k).map(|(s, mut m)| { m.clip_similarity_score = Some(s); m }).collect()
    }

    pub async fn backfill_clip_embeddings(&self) -> usize {
        if self.ensure_clip_engine().await.is_err() { return 0; }
        let pending: Vec<String> = {
            let files = self.files.lock().await;
            files.iter().filter(|f| f.file_type == "image" && f.clip_embedding.is_none()).map(|f| f.path.clone()).collect()
        };
        if pending.is_empty() { return 0; }
        log::info!("[CLIP] Backfilling {} image embeddings", pending.len());
        let mut done = 0usize;
        for p in pending {
            if !std::path::Path::new(&p).exists() { continue; }
            let mut guard = self.clip_engine.lock().await;
            if let Some(engine) = guard.as_mut() {
                match engine.embed_image_path(&p) {
                    Ok(vec) => {
                        let mut files = self.files.lock().await;
                        if let Some(fm) = files.iter_mut().find(|f| f.path == p) {
                            fm.clip_embedding = Some(vec.clone());
                            if fm.tags.len() < 2 {
                                let mut tags = engine.zero_shot_tags(fm.clip_embedding.as_ref().unwrap(), 3);
                                for t in tags.drain(..) { if !fm.tags.iter().any(|et| et == &t) { fm.tags.push(t); } }
                            }
                            if fm.category.is_none() { fm.category = engine.zero_shot_category(fm.clip_embedding.as_ref().unwrap()); }
                            done += 1;
                        }
                    }
                    Err(e) => log::warn!("[CLIP] Backfill failed for {}: {e}", p),
                }
            }
        }
        done
    }

    // Generate CLIP embeddings for explicit list of image paths (skips missing/non-image)
    pub async fn generate_clip_for_paths(&self, paths: &[String]) -> usize {
        if self.ensure_clip_engine().await.is_err() { return 0; }
        let mut added = 0usize;
        for p in paths {
            let pb = std::path::Path::new(p);
            if !pb.exists() { continue; }
            // Locate existing metadata or skip if not indexed yet
            let mut files = self.files.lock().await;
            if let Some(fm) = files.iter_mut().find(|f| f.path == *p && f.file_type == "image") {
                if fm.clip_embedding.is_some() { continue; }
                drop(files); // release lock while embedding
                let emb_opt = {
                    let mut guard = self.clip_engine.lock().await;
                    if let Some(engine) = guard.as_mut() { engine.embed_image_path(p).ok() } else { None }
                };
                if let Some(vec) = emb_opt {
                    let mut files2 = self.files.lock().await;
                    if let Some(fm2) = files2.iter_mut().find(|f| f.path == *p) {
                        fm2.clip_embedding = Some(vec.clone());
                        // Add zero-shot tags/category if needed
                        if let Some(engine) = self.clip_engine.lock().await.as_mut() {
                            if fm2.tags.len() < 2 {
                                let mut tags = engine.zero_shot_tags(fm2.clip_embedding.as_ref().unwrap(), 3);
                                for t in tags.drain(..) { if !fm2.tags.iter().any(|et| et == &t) { fm2.tags.push(t); } }
                            }
                            if fm2.category.is_none() {
                                fm2.category = engine.zero_shot_category(fm2.clip_embedding.as_ref().unwrap());
                            }
                        }
                        // Synthesize a short description if still missing or too short
                        let needs_desc = fm2.description.as_ref().map(|d| d.trim().len() < 12).unwrap_or(true);
                        if needs_desc {
                            let cat_opt = fm2.category.clone().filter(|c| !c.trim().is_empty());
                            let tag_list: Vec<String> = fm2.tags.iter().take(5).cloned().collect();
                            let desc = match (cat_opt, tag_list.is_empty()) {
                                (Some(cat), false) => format!("{cat} – tags: {}.", tag_list.join(", ")),
                                (Some(cat), true) => format!("{cat} image."),
                                (None, false) => format!("Image with tags: {}.", tag_list.join(", ")),
                                (None, true) => "Image (CLIP embedding generated).".to_string(),
                            };
                            fm2.description = Some(desc);
                        }
                        let path_clone = p.clone();
                        let wrote_desc = fm2.description.is_some();
                        drop(files2); // release before awaiting
                        // Persist updated metadata (best-effort) if we generated a description or new embedding
                        if wrote_desc {
                            if let Some(updated) = self.get_file_metadata(&path_clone).await { let _ = self.cache_thumbnail_and_metadata(&updated).await; }
                        }
                        added += 1;
                    }
                }
            }
        }
        added
    }

    // Generate CLIP embeddings recursively (all indexed image files without clip embedding)
    pub async fn generate_clip_recursive(&self) -> usize {
        let targets: Vec<String> = {
            let files = self.files.lock().await;
            files.iter().filter(|f| f.file_type == "image" && f.clip_embedding.is_none()).map(|f| f.path.clone()).collect()
        };
        self.generate_clip_for_paths(&targets).await
    }

    // Convenience: generate CLIP embedding for a single path; returns true if added.
    pub async fn generate_clip_for_path(&self, path: &str) -> bool {
        let added = self.generate_clip_for_paths(&[path.to_string()]).await;
        added > 0
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
