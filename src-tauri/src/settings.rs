use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub theme_mode: ThemeMode,
    #[serde(default)]
    pub default_ndi_groups: String,
    #[serde(default)]
    pub hide_donation_prompt: bool,
    /// Servo シェル背景の透明クリア（NDI アルファ）。
    #[serde(default)]
    pub ndi_alpha_enabled: bool,
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.default_ndi_groups.len() > 256 {
            return Err("NDIグループは256文字以内にしてください".into());
        }
        Ok(())
    }
}

/// アプリ設定の NDI グループ（空なら None）。
pub fn ndi_groups_from_app_settings(settings: &AppSettings) -> Option<String> {
    let t = settings.default_ndi_groups.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub async fn load_app_settings(path: &std::path::PathBuf) -> AppSettings {
    if !path.exists() {
        return AppSettings::default();
    }
    let Ok(t) = tokio::fs::read_to_string(path).await else {
        return AppSettings::default();
    };
    serde_json::from_str(&t).unwrap_or_default()
}

pub async fn save_app_settings(path: &std::path::PathBuf, s: &AppSettings) -> Result<(), String> {
    s.validate()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let t = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    tokio::fs::write(path, t).await.map_err(|e| e.to_string())?;
    Ok(())
}
