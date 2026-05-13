//! リモート入力キュー → Servo`WebView::notify_input_event`。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use euclid::Point2D;
use servo::{
    CSSPixel, Code as ServoCode, InputEvent, Key as ServoKey, KeyState, KeyboardEvent, Location,
    Modifiers as ServoModifiers, MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent,
    NamedKey as ServoNamedKey, WebView, WebViewPoint, WheelDelta, WheelEvent, WheelMode,
};

use super::remote_input::{RemoteInput, RemoteInputEvent};

/// 共有キュー（エンジン稼働中にTauriコマンドがpush）。
pub type InputQueue = Arc<Mutex<VecDeque<RemoteInput>>>;

pub fn new_input_queue() -> InputQueue {
    Arc::new(Mutex::new(VecDeque::new()))
}

/// 単一WebView用。キューは当該ストリーム専用であること（`submit_remote_input`がindexで振り分ける）。
pub fn drain_and_apply_all(queue: &InputQueue, webview: &WebView, w: u32, h: u32) {
    let mut q = match queue.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    while let Some(ev) = q.pop_front() {
        apply_one(webview, w, h, &ev.event);
    }
}

fn apply_one(webview: &WebView, w: u32, h: u32, ev: &RemoteInputEvent) {
    let scale_w = w.max(1) as f64;
    let scale_h = h.max(1) as f64;
    match ev {
        RemoteInputEvent::MouseMove { x_norm, y_norm } => {
            let pt = norm_point(*x_norm, *y_norm, scale_w, scale_h);
            let _ = webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(pt)));
        }
        RemoteInputEvent::MouseDown {
            x_norm,
            y_norm,
            button,
        } => {
            let pt = norm_point(*x_norm, *y_norm, scale_w, scale_h);
            let _ = webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                MouseButtonAction::Down,
                parse_button(button),
                pt,
            )));
        }
        RemoteInputEvent::MouseUp {
            x_norm,
            y_norm,
            button,
        } => {
            let pt = norm_point(*x_norm, *y_norm, scale_w, scale_h);
            let _ = webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                MouseButtonAction::Up,
                parse_button(button),
                pt,
            )));
        }
        RemoteInputEvent::Wheel {
            x_norm,
            y_norm,
            delta_x,
            delta_y,
        } => {
            let pt = norm_point(*x_norm, *y_norm, scale_w, scale_h);
            let _ = webview.notify_input_event(InputEvent::Wheel(WheelEvent::new(
                WheelDelta {
                    x: *delta_x,
                    y: *delta_y,
                    z: 0.0,
                    mode: WheelMode::DeltaPixel,
                },
                pt,
            )));
        }
        RemoteInputEvent::KeyDown { keysym, key } => {
            let (k, c) = keysym_or_text(*keysym, key.as_deref());
            let _ =
                webview.notify_input_event(InputEvent::Keyboard(KeyboardEvent::new_without_event(
                    KeyState::Down,
                    k,
                    c,
                    Location::Standard,
                    ServoModifiers::empty(),
                    false,
                    false,
                )));
        }
        RemoteInputEvent::KeyUp { keysym, key } => {
            let (k, c) = keysym_or_text(*keysym, key.as_deref());
            let _ =
                webview.notify_input_event(InputEvent::Keyboard(KeyboardEvent::new_without_event(
                    KeyState::Up,
                    k,
                    c,
                    Location::Standard,
                    ServoModifiers::empty(),
                    false,
                    false,
                )));
        }
    }
}

fn norm_point(x_norm: f64, y_norm: f64, w: f64, h: f64) -> WebViewPoint {
    let x = (x_norm.clamp(0.0, 1.0) * w) as f32;
    let y = (y_norm.clamp(0.0, 1.0) * h) as f32;
    WebViewPoint::from(Point2D::<f32, CSSPixel>::new(x, y))
}

fn parse_button(s: &str) -> MouseButton {
    match s.to_ascii_lowercase().as_str() {
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        _ => MouseButton::Left,
    }
}

fn keysym_or_text(keysym: Option<i32>, text_key: Option<&str>) -> (ServoKey, ServoCode) {
    if let Some(ks) = keysym {
        return match ks {
            0xff08 => (
                ServoKey::Named(ServoNamedKey::Backspace),
                ServoCode::Backspace,
            ),
            0xff09 => (ServoKey::Named(ServoNamedKey::Tab), ServoCode::Tab),
            0xff0d => (ServoKey::Named(ServoNamedKey::Enter), ServoCode::Enter),
            0xff1b => (ServoKey::Named(ServoNamedKey::Escape), ServoCode::Escape),
            0xff51 => (
                ServoKey::Named(ServoNamedKey::ArrowLeft),
                ServoCode::ArrowLeft,
            ),
            0xff52 => (ServoKey::Named(ServoNamedKey::ArrowUp), ServoCode::ArrowUp),
            0xff53 => (
                ServoKey::Named(ServoNamedKey::ArrowRight),
                ServoCode::ArrowRight,
            ),
            0xff54 => (
                ServoKey::Named(ServoNamedKey::ArrowDown),
                ServoCode::ArrowDown,
            ),
            0xff50 => (ServoKey::Named(ServoNamedKey::Home), ServoCode::Home),
            0xff57 => (ServoKey::Named(ServoNamedKey::End), ServoCode::End),
            0xff55 => (ServoKey::Named(ServoNamedKey::PageUp), ServoCode::PageUp),
            0xff56 => (
                ServoKey::Named(ServoNamedKey::PageDown),
                ServoCode::PageDown,
            ),
            0xffff => (ServoKey::Named(ServoNamedKey::Delete), ServoCode::Delete),
            0xff63 => (ServoKey::Named(ServoNamedKey::Insert), ServoCode::Insert),
            0xffbe => (ServoKey::Named(ServoNamedKey::F1), ServoCode::F1),
            0xffbf => (ServoKey::Named(ServoNamedKey::F2), ServoCode::F2),
            0xffc0 => (ServoKey::Named(ServoNamedKey::F3), ServoCode::F3),
            0xffc1 => (ServoKey::Named(ServoNamedKey::F4), ServoCode::F4),
            0xffc2 => (ServoKey::Named(ServoNamedKey::F5), ServoCode::F5),
            0xffc3 => (ServoKey::Named(ServoNamedKey::F6), ServoCode::F6),
            0xffc4 => (ServoKey::Named(ServoNamedKey::F7), ServoCode::F7),
            0xffc5 => (ServoKey::Named(ServoNamedKey::F8), ServoCode::F8),
            0xffc6 => (ServoKey::Named(ServoNamedKey::F9), ServoCode::F9),
            0xffc7 => (ServoKey::Named(ServoNamedKey::F10), ServoCode::F10),
            0xffc8 => (ServoKey::Named(ServoNamedKey::F11), ServoCode::F11),
            0xffc9 => (ServoKey::Named(ServoNamedKey::F12), ServoCode::F12),
            ks if ks > 0 && ks < 0x10000 => {
                if let Some(ch) = char::from_u32(ks as u32).filter(|c| !c.is_control()) {
                    (ServoKey::Character(ch.to_string()), ServoCode::Unidentified)
                } else {
                    (
                        ServoKey::Named(ServoNamedKey::Unidentified),
                        ServoCode::Unidentified,
                    )
                }
            }
            _ => (
                ServoKey::Named(ServoNamedKey::Unidentified),
                ServoCode::Unidentified,
            ),
        };
    }
    if let Some(t) = text_key.filter(|s| !s.is_empty()) {
        return (
            ServoKey::Character(t.chars().next().unwrap_or(' ').to_string()),
            ServoCode::Unidentified,
        );
    }
    (
        ServoKey::Named(ServoNamedKey::Unidentified),
        ServoCode::Unidentified,
    )
}
