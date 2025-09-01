use surrealdb::{engine::local::Db, opt::{capabilities::Capabilities, Config}, Surreal};
use crate::settings::UiSettings;
use std::sync::LazyLock;
pub mod settings;

pub static DB: LazyLock<Surreal<Db>> = LazyLock::new(Surreal::init);
pub const THUMBNAILS: &str = "thumbnails";
pub const USER_SETTINGS: &str = "user_settings";
pub const DB_DEFAULT_TABLE: &str = "./db/default.surql";
pub const DB_BACKUP_PATH: &str = "./db/backup.surql";

pub async fn new() -> anyhow::Result<(), anyhow::Error> {
    let capabilities = Capabilities::all().with_all_experimental_features_allowed();
    let config = Config::new().capabilities(capabilities);
    DB.connect::<surrealdb::engine::local::Mem>( config).await?;
    DB.use_ns("file_explorer").use_db("ai_search").await?;
    DB.wait_for(surrealdb::opt::WaitFor::Database).await;
    DB.import(DB_DEFAULT_TABLE).await?;
    let query = r#"
        BEGIN;
        DEFINE TABLE IF NOT EXISTS thumbnails TYPE NORMAL SCHEMAFULL PERMISSIONS FULL;
        DEFINE TABLE IF NOT EXISTS user_settings TYPE NORMAL SCHEMAFULL PERMISSIONS FULL;

        DEFINE FIELD IF NOT EXISTS caption ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS category ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS db_created ON thumbnails TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS description ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS embedding ON thumbnails TYPE array<float> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS file_type ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS filename ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS hash ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS modified ON thumbnails TYPE datetime PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS path ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS size ON thumbnails TYPE number PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS tags ON thumbnails TYPE array<string> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS thumbnail_b64 ON thumbnails TYPE bytes PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS tags[*] ON thumbnails TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS embedding[*] ON thumbnails TYPE float PERMISSIONS FULL;

        DEFINE FIELD IF NOT EXISTS qa_collapsed ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS drives_collapsed ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS preview_collapsed ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS preview_width ON user_settings TYPE number PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS sort ON user_settings TYPE object PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS sort.by ON user_settings TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS sort.asc ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS view_mode ON user_settings TYPE option<string> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS left_width ON user_settings TYPE number PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS ext_enabled ON user_settings TYPE option<array<any>> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS excluded_dirs ON user_settings TYPE option<array<string>> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS group_by_category ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS detail_column_widths ON user_settings TYPE option<array<number>> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS category_col_width ON user_settings TYPE option<number> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS auto_indexing ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS ai_prompt_template ON user_settings TYPE string PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS overwrite_descriptions ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS filter_modified_after ON user_settings TYPE option<string> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS filter_modified_before ON user_settings TYPE option<string> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS filter_category_multi ON user_settings TYPE option<array<string>> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS filter_only_with_thumb ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS filter_only_with_description ON user_settings TYPE bool PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS last_root ON user_settings TYPE option<string> PERMISSIONS FULL;
        DEFINE FIELD IF NOT EXISTS show_progress_overlay ON user_settings TYPE bool PERMISSIONS FULL;

        DEFINE INDEX IF NOT EXISTS category_idx ON thumbnails FIELDS category;
        DEFINE INDEX IF NOT EXISTS tags_idx ON thumbnails FIELDS tags;
        COMMIT;
    "#;
    
    let response = DB.query(query).await?;
    let _ = response.check()?;
    // Spawn periodic export (acts as safeguard since true process exit hook isn't wired yet)
    tokio::spawn(async move {
        use tokio::time::{sleep, Duration};
        loop {
            sleep(Duration::from_secs(30)).await;
            if let Err(e) = DB.export(DB_BACKUP_PATH).await { log::warn!("Periodic DB export failed: {e}"); }
        }
    });
    Ok(())
}

pub async fn save_settings(s: UiSettings) -> anyhow::Result<(), anyhow::Error> {
    DB.upsert::<Option<UiSettings>>(UiSettings::default().id).content::<UiSettings>(s).await?;
    DB.export(DB_BACKUP_PATH).await?;
    Ok(())
}

pub async fn get_settings() -> anyhow::Result<UiSettings, anyhow::Error> {
    let settings_res: Option<UiSettings> = DB.select(UiSettings::default().id).await?;
    log::info!("Got settings: {settings_res:?}");
    if let Some(settings)  = settings_res {
        return Ok(settings);
    } else {
        Ok(UiSettings::default())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    pub id: Option<String>,
    pub path: String,
    pub filename: String,
    pub file_type: String,
    pub size: u64,
    pub modified: Option<chrono::DateTime<chrono::Local>>,
    pub created: Option<chrono::DateTime<chrono::Local>>,
    pub thumbnail_path: Option<String>,
    // In-memory/base64 thumbnail data (data URL) when available. This lets AI search results
    // render thumbnails even if the underlying FoundFile list isn't currently visible.
    pub thumb_b64: Option<String>,
    // BLAKE3 hex hash of file contents to detect if content changed and re-embedding is needed.
    pub hash: Option<String>,
    // AI-powered metadata
    pub description: Option<String>,   // AI-generated description (multi-sentence)
    pub caption: Option<String>,       // Short caption/alt text
    pub tags: Vec<String>,             // AI-extracted tags
    pub category: Option<String>,      // Single high-level AI category
    pub embedding: Option<Vec<f32>>,   // AI embedding vector
    pub similarity_score: Option<f32>, // For search ranking
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct Thumbnail {
    pub db_created: surrealdb::Datetime,
    pub path: String,
    pub filename: String,
    pub file_type: String,
    pub size: u64,
    pub description: Option<String>,
    pub caption: Option<String>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub thumbnail_b64: Option<String>,
    pub modified: Option<String>,
    pub hash: Option<String>,
}

// Lightweight projection for semantic document debug (id + first chars)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DebugDocumentSnippet {
    pub id: String,
    pub title: Option<String>,
    pub preview: String,
    pub len: usize,
}


