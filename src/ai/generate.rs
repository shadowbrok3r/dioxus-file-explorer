use base64::engine::general_purpose::STANDARD as BASE64;
use kalosm::language::*;
use base64::Engine;

#[derive(Schema, Clone, Debug, serde::Serialize, serde::Deserialize, Default, Parse)]
pub struct VisionDescription {
    pub description: String,
    pub caption: String,
    pub tags: Vec<String>,
    pub category: String,
}

impl super::AISearchEngine {
    // AI vision model description generation
    pub async fn generate_vision_description(
        &self,
        image_path: &std::path::PathBuf,
    ) -> Option<VisionDescription> {
        if !image_path.exists() {
            log::warn!("Image file does not exist: {:?}", image_path);
            return None;
        }
        // Prefer JoyCaption adapter when compiled + configured
        #[cfg(feature = "joycaption")]
        if let Some(local_model) = self.joycaption_model() {
            // Build a temporary chat session manually: a single user media+instruction message.
            let bytes = match tokio::fs::read(image_path).await { Ok(b)=>b, Err(e)=>{ log::warn!("JoyCaption read bytes failed: {e}"); Vec::new() } };
            if !bytes.is_empty() {
                use kalosm::language::{MediaChunk, MediaSource, MediaType, MessageContent, ChatMessage, MessageType as MT, GenerationParameters};
                let media_source = MediaSource::bytes(bytes);
                let media_chunk = MediaChunk::new(media_source, MediaType::Image);
                let mut content = MessageContent::new();
                content.push(media_chunk);
                content.push("Analyze the supplied image and return JSON with keys: description, caption, tags (array), category.");
                let user_msg = ChatMessage::new(MT::UserMessage, content);
                let mut session = crate::ai::joycaption_adapter::JoyCaptionChatSession::new();
                use std::sync::{Arc, Mutex};
                let collected = Arc::new(Mutex::new(String::new()));
                let params = GenerationParameters::default().with_temperature(0.6);
                // Run streaming collection
                let collected_clone = collected.clone();
                let res = local_model.add_messages_with_callback(&mut session, &[user_msg], params, move |tok| {
                    if let Ok(mut guard) = collected_clone.lock() { guard.push_str(&tok); }
                    Ok(())
                }).await;
                match res {
                    Ok(()) => {
                        let final_text = collected.lock().ok().map(|g| g.clone()).unwrap_or_default();
                        log::info!("Final text: {final_text}");
                        if let Some(vd) = super::joycaption_adapter::extract_json_vision(&final_text)
                            .and_then(|v| serde_json::from_value::<VisionDescription>(v).ok()) {
                            return Some(vd);
                        } else {
                            // Fallback: try simple describe method
                            match crate::ai::joycaption_adapter::describe_image(image_path).await {
                                Ok(vd) => return Some(vd),
                                Err(e) => log::warn!("JoyCaption fallback describe failed: {e}"),
                            }
                        }
                    }
                    Err(e) => {
                        let final_text = collected.lock().ok().map(|g| g.clone()).unwrap_or_default();
                        log::error!("JoyCaption streaming chat failed: {e}\ntext: {final_text}");
                        match crate::ai::joycaption_adapter::describe_image(image_path).await {
                            Ok(vd) => return Some(vd),
                            Err(e2) => log::warn!("JoyCaption fallback describe failed: {e2}"),
                        }
                    }
                }
            }
        }
        if let Err(e) = self.ensure_vision_model().await {
            log::error!("Failed to ensure vision model: {}", e);
            return None;
        }
        let model_opt = { self.vision_model.lock().await.clone() };

        let Some(model) = model_opt else {
            log::error!("Vision model not loaded after ensure");
            return None;
        };

        log::info!("image_path: {image_path:?}");
        let bytes = match std::fs::read(image_path) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Failed reading image bytes for {:?}: {}", image_path, e);
                return None;
            }
        };

        let b64 = BASE64.encode(&bytes);
        let url_str = format!("data:image/png;base64,{b64}");
        log::info!("URL: {url_str}");

        let user_prompt = "Analyze this image, and please include a list of one-word tags, along with a category of the image.";
        let mut chat = model.chat();

        // Re-create media chunk each attempt (consumed by the call).
        let media_chunk = MediaChunk::new(
            MediaSource::url(url_str.clone()), 
            MediaType::Image
        );

        let gen_params = GenerationParameters::default().with_temperature(1.0);
        
        // Ask for typed response (structured parse) directly. Some local GGUF vision builds currently
        // expose a Llama-style text-only template that cannot concatenate a list (image+text) with '+' in Jinja,
        // producing a ChatTemplateError like: "tried to use + operator on unsupported types string and sequence".
        // We detect that template failure and fall back to a text-only prompt that inlines an <image> placeholder.
        let attempt = chat(&(media_chunk.clone(), user_prompt))
            .typed::<VisionDescription>()
            .with_sampler(gen_params.clone())
            .await;
        match attempt {
            Ok(vd) => Some(vd),
            Err(e) => {
                let err_str = format!("{e:?}");
                if err_str.contains("tried to use + operator on unsupported types string and sequence") {
                    log::warn!("[AI] Vision model chat template rejected multi-part (image+text) message; falling back to flattened prompt.");
                    // Fallback strategy 1: Provide a single text message that includes an <image> token style hint.
                    let fallback_prompt = format!(
                        "You are an image analyst. The user has supplied an image as a base64 data URL below.\n\nIMAGE_DATA_URL:\n{}User request: {user_prompt}\n",
                        url_str
                    );
                    let mut chat2 = model.chat();
                    match chat2(&fallback_prompt)
                        .typed::<VisionDescription>()
                        .with_sampler(gen_params)
                        .await
                    {
                        Ok(vd2) => Some(vd2),
                        Err(e2) => {
                            log::error!("[AI] Fallback vision description generation failed: {e2:?}");
                            None
                        }
                    }
                } else {
                    log::error!("Error generating description: {e:?}");
                    None
                }
            }
        }
    }

    // Generate (or regenerate if force) description for a single path without re-indexing document table.
    pub async fn generate_description_for_path(
        &self,
        path: &str,
        force: bool,
    ) -> anyhow::Result<Option<String>, anyhow::Error> {
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

        if let Some(vd) = self.generate_vision_description(&pb).await {
            // Update & persist
            if let Some(mut meta_inner) = self.get_file_metadata(path).await {
                meta_inner.description = Some(vd.description.clone());
                meta_inner.caption = Some(vd.caption.clone());
                // Use tags directly from structured vision response
                meta_inner.tags = vd.tags.clone();
                meta_inner.category = if vd.category.trim().is_empty() { None } else { Some(vd.category.clone()) };
                // Replace existing metadata in-memory
                {
                    let mut files = self.files.lock().await;
                    if let Some(idx) = files.iter().position(|f| f.path == path) {
                        files[idx] = meta_inner.clone();
                    }
                }
                if let Err(e) = self.cache_thumbnail_and_metadata(&meta_inner).await {
                    log::warn!("Failed to persist updated description for {}: {}", path, e);
                }
                return Ok(meta_inner.description.clone());
            }
            Ok(None)
        } else {
            Ok(None)
        }
    }

    // Generate an image from a prompt (placeholder implementation creates blank image w/ metadata header file)
    #[allow(dead_code)]
    pub async fn generate_image(
        &self,
        prompt: &str,
    ) -> anyhow::Result<std::path::PathBuf, anyhow::Error> {
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
            caption: Some(format!("generated image: {}", prompt)),
            tags: vec!["generated".into()],
            category: Some("generated".into()),
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