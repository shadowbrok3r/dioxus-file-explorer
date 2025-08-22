use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub qa_collapsed: bool,
    pub drives_collapsed: bool,
    pub preview_collapsed: bool,
    pub preview_width: u32,
    pub sort: Option<SortSetting>,
    pub view_mode: Option<String>, // "icons" | "details"
    pub left_width: u32,
    pub ext_enabled: Option<Vec<(String,bool)>>,
    pub excluded_dirs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortSetting { pub by: SortBy, pub asc: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortBy { Name, Modified, Created, Size, Type }

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            qa_collapsed: false,
            drives_collapsed: false,
            preview_collapsed: false,
            preview_width: 320,
            sort: Some(SortSetting { by: SortBy::Name, asc: true }),
            view_mode: Some("icons".into()),
            left_width: 240,
            ext_enabled: None,
            excluded_dirs: None,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    let proj = ProjectDirs::from("com", "Example", "DioxusMediaExplorer")?;
    let dir = proj.config_dir();
    let _ = fs::create_dir_all(dir);
    Some(dir.join("ui_settings.json"))
}

pub fn load_settings() -> UiSettings {
    if let Some(path) = settings_path() {
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(s) = serde_json::from_str::<UiSettings>(&data) { return s; }
        }
    }
    UiSettings::default()
}

pub fn save_settings(s: &UiSettings) {
    if let Some(path) = settings_path() {
        if let Ok(data) = serde_json::to_string_pretty(s) { let _ = fs::write(path, data); }
    }
}
