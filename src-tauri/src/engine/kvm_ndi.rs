//! NDI Advanced SDK（`ndi-kvm` feature）とStudio Monitor KVMのギャップを明示します。
//!
//! `grafton-ndi` 0.11ソースを確認したところ、**KVM / `NDIlib_send_*_kvm`相当のRust APIは存在しません**。
//! `advanced_sdk` featureは主に非同期送信完了コールバック向けです。

pub fn log_kvm_capability_once() {
    #[cfg(feature = "ndi-kvm")]
    {
        tracing::warn!(
            target: "momaku_lib",
            "ndi-kvm: grafton-ndiにSender側KVMのバインディングはありません。Studio MonitorのKVMは未対応です（`submit_remote_input`を利用してください）。"
        );
    }
}
