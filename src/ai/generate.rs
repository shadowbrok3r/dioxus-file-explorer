use kalosm::language::*;

impl super::AISearchEngine {
    // AI vision model description generation
    pub async fn generate_vision_description(&self, image_path: &std::path::PathBuf) -> Option<String> {
        if !image_path.exists() {
            log::warn!("Image file does not exist: {:?}", image_path);
            return None;
        }
        match self.ensure_vision_model().await {
            Ok(()) => {
                if let Some(model) = self.vision_model.lock().await.as_ref() {
                    log::info!("[AI] Vision model describing image: {:?}", image_path);
                    let prompt = "Describe this image in detail. Provide a concise natural language caption (<= 40 words).";
                    let bytes = match std::fs::read(image_path) {
                        Ok(b) => b,
                        Err(e) => {
                            log::warn!("Failed reading image bytes for {:?}: {}", image_path, e);
                            return None;
                        }
                    };
                    let media_chunk = MediaChunk::new(
                        MediaSource::bytes(bytes.clone()), 
                        MediaType::Image
                    );
                    // Attempt 1: slice-of-one pair
                    let mut chat = model.chat().with_session(session).with_system_prompt(prompt);
                    let mut stream = chat(&(media_chunk, prompt));
                    let mut description = String::new();
                    while let Some(token) = stream.next().await {
                        log::info!("Token stream: {token}");
                        description.push_str(&token.to_string());
                    }
                    if let Err(e) = stream.await {
                        log::warn!("Vision model finalization error (primary attempt): {}", e);
                    }
                    Some(description)
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

    // Generate (or regenerate if force) description for a single path without re-indexing document table.
    pub async fn generate_description_for_path(
        &self,
        path: &str,
        force: bool,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let pb = std::path::PathBuf::from(path);
        if !pb.exists() {
            return Ok(None);
        }
        {
            let files = self.files.lock().await;
            if !force {
                if let Some(f) = files.iter().find(|f| f.path == path) {
                    if f.description.is_some()
                        && f.description
                            .as_ref()
                            .map(|d| d.trim().len() >= 12)
                            .unwrap_or(false)
                    {
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
                    if let Some(idx) = files.iter().position(|f| f.path == path) {
                        files[idx] = meta.clone();
                    }
                }
                if let Err(e) = self.cache_thumbnail_and_metadata(&meta).await {
                    log::warn!("Failed to persist updated description for {}: {}", path, e);
                }
            }
            Ok(Some(desc))
        } else {
            Ok(None)
        }
    }

    // Generate an image from a prompt (placeholder implementation creates blank image w/ metadata header file)
    #[allow(dead_code)]
    pub async fn generate_image(
        &self,
        prompt: &str,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        use image::{ImageBuffer, Rgba};
        // Ensure directory
        let out_dir = std::path::PathBuf::from("./generated");
        if !out_dir.exists() {
            std::fs::create_dir_all(&out_dir)?;
        }
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
        let filename = if slug.is_empty() {
            format!("gen_{}.png", ts)
        } else {
            format!("{}_{}.png", slug, ts)
        };
        let out_path = out_dir.join(filename);
        // Simple gradient image as placeholder
        let imgx = 512;
        let imgy = 512;
        let mut imgbuf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(imgx, imgy);
        for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
            let r = (x as f32 / imgx as f32 * 255.0) as u8;
            let g = (y as f32 / imgy as f32 * 255.0) as u8;
            let b = 128u8;
            *pixel = Rgba([r, g, b, 255]);
        }
        imgbuf.save(&out_path)?;
        log::info!(
            "Generated placeholder image: {:?} for prompt '{}'",
            out_path,
            prompt
        );

        // Index generated image metadata
        let meta = super::FileMetadata {
            id: None,
            path: out_path.to_string_lossy().to_string(),
            filename: out_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            file_type: "image".to_string(),
            size: std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0),
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
}