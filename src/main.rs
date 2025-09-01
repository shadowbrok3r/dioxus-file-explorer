mod components;
mod utilities;
mod database;
mod app;
mod ai;

pub use utilities::{explorer::*, scan::*, thumbs::*, types::*, files::*};
pub use database::*;

fn main() {
    let level = std::env::var("RUST_LOG").ok()
    .and_then(|v| 
        v.parse::<simplelog::LevelFilter>().ok()
    ).unwrap_or(simplelog::LevelFilter::Info);
    let log_file = std::fs::File::create("output.log")
    .unwrap_or_else(|_| 
        std::fs::File::create("output.log")
        .expect("create output.log")
    );

    let _ = simplelog::WriteLogger::init(level, simplelog::Config::default(), log_file);

    dioxus::LaunchBuilder::desktop()
    .with_cfg(
        dioxus::desktop::Config::new()
        .with_window(
            dioxus::desktop::WindowBuilder::new()
            .with_resizable(true)
            .with_title("Ai Filesystem Tool")
        )
        // .with_disable_context_menu(true)
        // .with_menu(None)
    )
    .launch(app::app);
}
