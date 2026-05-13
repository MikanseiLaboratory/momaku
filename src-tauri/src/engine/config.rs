use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use tauri::Emitter;

fn default_true() -> bool {
    true
}

fn default_video_send_mode() -> VideoSendMode {
    VideoSendMode::FixedFps
}

/// 映像の `paint` と NDI 送出のタイミング（JSON では `"fixedFps"` / `"onDemand"` 文字列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoSendMode {
    /// ループを FPS で回し、毎ティック `paint` して NDI 送出する。
    FixedFps,
    /// Servo が `needs_paint` を立てたときのみ `paint` + 送出する。
    OnDemand,
}

impl Serialize for VideoSendMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            VideoSendMode::FixedFps => "fixedFps",
            VideoSendMode::OnDemand => "onDemand",
        })
    }
}

struct VideoSendModeStrVisitor;

impl Visitor<'_> for VideoSendModeStrVisitor {
    type Value = VideoSendMode;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("\"fixedFps\" or \"onDemand\"")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v {
            "fixedFps" | "FixedFps" => Ok(VideoSendMode::FixedFps),
            "onDemand" | "OnDemand" => Ok(VideoSendMode::OnDemand),
            _ => Err(E::custom(format!("未知の videoSendMode: {v}"))),
        }
    }
}

impl<'de> Deserialize<'de> for VideoSendMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(VideoSendModeStrVisitor)
    }
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
    #[serde(default = "default_video_send_mode")]
    pub video_send_mode: VideoSendMode,
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

/// Servo 用 **std::thread** から `emit` するとメインスレッド待ちでデッドロックし得るため、
/// Tokio ランタイム上で非同期に送る（失敗時は `tracing` のみ）。
pub(crate) fn emit_log_from_worker(
    runtime: &tokio::runtime::Handle,
    app: tauri::AppHandle,
    message: String,
) {
    let _ = runtime.spawn(async move {
        let _ = emit_log(&app, message);
    });
}
