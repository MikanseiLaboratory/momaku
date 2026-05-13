//! Servoオフスクリーン + NDI。ストリーム **ごとに** 専用スレッドで `Servo` / `SoftwareRenderingContext`を1組ずつ起動します。

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use dpi::PhysicalSize;
use euclid::{Box2D, Point2D, Scale};
use grafton_ndi::{PixelFormat, ScanType, Sender, SenderOptions, VideoFrame, NDI};
use servo::{
    EventLoopWaker, RenderingContext, ServoBuilder, SoftwareRenderingContext, WebView,
    WebViewBuilder,
};
use tauri::AppHandle;
use tokio::sync::watch;
use url::Url;

use super::config::{emit_log, StreamConfig};
use super::input::{self, InputQueue};
use super::servo_delegate::{DelegateState, ServoBridge, WebViewBridge};

struct ChannelWaker {
    tx: mpsc::Sender<()>,
}

impl EventLoopWaker for ChannelWaker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(ChannelWaker {
            tx: self.tx.clone(),
        })
    }

    fn wake(&self) {
        let _ = self.tx.send(());
    }
}

struct StreamSlot {
    webview: WebView,
    delegate: Rc<DelegateState>,
}

/// 1 ストリーム分のServo + NDIを`shutdown_rx`がtrueになるまで実行します。
pub async fn run_single_stream(
    stream_index: usize,
    cfg: StreamConfig,
    app: AppHandle,
    shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    cfg.validate().map_err(anyhow::Error::msg)?;

    let input_queue = input::new_input_queue();
    crate::register_stream_input(stream_index, input_queue.clone());

    let stop = Arc::new(AtomicBool::new(false));
    let stop_bg = stop.clone();
    let mut shutdown_bg = shutdown_rx.clone();
    tokio::spawn(async move {
        loop {
            if *shutdown_bg.borrow() {
                stop_bg.store(true, Ordering::SeqCst);
                break;
            }
            if shutdown_bg.changed().await.is_err() {
                break;
            }
            if *shutdown_bg.borrow() {
                stop_bg.store(true, Ordering::SeqCst);
                break;
            }
        }
    });

    let app_thread = app.clone();
    let input_for_thread = input_queue.clone();
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<anyhow::Result<()>>();
    std::thread::Builder::new()
        .name(format!("momaku-servo-{stream_index}"))
        .spawn(move || {
            let res = servo_single_thread_main(stream_index, cfg, app_thread, stop, input_for_thread);
            let _ = done_tx.send(res);
        })
        .with_context(|| format!("spawn servo thread (stream {stream_index})"))?;

    let res = done_rx
        .await
        .map_err(|_| anyhow::anyhow!("Servoスレッドが結果を返さず終了しました"))?;
    crate::unregister_stream_input(stream_index);
    res
}

fn servo_single_thread_main(
    stream_index: usize,
    cfg: StreamConfig,
    app: AppHandle,
    stop: Arc<AtomicBool>,
    inputs: InputQueue,
) -> anyhow::Result<()> {
    let _ = emit_log(
        &app,
        format!("NDIを初期化しています（ストリーム {}）…", stream_index),
    );
    super::kvm_ndi::log_kvm_capability_once();
    let ndi = NDI::new().context("NDI::new（NDI SDK / ランタイムを確認してください）")?;

    let w0 = cfg.width.max(1);
    let h0 = cfg.height.max(1);
    let _ = emit_log(
        &app,
        format!(
            "Servoを起動しています（ストリーム {} / {}x{}）…",
            stream_index, w0, h0
        ),
    );

    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let waker: Box<dyn EventLoopWaker> = Box::new(ChannelWaker { tx: wake_tx });

    let rendering_context = SoftwareRenderingContext::new(PhysicalSize::new(w0, h0))
        .map_err(|e| anyhow::anyhow!("SoftwareRenderingContext::newに失敗: {e:?}"))?;
    let rendering_context = Rc::new(rendering_context);
    rendering_context
        .make_current()
        .map_err(|e| anyhow::anyhow!("SoftwareRenderingContext::make_current: {e:?}"))?;

    let servo = ServoBuilder::default().event_loop_waker(waker).build();
    servo.set_delegate(Rc::new(ServoBridge));

    let rendering_context_dyn: Rc<dyn RenderingContext> = Rc::clone(&rendering_context) as _;

    let delegate_state = Rc::new(DelegateState {
        webview: std::cell::RefCell::new(None),
        rendering_context: std::cell::RefCell::new(Some(rendering_context_dyn.clone())),
        pending_popup_webview: std::cell::RefCell::new(None),
        new_webview_handler: std::cell::RefCell::new(None),
        needs_paint: std::cell::Cell::new(false),
        current_cursor: std::cell::Cell::new(servo::Cursor::Default),
        current_url: std::cell::RefCell::new(None),
        current_title: std::cell::RefCell::new(None),
        status_text: std::cell::RefCell::new(None),
        load_status: std::cell::Cell::new(servo::LoadStatus::Started),
    });

    let bridge = Rc::new(WebViewBridge {
        state: Rc::clone(&delegate_state),
    });

    let initial_url = Url::parse(&cfg.url).map_err(|e| anyhow::anyhow!("URL: {e}"))?;
    let webview = WebViewBuilder::new(&servo, rendering_context_dyn.clone())
        .delegate(bridge)
        .hidpi_scale_factor(Scale::new(1.0_f32))
        .url(initial_url)
        .build();

    *delegate_state.webview.borrow_mut() = Some(webview.clone());

    let mut sender_builder = SenderOptions::builder(&cfg.ndi_name)
        .clock_video(cfg.ndi_clock_video)
        .clock_audio(cfg.ndi_clock_audio);
    if let Some(ref g) = cfg.ndi_groups {
        let t = g.trim();
        if !t.is_empty() {
            sender_builder = sender_builder.groups(t);
        }
    }
    let sender_opts = sender_builder.build();
    let sender = Sender::new(&ndi, &sender_opts).context("Sender::new")?;

    let slot = StreamSlot {
        webview: webview.clone(),
        delegate: delegate_state,
    };

    slot.webview.resize(PhysicalSize::new(w0, h0));
    slot.webview.show();
    slot.webview.focus();

    let _ = emit_log(
        &app,
        format!(
            "NDI送出開始(Servo): [{}] {} ({})",
            stream_index, cfg.ndi_name, cfg.url
        ),
    );

    while !stop.load(Ordering::Relaxed) {
        while wake_rx.try_recv().is_ok() {}

        input::drain_and_apply_all(&inputs, &slot.webview, w0, h0);

        servo.spin_event_loop();
        while wake_rx.try_recv().is_ok() {
            servo.spin_event_loop();
        }

        if slot.delegate.needs_paint.replace(false) {
            if let Err(e) = rendering_context.make_current() {
                let _ = emit_log(&app, format!("make_current: {e:?}"));
                continue;
            }
            slot.webview.paint();
            let size = rendering_context.size();
            let rect = Box2D::new(
                Point2D::new(0, 0),
                Point2D::new(size.width as i32, size.height as i32),
            );
            if let Some(rgba_img) = rendering_context.read_to_image(rect) {
                let out_w = rgba_img.width();
                let out_h = rgba_img.height();
                let rgba_bytes = rgba_img.into_raw();
                let fps_n = cfg.fps as i32;
                let fps_d = 1_i32;
                let mut video = VideoFrame::builder()
                    .resolution(out_w as i32, out_h as i32)
                    .pixel_format(PixelFormat::BGRA)
                    .frame_rate(fps_n, fps_d)
                    .aspect_ratio(out_w as f32 / out_h.max(1) as f32)
                    .scan_type(ScanType::Progressive)
                    .build()
                    .context("VideoFrame::build")?;
                bgra_fill_from_rgba(&mut video.data, &rgba_bytes);
                sender.send_video(&video);
            }
            rendering_context.present();
        }

        let fps = cfg.fps.max(1);
        std::thread::sleep(Duration::from_secs_f64(1.0 / fps as f64));
    }

    let _ = emit_log(
        &app,
        format!(
            "NDI送出停止(Servo): [{}] {} ({})",
            stream_index, cfg.ndi_name, cfg.url
        ),
    );
    Ok(())
}

fn bgra_fill_from_rgba(dst: &mut [u8], src: &[u8]) {
    debug_assert_eq!(dst.len(), src.len());
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        d[0] = s[2];
        d[1] = s[1];
        d[2] = s[0];
        d[3] = s[3];
    }
}
