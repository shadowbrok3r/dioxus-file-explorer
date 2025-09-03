use super::AISearchEngine;
use kalosm::language::*;
use std::sync::Arc;
use tokio::sync::Mutex;

impl AISearchEngine {
    // Ensure a generic chat (OpenAI compatible) model is loaded. Falls back to vision model if separate chat not configured.
    pub async fn ensure_gpt_model(&self) -> Result<(), anyhow::Error> {
        {
            let guard = self.gpt_model.lock().await;
            if guard.is_some() {
                return Ok(());
            }
        }
        // Placeholder: reuse vision model path (multimodal) if dedicated chat model not yet chosen.
        self.ensure_vision_model().await?;
        Ok(())
    }

    // Unified chat entry: tries gpt_model first, then vision_model chat interface.
    pub async fn chat_any(&self, prompt: &str) -> Option<String> {
        // Try GPT model
        if self.ensure_gpt_model().await.is_ok() {
            if let Some(resp) = self.try_chat_gpt(prompt).await {
                return Some(resp);
            }
        }
        // Fallback to vision model
        if self.ensure_vision_model().await.is_ok() {
            if let Some(resp) = self.try_chat_vision(prompt).await {
                return Some(resp);
            }
        }
        None
    }

    async fn try_chat_gpt(&self, prompt: &str) -> Option<String> {
        let mut guard = self.gpt_model.lock().await;
        let model_opt = guard.as_mut();
        let Some(_model) = model_opt else {
            return None;
        };
        // Placeholder until dedicated text model integrated: return None so vision model handles.
        None
    }

    async fn try_chat_vision(&self, prompt: &str) -> Option<String> {
        let model_guard = self.vision_model.lock().await;
        let Some(model) = model_guard.as_ref() else {
            return None;
        };
        let mut chat = model.chat();
        let mut stream = chat(&prompt.to_string());
        let mut out = String::new();
        while let Some(tok) = stream.next().await {
            out.push_str(&tok.to_string());
        }
        if let Err(e) = stream.await {
            log::warn!("[AI] vision chat finalize err: {}", e);
        }
        Some(out.trim().to_string())
    }
}
