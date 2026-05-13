//! Servo オフスクリーン + NDI。
//!
//! - `Servo` はプロセス内 1 つ、`grafton_ndi::NDI` も `OnceLock` で 1 つ。ストリームごとに `WebView`、
//!   `SoftwareRenderingContext`、NDI `Sender` を持つ。
//! - `Sender::new` / `drop`（`NDIlib_send_destroy`）は grafton-ndi の前提に合わせ FFI 専用スレッド `ndi_ffi_tx` に直列化する。
//! - 停止時は `DropWithAck` で `Sender` を FFI 側へ渡し、ack をホストが非ブロッキングで待ってから
//!   `done_tx` を返す（ソースがネットワークに残るのを防ぎ、ホストの `spin` もブロックしない）。
//! - 映像は同期 `send_video` のみ。`send_video_async` は `flush_null_frame` で停止処理まで届かなくなるため使わない。
//! - Servo 資源の drop は一括 `Vec::clear` ではなく順に行い `spin` を挟む（相互デッドロック回避）。
//! - 新規 `Sender::new` の前に `flush_deferred_teardown_before_new_streams` で旧送出を片付け、mpsc FIFO で destroy→create の順を保証。

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Context;
use dpi::PhysicalSize;
use euclid::{Box2D, Point2D, Scale};
use grafton_ndi::{
    PixelFormat, ScanType, Sender as NdiSender, SenderOptions, VideoFrame, NDI,
};
use servo::{
    EventLoopWaker, RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext, WebView,
    WebViewBuilder,
};
use tauri::AppHandle;
use url::Url;

use super::config::{emit_log_from_worker, StreamConfig, VideoSendMode};
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

struct ActiveStream {
    stream_index: usize,
    cfg: StreamConfig,
    stop: Arc<AtomicBool>,
    inputs: InputQueue,
    done_tx: tokio::sync::oneshot::Sender<anyhow::Result<()>>,
    runtime: tokio::runtime::Handle,
    app: AppHandle,
    webview: WebView,
    delegate: Rc<DelegateState>,
    rendering_context: Rc<SoftwareRenderingContext>,
    sender: NdiSender,
    last_frame: Option<Instant>,
}

enum HostMessage {
    AddStream {
        stream_index: usize,
        cfg: StreamConfig,
        stop: Arc<AtomicBool>,
        inputs: InputQueue,
        done_tx: tokio::sync::oneshot::Sender<anyhow::Result<()>>,
        runtime: tokio::runtime::Handle,
        app: AppHandle,
    },
}

static SERVO_HOST_TX: LazyLock<StdMutex<Option<mpsc::Sender<HostMessage>>>> =
    LazyLock::new(|| StdMutex::new(None));

fn host_command_tx() -> mpsc::Sender<HostMessage> {
    let mut g = SERVO_HOST_TX
        .lock()
        .expect("SERVO_HOST_TX mutex poisoned");
    if let Some(tx) = g.as_ref() {
        return tx.clone();
    }
    let (tx, rx) = mpsc::channel::<HostMessage>();
    let tx_stored = tx.clone();
    std::thread::Builder::new()
        .name("momaku-servo-host".into())
        .spawn(move || servo_host_main(rx))
        .expect("spawn momaku-servo-host");
    *g = Some(tx_stored);
    tx
}

/// 1 ストリーム分をホストスレッドに登録し、`stop` が処理されるまで待機します。
pub async fn run_single_stream(
    stream_index: usize,
    cfg: StreamConfig,
    app: AppHandle,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    cfg.validate().map_err(anyhow::Error::msg)?;

    let runtime = tokio::runtime::Handle::current();

    let input_queue = input::new_input_queue();
    crate::register_stream_input(stream_index, input_queue.clone());

    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<anyhow::Result<()>>();

    let tx = host_command_tx();
    if let Err(e) = tx.send(HostMessage::AddStream {
        stream_index,
        cfg,
        stop,
        inputs: input_queue,
        done_tx,
        runtime: runtime.clone(),
        app: app.clone(),
    }) {
        crate::unregister_stream_input(stream_index);
        return Err(anyhow::anyhow!(
            "Servo ホストスレッドへの送信に失敗しました（終了中？）: {e}"
        ));
    }

    let res = match done_rx.await {
        Ok(r) => r,
        Err(_) => {
            crate::unregister_stream_input(stream_index);
            return Err(anyhow::anyhow!(
                "Servoスレッドが結果を返さず終了しました"
            ));
        }
    };

    crate::unregister_stream_input(stream_index);
    res
}

/// 停止時にホストへ渡す資源の束（NDI 送出手は FFI スレッドへ移送済み）。
struct DeferredTeardown {
    rendering_context: Rc<SoftwareRenderingContext>,
    delegate: Rc<DelegateState>,
    webview: WebView,
}

/// FFI スレッド上での `NDIlib_send_destroy` 完了を待つためのエントリ。
/// ホストスレッドが非ブロッキングでポーリングし、ack を受信したら `done_tx` を送る。
struct PendingNdiTeardown {
    stream_index: usize,
    ndi_name: String,
    url: String,
    ack_rx: mpsc::Receiver<()>,
    done_tx: Option<tokio::sync::oneshot::Sender<anyhow::Result<()>>>,
    runtime: tokio::runtime::Handle,
    app: AppHandle,
    started_at: Instant,
}

const NDI_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(60);

/// ホストループが `AddStream` を待つ間隔（ack 待ちなどで短いポーリングにする）。
const SERVO_HOST_CMD_POLL: Duration = Duration::from_millis(4);
/// メインループのスロットル（CPU占有を抑える）。
const SERVO_HOST_LOOP_SLEEP: Duration = Duration::from_millis(2);

/// プロセス内で唯一の NDI ランタイムハンドル（ストリームや Servo ホストの再起動をまたいで共有）。
static GLOBAL_NDI: OnceLock<NDI> = OnceLock::new();

fn global_ndi() -> Result<&'static NDI, grafton_ndi::Error> {
    if let Some(n) = GLOBAL_NDI.get() {
        return Ok(n);
    }
    let n = NDI::new()?;
    match GLOBAL_NDI.set(n) {
        Ok(()) => {}
        Err(dup) => drop(dup),
    }
    Ok(GLOBAL_NDI.get().expect("GLOBAL_NDI initialized"))
}

enum NdiFfiCmd {
    Create {
        opts: SenderOptions,
        reply_tx: mpsc::Sender<Result<NdiSender, grafton_ndi::Error>>,
    },
    /// 停止時: `NDIlib_send_destroy` 完了までホストが待てるようにする。
    DropWithAck {
        sender: NdiSender,
        ack: mpsc::Sender<()>,
    },
}

static NDI_FFI_TX: OnceLock<mpsc::Sender<NdiFfiCmd>> = OnceLock::new();

fn ndi_ffi_tx() -> &'static mpsc::Sender<NdiFfiCmd> {
    NDI_FFI_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<NdiFfiCmd>();
        std::thread::Builder::new()
            .name("momaku-ndi-ffi".into())
            .spawn(move || {
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        NdiFfiCmd::Create { opts, reply_tx } => {
                            let res = match global_ndi() {
                                Ok(ndi_ref) => NdiSender::new(ndi_ref, &opts),
                                Err(e) => Err(e),
                            };
                            let _ = reply_tx.send(res);
                        }
                        NdiFfiCmd::DropWithAck { sender, ack } => {
                            drop(sender);
                            let _ = ack.send(());
                        }
                    }
                }
            })
            .expect("spawn momaku-ndi-ffi");
        tx
    })
}

fn ndi_ffi_create(opts: SenderOptions) -> Result<NdiSender, grafton_ndi::Error> {
    let (reply_tx, reply_rx) = mpsc::channel();
    ndi_ffi_tx()
        .send(NdiFfiCmd::Create { opts, reply_tx })
        .map_err(|_| {
            grafton_ndi::Error::InitializationFailed("NDI FFI thread disconnected".into())
        })?;
    match reply_rx.recv() {
        Ok(r) => r,
        Err(_) => Err(grafton_ndi::Error::InitializationFailed(
            "NDI FFI create reply channel closed".into(),
        )),
    }
}

/// `Sender` を FFI スレッドへ送って destroy。スレッド終了時はこのスレッドで drop し ack を即送る。
fn queue_ndi_sender_teardown(sender: NdiSender) -> mpsc::Receiver<()> {
    let (ack_tx, ack_rx) = mpsc::channel();
    if let Err(mpsc::SendError(cmd)) = ndi_ffi_tx().send(NdiFfiCmd::DropWithAck {
        sender,
        ack: ack_tx,
    }) {
        let NdiFfiCmd::DropWithAck { sender, ack } = cmd else {
            unreachable!("queue_ndi_sender_teardown only enqueues DropWithAck");
        };
        drop(sender);
        let _ = ack.send(());
    }
    ack_rx
}

/// Servo インスタンスがないとき、`DeferredTeardown` を単純に順に drop する。
fn drop_deferred_stack_plain(deferred_teardown: &mut Vec<DeferredTeardown>) {
    while let Some(t) = deferred_teardown.pop() {
        let DeferredTeardown {
            rendering_context,
            delegate,
            webview,
        } = t;
        drop(webview);
        drop(delegate);
        drop(rendering_context);
    }
}

/// `Vec::clear` 一括 drop は `WebView` と NDI `Sender` が相互にブロックし得るため、分解して `spin` を挟む。
fn teardown_one_stream(servo_ref: &Servo, wrx: &mpsc::Receiver<()>, t: DeferredTeardown) {
    let DeferredTeardown {
        rendering_context,
        delegate,
        webview,
    } = t;
    servo_pump_events(servo_ref, wrx);
    drop(webview);
    servo_pump_events(servo_ref, wrx);
    drop(delegate);
    drop(rendering_context);
    servo_pump_events(servo_ref, wrx);
}

/// Servo のイベントキューを処理する。ウェイクが途切れたら早期終了し、最大回数で打ち切る。
fn servo_pump_events(servo_ref: &Servo, wrx: &mpsc::Receiver<()>) {
    const MAX_IDLE_PASSES: usize = 48;
    const MAX_TOTAL_SPINS: usize = 20_000;
    let mut total = 0usize;
    let mut idle = 0usize;
    while total < MAX_TOTAL_SPINS {
        servo_ref.spin_event_loop();
        total += 1;
        let mut woke = false;
        while wrx.try_recv().is_ok() {
            woke = true;
            servo_ref.spin_event_loop();
            total += 1;
        }
        if woke {
            idle = 0;
        } else {
            idle += 1;
            if idle >= MAX_IDLE_PASSES {
                break;
            }
        }
    }
}

/// `try_recv(AddStream)` より前に実行し、旧 NDI 送出手を破棄してから次の `Sender::new` に進む。
fn flush_deferred_teardown_before_new_streams(
    servo: &Option<Servo>,
    wake_rx: &Option<mpsc::Receiver<()>>,
    deferred_teardown: &mut Vec<DeferredTeardown>,
) {
    if deferred_teardown.is_empty() {
        return;
    }
    let (Some(servo_ref), Some(wrx)) = (servo.as_ref(), wake_rx.as_ref()) else {
        drop_deferred_stack_plain(deferred_teardown);
        return;
    };
    while wrx.try_recv().is_ok() {}
    servo_ref.spin_event_loop();
    while wrx.try_recv().is_ok() {
        servo_ref.spin_event_loop();
    }
    servo_pump_events(servo_ref, wrx);
    while let Some(t) = deferred_teardown.pop() {
        teardown_one_stream(servo_ref, wrx, t);
    }
}

/// `pending_ndi_teardowns` から完了済みエントリを除去し、`done_tx` を送信する（非ブロッキング）。
fn drain_completed_ndi_teardowns(pending: &mut Vec<PendingNdiTeardown>) {
    pending.retain_mut(|t| {
        let completed = match t.ack_rx.try_recv() {
            Ok(()) => true,
            Err(mpsc::TryRecvError::Disconnected) => true,
            Err(mpsc::TryRecvError::Empty) => {
                t.started_at.elapsed() > NDI_TEARDOWN_TIMEOUT
            }
        };
        if completed {
            if let Some(done_tx) = t.done_tx.take() {
                emit_log_from_worker(
                    &t.runtime,
                    t.app.clone(),
                    format!(
                        "NDI送出停止(Servo): [{}] {} ({})",
                        t.stream_index, t.ndi_name, t.url
                    ),
                );
                let _ = done_tx.send(Ok(()));
            }
            false
        } else {
            true
        }
    });
}

fn servo_host_main(cmd_rx: mpsc::Receiver<HostMessage>) {
    let mut servo: Option<Servo> = None;
    let mut wake_rx: Option<mpsc::Receiver<()>> = None;
    let mut slots: HashMap<usize, ActiveStream> = HashMap::new();
    // ティアダウンは `spin` 後に `pop` して順次分解 drop（`Vec::clear` の一括 drop は使わない）。
    let mut deferred_teardown: Vec<DeferredTeardown> = Vec::new();
    let mut pending_ndi_teardowns: Vec<PendingNdiTeardown> = Vec::new();

    loop {
        drain_completed_ndi_teardowns(&mut pending_ndi_teardowns);

        // slots・deferred_teardown・pending_ndi_teardowns がすべて空のときだけブロッキング recv
        if slots.is_empty() && deferred_teardown.is_empty() && pending_ndi_teardowns.is_empty() {
            match cmd_rx.recv() {
                Ok(HostMessage::AddStream {
                    stream_index,
                    cfg,
                    stop,
                    inputs,
                    done_tx,
                    runtime,
                    app,
                }) => {
                    try_add_stream(
                        stream_index,
                        cfg,
                        stop,
                        inputs,
                        done_tx,
                        runtime,
                        app,
                        &mut servo,
                        &mut wake_rx,
                        &mut slots,
                    );
                }
                Err(_) => break,
            }
            continue;
        }

        // `AddStream` より先に停止済みスロットを外す（同一 index の二重登録を防ぐ）
        deferred_teardown.extend(remove_finished_streams(
            &mut slots,
            &mut pending_ndi_teardowns,
        ));

        flush_deferred_teardown_before_new_streams(&servo, &wake_rx, &mut deferred_teardown);

        while let Ok(HostMessage::AddStream {
            stream_index,
            cfg,
            stop,
            inputs,
            done_tx,
            runtime,
            app,
        }) = cmd_rx.try_recv()
        {
            try_add_stream(
                stream_index,
                cfg,
                stop,
                inputs,
                done_tx,
                runtime,
                app,
                &mut servo,
                &mut wake_rx,
                &mut slots,
            );
        }
        match cmd_rx.recv_timeout(SERVO_HOST_CMD_POLL) {
            Ok(HostMessage::AddStream {
                stream_index,
                cfg,
                stop,
                inputs,
                done_tx,
                runtime,
                app,
            }) => {
                try_add_stream(
                    stream_index,
                    cfg,
                    stop,
                    inputs,
                    done_tx,
                    runtime,
                    app,
                    &mut servo,
                    &mut wake_rx,
                    &mut slots,
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        let Some(servo_ref) = servo.as_ref() else {
            drop_deferred_stack_plain(&mut deferred_teardown);
            continue;
        };
        let Some(wrx) = wake_rx.as_ref() else {
            drop_deferred_stack_plain(&mut deferred_teardown);
            continue;
        };

        while wrx.try_recv().is_ok() {}

        for slot in slots.values() {
            let w0 = slot.cfg.width.max(1);
            let h0 = slot.cfg.height.max(1);
            input::drain_and_apply_all(&slot.inputs, &slot.webview, w0, h0);
        }

        servo_ref.spin_event_loop();
        while wrx.try_recv().is_ok() {
            servo_ref.spin_event_loop();
        }

        // spin 後にティアダウンを **pop して順に** drop（`Vec::clear` は相互ブロックし得る）
        if !deferred_teardown.is_empty() {
            servo_pump_events(servo_ref, wrx);
            while let Some(t) = deferred_teardown.pop() {
                teardown_one_stream(servo_ref, wrx, t);
            }
        }

        let now = Instant::now();
        for slot in slots.values_mut() {
            match slot.cfg.video_send_mode {
                VideoSendMode::FixedFps => {
                    let min_dt =
                        Duration::from_secs_f64(1.0 / slot.cfg.fps.max(1) as f64);
                    if slot
                        .last_frame
                        .map(|t| now.duration_since(t) < min_dt)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    slot.last_frame = Some(now);
                    let _ = paint_capture_send_ndi(
                        &slot.runtime,
                        &slot.app,
                        &slot.webview,
                        &slot.rendering_context,
                        &slot.sender,
                        &slot.cfg,
                    );
                }
                VideoSendMode::OnDemand => {
                    if slot.delegate.needs_paint.replace(false) {
                        slot.last_frame = Some(now);
                        let _ = paint_capture_send_ndi(
                            &slot.runtime,
                            &slot.app,
                            &slot.webview,
                            &slot.rendering_context,
                            &slot.sender,
                            &slot.cfg,
                        );
                    }
                }
            }
        }

        std::thread::sleep(SERVO_HOST_LOOP_SLEEP);
    }
}

fn remove_finished_streams(
    slots: &mut HashMap<usize, ActiveStream>,
    pending_ndi_teardowns: &mut Vec<PendingNdiTeardown>,
) -> Vec<DeferredTeardown> {
    let to_remove: Vec<usize> = slots
        .iter()
        .filter(|(_, s)| s.stop.load(Ordering::Acquire))
        .map(|(&i, _)| i)
        .collect();

    let mut deferred = Vec::new();
    for i in to_remove {
        let Some(slot) = slots.remove(&i) else {
            continue;
        };
        let ActiveStream {
            stream_index: _,
            cfg,
            stop: _,
            inputs: _,
            done_tx,
            runtime,
            app,
            webview,
            delegate,
            rendering_context,
            sender,
            last_frame: _,
        } = slot;

        let ack_rx = queue_ndi_sender_teardown(sender);

        pending_ndi_teardowns.push(PendingNdiTeardown {
            stream_index: i,
            ndi_name: cfg.ndi_name.clone(),
            url: cfg.url.clone(),
            ack_rx,
            done_tx: Some(done_tx),
            runtime,
            app,
            started_at: Instant::now(),
        });

        deferred.push(DeferredTeardown {
            rendering_context,
            delegate,
            webview,
        });
    }
    deferred
}

#[allow(clippy::too_many_arguments)]
fn try_add_stream(
    stream_index: usize,
    cfg: StreamConfig,
    stop: Arc<AtomicBool>,
    inputs: InputQueue,
    done_tx: tokio::sync::oneshot::Sender<anyhow::Result<()>>,
    runtime: tokio::runtime::Handle,
    app: AppHandle,
    servo: &mut Option<Servo>,
    wake_rx: &mut Option<mpsc::Receiver<()>>,
    slots: &mut HashMap<usize, ActiveStream>,
) {
    if slots.contains_key(&stream_index) {
        let _ = done_tx.send(Err(anyhow::anyhow!(
            "内部エラー: ストリーム {stream_index} は既にホストに登録されています"
        )));
        return;
    }

    if let Err(e) = cfg.validate() {
        let _ = done_tx.send(Err(anyhow::anyhow!(e)));
        return;
    }

    if let Err(e) = global_ndi() {
        let _ = done_tx.send(Err(anyhow::anyhow!(
            "NDI::new（NDI SDK / ランタイムを確認してください）: {e}"
        )));
        return;
    }

    if servo.is_none() {
        emit_log_from_worker(
            &runtime,
            app.clone(),
            format!("Servoを起動しています（ストリーム {stream_index} / NDI ランタイム共有）…"),
        );
        super::kvm_ndi::log_kvm_capability_once();
        let (w_tx, w_rx) = mpsc::channel::<()>();
        let waker: Box<dyn EventLoopWaker> = Box::new(ChannelWaker { tx: w_tx.clone() });
        let servo_inst = ServoBuilder::default().event_loop_waker(waker).build();
        servo_inst.set_delegate(Rc::new(ServoBridge));
        *servo = Some(servo_inst);
        *wake_rx = Some(w_rx);
    }

    let w0 = cfg.width.max(1);
    let h0 = cfg.height.max(1);
    emit_log_from_worker(
        &runtime,
        app.clone(),
        format!(
            "Servo WebView を登録しています（ストリーム {} / {}x{}）…",
            stream_index, w0, h0
        ),
    );

    let Some(servo_ref) = servo.as_ref() else {
        let _ = done_tx.send(Err(anyhow::anyhow!(
            "内部エラー: Servo が未初期化です"
        )));
        return;
    };

    let rendering_context = match SoftwareRenderingContext::new(PhysicalSize::new(w0, h0)) {
        Ok(rc) => Rc::new(rc),
        Err(e) => {
            let _ = done_tx.send(Err(anyhow::anyhow!(
                "SoftwareRenderingContext::newに失敗: {e:?}"
            )));
            return;
        }
    };
    if let Err(e) = rendering_context.make_current() {
        let _ = done_tx.send(Err(anyhow::anyhow!(
            "SoftwareRenderingContext::make_current: {e:?}"
        )));
        return;
    }

    let rendering_context_dyn: Rc<dyn RenderingContext> =
        Rc::clone(&rendering_context) as Rc<dyn RenderingContext>;

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

    let initial_url = match Url::parse(&cfg.url) {
        Ok(u) => u,
        Err(e) => {
            let _ = done_tx.send(Err(anyhow::anyhow!("URL: {e}")));
            return;
        }
    };
    let webview = WebViewBuilder::new(servo_ref, rendering_context_dyn.clone())
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
    let sender = match ndi_ffi_create(sender_opts) {
        Ok(s) => s,
        Err(e) => {
            let _ = done_tx.send(Err(anyhow::anyhow!("Sender::new: {e}")));
            return;
        }
    };

    webview.resize(PhysicalSize::new(w0, h0));
    webview.show();
    webview.focus();

    emit_log_from_worker(
        &runtime,
        app.clone(),
        format!(
            "NDI送出開始(Servo): [{}] {} ({}) / {:?}",
            stream_index, cfg.ndi_name, cfg.url, cfg.video_send_mode
        ),
    );

    slots.insert(
        stream_index,
        ActiveStream {
            stream_index,
            cfg,
            stop,
            inputs,
            done_tx,
            runtime,
            app,
            webview,
            delegate: delegate_state,
            rendering_context,
            sender,
            last_frame: None,
        },
    );
}

fn paint_capture_send_ndi(
    runtime: &tokio::runtime::Handle,
    app: &AppHandle,
    webview: &WebView,
    rendering_context: &Rc<SoftwareRenderingContext>,
    sender: &NdiSender,
    cfg: &StreamConfig,
) -> anyhow::Result<()> {
    if let Err(e) = rendering_context.make_current() {
        emit_log_from_worker(runtime, app.clone(), format!("make_current: {e:?}"));
        return Ok(());
    }
    webview.paint();
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
