use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use tauri::Emitter;

fn default_video_send_mode() -> VideoSendMode {
    VideoSendMode::FixedFps
}

fn default_frame_buffer() -> u32 {
    0
}

/// ストリームあたりのフレームバッファ（先確保する RGBA 生バッファ本数）の上限。
pub const STREAM_FRAME_BUFFER_CAP: u32 = 30;

/// 映像の `paint` と NDI 送出のタイミング（JSON では `"fixedFps"` / `"onDemand"` / `"hybrid"`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoSendMode {
    /// 累積デッドラインで間隔 `1/fps` を維持し、その時刻まで待機してから `paint` + NDI 送出する。
    FixedFps,
    /// Servo が `needs_paint` を立てたときのみ `paint` + 送出する。
    OnDemand,
    /// `FixedFps` と同様に FPS を維持しつつ、`needs_paint` のときは待たずに即 1 フレーム送出する。
    Hybrid,
}

impl Serialize for VideoSendMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            VideoSendMode::FixedFps => "fixedFps",
            VideoSendMode::OnDemand => "onDemand",
            VideoSendMode::Hybrid => "hybrid",
        })
    }
}

struct VideoSendModeStrVisitor;

impl Visitor<'_> for VideoSendModeStrVisitor {
    type Value = VideoSendMode;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("\"fixedFps\", \"onDemand\", or \"hybrid\"")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v {
            "fixedFps" | "FixedFps" => Ok(VideoSendMode::FixedFps),
            "onDemand" | "OnDemand" => Ok(VideoSendMode::OnDemand),
            "hybrid" | "Hybrid" => Ok(VideoSendMode::Hybrid),
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
    /// NDI 送出用に先確保する RGBA フレーム相当バッファの本数（0 で無効、最大 [`STREAM_FRAME_BUFFER_CAP`]）。
    #[serde(default = "default_frame_buffer")]
    pub frame_buffer: u32,
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
        if self.frame_buffer > STREAM_FRAME_BUFFER_CAP {
            return Err(format!(
                "フレームバッファは0〜{STREAM_FRAME_BUFFER_CAP}の範囲にしてください"
            ));
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
    /// `streams.json` のストリームインデックスと対応（`true` = 当該ストリームが送出中）
    pub streams_running: Vec<bool>,
}

pub(crate) fn emit_log(app: &tauri::AppHandle, message: String) -> tauri::Result<()> {
    app.emit("engine-log", EngineLogPayload { message })
}

/// Servo 用の **Tokio ブロッキングワーカー**から直接 `emit` するとメインスレッド待ちでデッドロックし得るため、
/// Tokio ランタイム上で非同期に送る（失敗時は `tracing` のみ）。
pub(crate) fn emit_log_from_worker(
    runtime: &tokio::runtime::Handle,
    app: tauri::AppHandle,
    message: String,
) {
    drop(runtime.spawn(async move {
        let _ = emit_log(&app, message);
    }));
}
