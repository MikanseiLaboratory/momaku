use serde::{Deserialize, Serialize};
use tauri::Emitter;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamConfig {
    pub url: String,
    pub ndi_name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// NDIグループ（空・未設定はNone）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ndi_groups: Option<String>,
    #[serde(default = "default_true")]
    pub ndi_clock_video: bool,
    #[serde(default = "default_true")]
    pub ndi_clock_audio: bool,
}

impl StreamConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.url.trim().is_empty() {
            return Err("URLが空です".into());
        }
        if self.ndi_name.trim().is_empty() {
            return Err("NDI名が空です".into());
        }
        if self.width < 64 || self.height < 64 {
            return Err("幅・高さは 64 以上にしてください".into());
        }
        if !(1..=120).contains(&self.fps) {
            return Err("FPSは1〜120の範囲にしてください".into());
        }
        if let Some(ref g) = self.ndi_groups {
            if g.len() > 256 {
                return Err("NDIグループは256文字以内にしてください".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, serde::Serialize)]
pub struct EngineLogPayload {
    pub message: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatusPayload {
    pub running: bool,
    /// `streams.json`の行インデックスと対応（`true`=当該行が送出中）
    pub streams_running: Vec<bool>,
}

pub(crate) fn emit_log(app: &tauri::AppHandle, message: String) -> tauri::Result<()> {
    app.emit("engine-log", EngineLogPayload { message })
}
