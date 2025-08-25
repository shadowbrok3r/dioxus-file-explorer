use base64::engine::general_purpose::STANDARD as BASE64;
use kalosm::language::*;
use base64::Engine;

// Typed schema for structured vision model responses.
// The model will be instructed to return JSON matching this schema so we avoid
// brittle free-form parsing and reduce 400 errors due to malformed streaming.
#[derive(Schema, Parse, Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct VisionDescription {
    /// A detailed natural language description (1-3 sentences, <= ~80 words)
    /// Allow common punctuation, limit length (approx <= 600 chars as safety cap)
    #[parse(pattern = r#"[A-Za-z0-9 ,.'\"()\-_/:%&#!?\[\]\n]{1,600}"#)]
    pub description: String,
    /// A concise caption (<= 40 words) suitable for thumbnail / alt text
    #[parse(pattern = r#"[A-Za-z0-9 ,.'\"()\-_/:%&#!?\[\]]{1,240}"#)]
    pub caption: String,
    /// 3-12 concise lowercase search tags (1-3 words each, no punctuation)
    /// (Element-level constraints not enforced here; will post-filter in code.)
    pub tags: Vec<String>,
    /// Single high-level category label (lowercase snake_case or hyphenated; may be empty if uncertain)
    #[parse(pattern = r"[a-z0-9_\-]{0,40}")]
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
        // let system_prompt = format!(
        //     r#"
        //     You analyze images and return strict JSON ONLY, matching this schema exactly: {}.
        //     Rules:\n\
        //     - description: 1-3 complete sentences, neutral, factual, <= 80 words. No hallucination beyond visible content.\n\
        //     - caption: short concise alt-text style (<= 40 words).\n\
        //     - tags: array of 3-12 concise lowercase search tags capturing salient concepts / objects / context (1-3 words each). No punctuation, numbering, quotes, or duplicates. If nothing meaningful, return an empty array.\n\
        //     - category: single high-level bucket (lowercase; prefer existing common photo/media genres). If unsure, use an empty string.\n\
        //     - NEVER add extra keys or commentary. Output MUST be valid JSON matching schema — no markdown.\n\
        //     - If image is blank / corrupted, use description & caption to say so and provide an empty tags array.\n\
        //     "#,
        //     VisionDescription::schema()
        // );

        let user_prompt = "Analyze this image";
        let mut chat = model
            .chat();
            // .with_system_prompt(system_prompt.clone());

        // Re-create media chunk each attempt (consumed by the call).
        let media_chunk = MediaChunk::new(
            MediaSource::url(url_str.clone()), 
            MediaType::Image
        );
        // Ask for typed response (structured parse) directly.
        match chat(&(media_chunk, user_prompt))
            .with_sampler(
                GenerationParameters::default()
                .with_temperature(1.0)
            )
            .typed::<VisionDescription>()
            .await
        {
            Ok(vd) => { return Some(vd); }
            Err(e) => {
                let msg = format!("vision description parse error: {e}");
                log::warn!("[AI] {}", msg);
                // Fallback: we try a best-effort JSON extraction from the raw model output.
                // Re-run a non-typed generation to capture raw text (avoids consuming original stream again).
                let mut chat_raw = model.chat(); // .with_system_prompt(system_prompt.clone());
                let media_chunk2 = MediaChunk::new(MediaSource::url(url_str.clone()), MediaType::Image);
                let mut stream = chat_raw(&(media_chunk2, user_prompt))
                .with_sampler(
                    GenerationParameters::default()
                    .with_temperature(1.0)
                )
                .typed::<VisionDescription>();
            
                let mut raw = String::new();
                while let Some(tok) = stream.next().await { raw.push_str(&tok.to_string()); }
                let _ = stream.await;
                if let Some(vd) = fallback_parse_vision_json(&raw) {
                    log::info!("[AI] Fallback JSON vision parse succeeded");
                    return Some(vd);
                } else {
                    log::warn!("[AI] Fallback vision JSON parse failed; raw len={} snippet={}", raw.len(), &raw.chars().take(200).collect::<String>());
                    return None;
                }
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

// Attempt to pull a JSON object from a possibly noisy model output and deserialize VisionDescription.
fn fallback_parse_vision_json(raw: &str) -> Option<VisionDescription> {
    // Heuristic: find first '{' and last '}' and attempt to parse substring; also try to correct trailing commas.
    let start = raw.find('{')?;
    let end = raw.rfind('}')?; // inclusive
    if end <= start { return None; }
    let mut candidate = raw[start..=end].to_string();
    // Remove common markdown fences/backticks or leading "json" hints
    if candidate.starts_with("```") {
        if let Some(idx) = candidate.find('{') { candidate = candidate[idx..].to_string(); }
    }
    // Simple fix: eliminate trailing commas before } or ]
    candidate = candidate
        .lines()
        .map(|l| {
            let trimmed = l.trim_end();
            if trimmed.ends_with(',') && (trimmed.ends_with("},") || trimmed.ends_with("],")) {
                // keep - legitimate commas
                l.to_string()
            } else if trimmed.ends_with(',') && (trimmed.ends_with('}') || trimmed.ends_with(']')) {
                // improbable pattern, but keep as-is
                l.to_string()
            } else { l.to_string() }
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Deserialize once; on error attempt a lenient tag split.
    match serde_json::from_str::<VisionDescription>(&candidate) {
        Ok(mut vd) => {
            // Basic sanitation
            vd.tags = vd.tags.into_iter().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).take(16).collect();
            Some(vd)
        }
        Err(e) => {
            log::debug!("[AI] fallback primary parse failed: {}", e);
            // Try to coerce minimal fields using regex-like splits.
            let desc = extract_field(&candidate, "description").unwrap_or_default();
            let caption = extract_field(&candidate, "caption").unwrap_or_else(|| desc.chars().take(60).collect());
            let tags_raw = extract_field(&candidate, "tags").unwrap_or_default();
            let tags: Vec<String> = tags_raw
                .split(|c: char| c == ',' || c == ';' || c == '\n')
                .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'' || c == '[' || c == ']'))
                .filter(|s| !s.is_empty())
                .take(16)
                .map(|s| s.to_lowercase())
                .collect();
            let category = extract_field(&candidate, "category").unwrap_or_default();
            if desc.is_empty() && caption.is_empty() && tags.is_empty() { return None; }
            Some(VisionDescription { description: desc, caption, tags, category, ..Default::default() })
        }
    }
}

fn extract_field(src: &str, key: &str) -> Option<String> {
    // naive search: "key" : value
    let needle = format!("\"{}\"", key);
    let idx = src.find(&needle)?;
    let rest = &src[idx + needle.len()..];
    // skip to first ':'
    let colon = rest.find(':')?;
    let after = &rest[colon + 1..];
    // Trim and collect until comma on same nesting level or line break
    let mut val = String::new();
    let mut depth = 0i32;
    for ch in after.chars() {
        match ch {
            '{' | '[' => { depth += 1; val.push(ch); }
            '}' | ']' => { if depth <= 0 { break; } depth -= 1; val.push(ch); }
            ',' if depth == 0 => break,
            '\n' | '\r' if depth == 0 => break,
            _ => val.push(ch),
        }
    }
    let cleaned = val.trim().trim_matches(|c| c == '"' || c == '\'' ).trim().trim_matches(',').to_string();
    Some(cleaned)
}
