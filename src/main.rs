mod ai;
mod app;
mod explorer;
mod scan;
mod settings;
mod thumbs;
mod types;
mod components;

use dioxus::desktop::{Config, WindowBuilder};
use simplelog::{Config as LogConfig, LevelFilter, WriteLogger};
use std::fs::File;

fn main() {
    let level = std::env::var("RUST_LOG").ok().and_then(|v| v.parse::<LevelFilter>().ok()).unwrap_or(LevelFilter::Info);
    let log_file = File::create("output.log").unwrap_or_else(|_| File::create("output.log").expect("create output.log"));
    let _ = WriteLogger::init(level, LogConfig::default(), log_file);

    dioxus::LaunchBuilder::desktop()
    .with_cfg(
        Config::new()
        .with_window(
            WindowBuilder::new()
            .with_resizable(true)
            .with_title("Ai Filesystem Tool")
        )
        // .with_disable_context_menu(true)
        // .with_menu(None)
    )
    .launch(app::app);
}
