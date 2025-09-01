use surrealdb::{engine::{remote::ws::Client, local::Db}, opt::{capabilities::Capabilities, Config}, Surreal};
use std::{path::PathBuf, sync::LazyLock};
pub mod settings;
pub mod files;
pub use settings::*;
pub use files::*;

pub static DB: LazyLock<Surreal<Client>> = LazyLock::new(Surreal::init);
pub static LOCAL_DB: LazyLock<Surreal<Db>> = LazyLock::new(Surreal::init);
pub const THUMBNAILS: &str = "thumbnails";
pub const USER_SETTINGS: &str = "user_settings";
pub const DB_DEFAULT_TABLE: &str = "./db/default.surql";
pub const DB_BACKUP_PATH: &str = "./db/backup.surql";

pub async fn new() -> anyhow::Result<(), anyhow::Error> {
    let capabilities = Capabilities::all().with_all_experimental_features_allowed();
    let config = Config::new().capabilities(capabilities);
    LOCAL_DB.connect::<surrealdb::engine::local::SurrealKv>(("./db/ai_search1.db", config)).await?;
    DB.connect::<surrealdb::engine::remote::ws::Ws>("localhost:50000").await?;
    DB.use_ns("file_explorer").use_db("ai_search").await?;
    DB.wait_for(surrealdb::opt::WaitFor::Database).await;
    // DB.import(DB_DEFAULT_TABLE).await?;
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
    // tokio::spawn(async move {
    //     use tokio::time::{sleep, Duration};
    //     loop {
    //         sleep(Duration::from_secs(30)).await;
    //         if let Err(e) = DB.export(DB_BACKUP_PATH).await { log::warn!("Periodic DB export failed: {e}"); }
    //     }
    // });
    Ok(())
}

pub async fn save_thumbnail_batch(thumbs: Vec<Thumbnail>) -> anyhow::Result<(), anyhow::Error> {
    let _: Option<crate::Thumbnail> = DB
        .create("thumbnails")
        .content::<Vec<crate::Thumbnail>>(thumbs)
        .await?
        .take();
    Ok(())
}

// Load all thumbnail rows (full records)
pub async fn load_all_thumbnails() -> anyhow::Result<Vec<Thumbnail>, anyhow::Error> {
    let rows: Vec<crate::Thumbnail> = DB.select("thumbnails").await?;
    Ok(rows)
}

// Build a lookup map: path -> (hash, thumb_b64, category)
pub async fn load_thumb_lookup() -> anyhow::Result<std::collections::HashMap<String,(Option<String>,Option<String>,Option<String>)>, anyhow::Error> {
    let rows: Vec<crate::Thumbnail> = DB.select("thumbnails").await?;
    let mut map = std::collections::HashMap::with_capacity(rows.len());
    for r in rows.into_iter() {
        map.insert(r.path.clone(), (r.hash.clone(), r.thumbnail_b64.clone(), r.category.clone()));
    }
    Ok(map)
}

// Save (insert) a single thumbnail row (best-effort). Does not deduplicate existing rows.
pub async fn save_thumbnail_row(row: Thumbnail) -> anyhow::Result<(), anyhow::Error> {
    let _: Option<crate::Thumbnail> = DB
        .create("thumbnails")
        .content::<crate::Thumbnail>(row)
        .await?
        .take();
    Ok(())
}

pub async fn retrieve_all_thumbs(thumbs: Vec<Thumbnail>) -> anyhow::Result<Vec<Thumbnail>, anyhow::Error> {
    let thumbs: Vec<crate::Thumbnail> = DB
        .select("thumbnails")
        .await?;
    Ok(thumbs)
}

pub async fn get_thumbs_for_path(path: PathBuf) -> anyhow::Result<Vec<Thumbnail>, anyhow::Error> {
    let thumbs: Vec<crate::Thumbnail> = DB
        .select("thumbnails")
        .await?;
    Ok(thumbs)
}

pub async fn save_settings(s: UiSettings) -> anyhow::Result<(), anyhow::Error> {
    DB.upsert::<Option<UiSettings>>(UiSettings::default().id).content::<UiSettings>(s).await?;
    // DB.export(DB_BACKUP_PATH).await?;
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




