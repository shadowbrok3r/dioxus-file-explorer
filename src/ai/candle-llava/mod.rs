//! Internal Candle-based LLaVA components (ported) used by the JoyCaption adapter.
pub mod clip;
pub mod clip_image_processor;
pub mod config;
pub mod constants;
pub mod conversation;
pub mod llama;
pub mod model;
pub mod utils;

pub fn load_image<T: AsRef<std::path::Path>>(
    path: T,
    processor: &clip_image_processor::CLIPImageProcessor,
    llava_config: &config::LLaVAConfig,
    dtype: candle_core::DType,
) -> anyhow::Result<((u32, u32), candle_core::Tensor)> {
    let img = image::ImageReader::open(path)?.decode()?;
    let img_tensor = crate::ai::candle_llava::utils::process_image(&img, processor, llava_config)?;
    Ok(((img.width(), img.height()), img_tensor.to_dtype(dtype)?))
}

pub fn scalarize_id(opt: Option<&serde_json::Value>, default_: i64) -> serde_json::Value {
    use serde_json::Value::*;
    match opt {
        Some(Number(n)) => Number(n.clone()),
        Some(Array(a)) => a.get(0)
            .and_then(|x| x.as_i64())
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::json!(default_)),
        Some(v) => v.clone(),
        None => serde_json::json!(default_),
    }
}