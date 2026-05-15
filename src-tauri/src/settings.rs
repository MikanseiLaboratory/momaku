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
    /// NDI 送出用に Servo のシェルクリアを透明にする（`prefs::shell_background_color_rgba`）。ページの CSS は変更しない。
    #[serde(default)]
    pub ndi_alpha_enabled: bool,
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.default_ndi_groups.len() > 256 {
            return Err("既定のNDIグループは256文字以内にしてください".into());
        }
        Ok(())
    }
}

/// 行の NDI グループが空のとき、アプリ既定を使う（送出時に適用。`streams.json` は書き換えない）。
pub fn effective_ndi_groups_for_stream(
    row_ndi: &Option<String>,
    settings: &AppSettings,
) -> Option<String> {
    if let Some(ref g) = row_ndi {
        let t = g.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
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
