//! Dioxus Media Explorer entry point. See `app.rs` for UI and feature modules in `src/`.
mod app;
mod types;
mod explorer;
mod thumbs;
mod scan;
mod settings;
mod ai_search;
use dioxus::desktop::{Config, WindowBuilder};
use simplelog::{WriteLogger, Config as LogConfig, LevelFilter};
use std::fs::File;

fn main() {
    let log_file = File::create("output.log").unwrap_or_else(|_| File::create("output.log").expect("create output.log"));
    let level = std::env::var("RUST_LOG").ok()
        .and_then(|v| v.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);
    let _ = WriteLogger::init(level, LogConfig::default(), log_file);
    log::info!("Logger initialized (level={:?})", level);
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(WindowBuilder::new().with_resizable(true)))
        .launch(app::app);
}
