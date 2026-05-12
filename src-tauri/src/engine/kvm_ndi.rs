//! NDI Advanced SDK（`ndi-kvm` feature）と Studio Monitor KVM のギャップを明示します。
//!
//! `grafton-ndi` 0.11 ソースを確認したところ、**KVM / `NDIlib_send_*_kvm` 相当の Rust API は存在しません**。
//! `advanced_sdk` feature は主に非同期送信完了コールバック向けです。

pub fn log_kvm_capability_once() {
    #[cfg(feature = "ndi-kvm")]
    {
        tracing::warn!(
            target: "momaku_lib",
            "ndi-kvm: grafton-ndi に Sender 側 KVM のバインディングはありません。Studio Monitor の KVM は未対応です（`submit_remote_input` を利用してください）。"
        );
    }
}
