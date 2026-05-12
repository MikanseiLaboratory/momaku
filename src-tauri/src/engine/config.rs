use serde::{Deserialize, Serialize};
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamConfig {
    pub url: String,
    pub ndi_name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl StreamConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.url.trim().is_empty() {
            return Err("URL が空です".into());
        }
        if self.ndi_name.trim().is_empty() {
            return Err("NDI 名が空です".into());
        }
        if self.width < 64 || self.height < 64 {
            return Err("幅・高さは 64 以上にしてください".into());
        }
        if !(1..=120).contains(&self.fps) {
            return Err("FPS は 1〜120 の範囲にしてください".into());
        }
        Ok(())
    }

    /// 共有 1 つのオフスクリーン描画面のため、複数ストリーム時は解像度を揃える必要があります。
    pub fn validate_servo_bundle(streams: &[StreamConfig]) -> Result<(), String> {
        if streams.is_empty() {
            return Err("ストリームが空です".into());
        }
        let w0 = streams[0].width;
        let h0 = streams[0].height;
        for (i, s) in streams.iter().enumerate() {
            if s.width != w0 || s.height != h0 {
                return Err(format!(
                    "Servo エンジンでは全ストリームの幅・高さを一致させてください（行0: {}x{}、行{}: {}x{}）",
                    w0, h0, i, s.width, s.height
                ));
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
pub struct EngineStatusPayload {
    pub running: bool,
}

pub(crate) fn emit_log(app: &tauri::AppHandle, message: String) -> tauri::Result<()> {
    app.emit("engine-log", EngineLogPayload { message })
}
