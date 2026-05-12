//! # Servo 埋め込み
//!
//! - **クレート**: 公式 [`servo`](https://crates.io/crates/servo) **0.1**（`default-features = false` + `no-wgl`）。
//! - **Windows**: **Visual Studio C++ ビルドツール**（`mozangle`）が必要。Developer PowerShell または CI の `ilammy/msvc-dev-cmd` を参照。
//! - **Linux / macOS**: 本リポジトリの CI は Windows ネイティブ中心。他 OSはローカルで `cargo check` を確認してください。

mod config;
mod input;
mod kvm_ndi;
mod remote_input;
mod servo_delegate;
mod servo_thread;

pub use config::{EngineLogPayload, EngineStatusPayload, StreamConfig};
pub use remote_input::RemoteInput;

/// 全ストリームの NDI 送出を開始し、`shutdown_rx` が true になるまでブロックします。
pub async fn run_all(
    streams: Vec<StreamConfig>,
    app: tauri::AppHandle,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    servo_thread::run_all(streams, app, shutdown_rx).await
}
