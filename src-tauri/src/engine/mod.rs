//! # Servo埋め込み
//!
//! - **クレート**: 公式 [`servo`](https://crates.io/crates/servo) **0.1**（`default-features = false`）。**Windows のみ** `no-wgl`（mozangle EGL DLL 経路）。Linux/macOS では `mozangle` の EGL が未サポートのため付与しない。
//! - **Windows**: **Visual Studio C++ ビルドツール**（`mozangle`）が必要。Developer PowerShellまたはCIの`ilammy/msvc-dev-cmd`を参照。
//! - **Linux / macOS**: `mozangle` は WGL/EGL の Windows 専用パスを除いたビルド。LLVM（bindgen）が必要な場合があります。

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
    ndi_alpha_enabled: bool,
    ndi_groups: Option<String>,
) -> anyhow::Result<()> {
    servo_thread::run_single_stream(stream_index, cfg, app, stop, ndi_alpha_enabled, ndi_groups)
        .await
}
