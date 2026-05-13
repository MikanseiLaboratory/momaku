//! Tauriコマンド用のリモート入力ペイロード（Servo非依存）。

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct RemoteInput {
    pub stream_index: usize,
    pub event: RemoteInputEvent,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteInputEvent {
    MouseMove {
        x_norm: f64,
        y_norm: f64,
    },
    MouseDown {
        x_norm: f64,
        y_norm: f64,
        button: String,
    },
    MouseUp {
        x_norm: f64,
        y_norm: f64,
        button: String,
    },
    Wheel {
        x_norm: f64,
        y_norm: f64,
        delta_x: f64,
        delta_y: f64,
    },
    KeyDown {
        keysym: Option<i32>,
        key: Option<String>,
    },
    KeyUp {
        keysym: Option<i32>,
        key: Option<String>,
    },
}
