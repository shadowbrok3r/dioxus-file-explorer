//! Dioxus Media Explorer entry point. See `app.rs` for UI and feature modules in `src/`.
mod app;
mod types;
mod explorer;
mod thumbs;
mod scan;
mod settings;
mod ai_search;
use dioxus::desktop::{Config, WindowBuilder};

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(WindowBuilder::new().with_resizable(true)))
    .launch(app::app);
}
