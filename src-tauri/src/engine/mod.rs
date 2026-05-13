//! # Servo埋め込み
//!
//! - **クレート**: 公式 [`servo`](https://crates.io/crates/servo) **0.1**（`default-features = false` + `no-wgl`）。
//! - **Windows**: **Visual Studio C++ ビルドツール**（`mozangle`）が必要。Developer PowerShellまたはCIの`ilammy/msvc-dev-cmd`を参照。
//! - **Linux / macOS**: 本リポジトリのCIはWindowsネイティブ中心。他OSはローカルで`cargo check`を確認してください。

mod config;
mod input;
mod kvm_ndi;
mod remote_input;
mod servo_delegate;
mod servo_thread;

pub use config::{EngineLogPayload, EngineStatusPayload, StreamConfig, VideoSendMode};
pub use input::InputQueue;
pub use remote_input::RemoteInput;

/// 1 ストリーム分を起動し、`stop` が処理されるまでブロックします（`Servo` はプロセス内で共有）。
pub async fn run_single_stream(
    stream_index: usize,
    cfg: StreamConfig,
    app: tauri::AppHandle,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<()> {
    servo_thread::run_single_stream(stream_index, cfg, app, stop).await
}
