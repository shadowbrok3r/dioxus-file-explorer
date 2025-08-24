## Dioxus File Explorer – AI Metadata Overview

This app indexes local files (images, videos, other media) and enriches images with AI‑generated structured metadata using a single typed model response.

### Structured Vision Schema

The vision model is instructed (via Kalosm) to emit STRICT JSON conforming to this Rust schema:

```rust
#[derive(Schema, Parse, Clone, serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct VisionDescription {
	pub description: String, // 1‑3 sentences, <= ~80 words
	pub caption: String,     // concise alt‑text (<= 40 words)
	pub tags: Vec<String>,   // 3‑12 lowercase search tags (1-3 words each)
}
```

All three fields are produced in one generation step (no post‑hoc string parsing). Tags are never parsed from free‑form comma text – they arrive as a JSON array.

### Indexing Pipeline (Simplified)
1. Fast file scan collects size, times, type, extension, optional thumbnail path.
2. For images, `generate_vision_description` loads the vision model (once, cached) and requests the `VisionDescription` struct.
3. The returned struct populates `FileMetadata.description`, `caption`, and `tags` directly.
4. A semantic document is built with header lines (FILE_PATH, HASH, FILE_TYPE, FILE_SIZE, CAPTION, TAGS, SEGMENTS, DESCRIPTION, OCR) followed by a unified searchable body containing filename, description, caption, text_content, segments, and tags.
5. The document table (SurrealDB + Kalosm) stores embeddings for semantic search.

### Removed Legacy Logic
`extract_ai_tags` has been eliminated. Tag generation is now intrinsic to the vision response to guarantee consistency and avoid brittle formatting heuristics.

### Debug View
The in‑app Database Debug View lists:
* Cached thumbnail/metadata rows (including caption & tag counts)
* Semantic document snippets (showing stored header + preview text)

### Future Ideas
* Extend structured schema with: dominant colors, objects (already separately stored), OCR confidence stats.
* Unify non‑image (document/video) enrichment into parallel typed schemas.

---
This README focuses on the AI metadata refactor; general usage instructions still TBD.
