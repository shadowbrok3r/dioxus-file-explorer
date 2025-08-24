pub mod ai; // keep available for completeness, though tests only need app
pub mod app;
pub mod explorer;
pub mod scan;
pub mod settings;
pub mod thumbs;
pub mod types;
pub mod components;

// Re-export helper for tests
pub use app::normalize_detail_widths;
