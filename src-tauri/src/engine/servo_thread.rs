//! Servo オフスクリーン + NDI。
//!
//! - `Servo` はプロセス内 1 つ、`grafton_ndi::NDI` も `OnceLock` で 1 つ。ストリームごとに `WebView`、
//!   `SoftwareRenderingContext`、NDI `Sender` を持つ。
//! - `Sender::new` / `drop`（`NDIlib_send_destroy`）は grafton-ndi の前提に合わせ FFI 専用スレッド `ndi_ffi_tx` に直列化する。
//! - 停止時は `DropWithAck` で `Sender` を FFI 側へ渡し、ack をホストが非ブロッキングで待ってから
//!   `done_tx` を返す（ソースがネットワークに残るのを防ぎ、ホストの `spin` もブロックしない）。
//! - `fixedFps` は累積デッドライン（`1/fps`）で位相を保ち、`park_until` で次ティックまで待ってから送出する。
//! - 映像は `send_video_async` と `AsyncVideoToken`（drop / `wait`）でゼロコピー送出する（[grafton-ndi `Sender`](https://docs.rs/grafton-ndi)）。ピクセルは `read_to_image` と同じ **RGBA** をそのまま `PixelFormat::RGBA` で送る。
//! - `AsyncVideoToken` が送出バッファを借用するため同一 `Sender` での多重 inflight は組めない; `read_to_image` の Vec とスロット上の `ndi_rgba_buffer` を `mem::swap` してアロケーションを再利用する。
//! - Servo 資源の drop は一括 `Vec::clear` ではなく順に行い `spin` を挟む（相互デッドロック回避）。
//! - NDI アルファ用のシェル透明化は **アプリ設定** `ndi_alpha_enabled` のみで行い、`prefs::shell_background_color_rgba` を切り替える（ページ CSS は上書きしない）。
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
    BorrowedVideoFrame, LineStrideOrSize, PixelFormat, ScanType, Sender as NdiSender,
    SenderOptions, NDI,
};
use servo::{
    EventLoopWaker, Preferences, RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext,
    WebView, WebViewBuilder,
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
    /// `send_video_async` 用 RGBA 非圧縮バッファ（`read_to_image` と同レイアウト、フレームごとに再利用）。
    ndi_rgba_buffer: Vec<u8>,
    /// このストリーム開始時点のアプリ設定 `ndi_alpha_enabled`（ティアダウン後のシェル pref 復帰に使用）。
    ndi_alpha_enabled_at_start: bool,
    /// `fixedFps` の次フレーム送出予定時刻（累積位相）。`None` は初回ティック前。
    fixed_fps_deadline: Option<Instant>,
}

enum HostMessage {
    AddStream {
        stream_index: usize,
        cfg: StreamConfig,
        ndi_alpha_enabled: bool,
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
    let mut g = SERVO_HOST_TX.lock().expect("SERVO_HOST_TX mutex poisoned");
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
    ndi_alpha_enabled: bool,
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
        ndi_alpha_enabled,
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
            return Err(anyhow::anyhow!("Servoスレッドが結果を返さず終了しました"));
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

/// メインループのスロットル（`onDemand` のみ。`fixedFps` 時はデッドライン待ちで律速する）。
const SERVO_HOST_LOOP_SLEEP: Duration = Duration::from_millis(1);

/// アプリ設定に基づき、いずれかの稼働ストリームが透明シェルで開始されていれば `shell_background_color_rgba` を透明にする。
/// `new_stream_wants` は **まだ `slots` に無い**追加ストリームのフラグ。
fn want_transparent_ndi_shell(slots: &HashMap<usize, ActiveStream>, new_stream_wants: bool) -> bool {
    new_stream_wants || slots.values().any(|s| s.ndi_alpha_enabled_at_start)
}

fn apply_servo_shell_clear_transparency(want_transparent: bool) {
    let mut p = servo::prefs::get().clone();
    p.shell_background_color_rgba = if want_transparent {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    };
    servo::prefs::set(p);
}

/// `deadline` まで待機（粗い `sleep` と短い残りの yield で OS 粒度のズレを抑える）。
fn park_until(deadline: Instant) {
    const COARSE: Duration = Duration::from_millis(2);
    loop {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let left = deadline.saturating_duration_since(now);
        if left > COARSE + Duration::from_micros(400) {
            std::thread::sleep(left.saturating_sub(Duration::from_millis(1)));
        } else if left > Duration::from_micros(400) {
            std::thread::sleep(Duration::from_micros(300));
        } else {
            std::thread::yield_now();
        }
    }
}

/// 前ティックのデッドライン `prev` に基づき次の送出予定を進める（遅延時は間引き）。
fn advance_fixed_fps_deadline(slot: &mut ActiveStream, min_dt: Duration, prev: Option<Instant>) {
    let now = Instant::now();
    slot.fixed_fps_deadline = Some(match prev {
        None => now + min_dt,
        Some(d) => {
            let mut n = d + min_dt;
            while n < now {
                n += min_dt;
            }
            n
        }
    });
}

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
            Err(mpsc::TryRecvError::Empty) => t.started_at.elapsed() > NDI_TEARDOWN_TIMEOUT,
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
                    ndi_alpha_enabled,
                    stop,
                    inputs,
                    done_tx,
                    runtime,
                    app,
                }) => {
                    try_add_stream(
                        stream_index,
                        cfg,
                        ndi_alpha_enabled,
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
        if servo.is_some() {
            apply_servo_shell_clear_transparency(want_transparent_ndi_shell(&slots, false));
        }

        flush_deferred_teardown_before_new_streams(&servo, &wake_rx, &mut deferred_teardown);

        while let Ok(HostMessage::AddStream {
            stream_index,
            cfg,
            ndi_alpha_enabled,
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
                ndi_alpha_enabled,
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
        match cmd_rx.try_recv() {
            Ok(HostMessage::AddStream {
                stream_index,
                cfg,
                ndi_alpha_enabled,
                stop,
                inputs,
                done_tx,
                runtime,
                app,
            }) => {
                try_add_stream(
                    stream_index,
                    cfg,
                    ndi_alpha_enabled,
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
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
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

        for slot in slots.values_mut() {
            match slot.cfg.video_send_mode {
                VideoSendMode::FixedFps => {
                    let min_dt = Duration::from_secs_f64(1.0 / slot.cfg.fps.max(1) as f64);
                    let prev_deadline = slot.fixed_fps_deadline;
                    if let Some(d) = prev_deadline {
                        park_until(d);
                    }
                    let paint_res = paint_capture_send_ndi(
                        &slot.runtime,
                        &slot.app,
                        &slot.webview,
                        &slot.rendering_context,
                        &mut slot.sender,
                        &mut slot.ndi_rgba_buffer,
                        &slot.cfg,
                    );
                    if let Err(e) = &paint_res {
                        emit_log_from_worker(
                            &slot.runtime,
                            slot.app.clone(),
                            format!("NDI 送出: {e:#}"),
                        );
                    }
                    advance_fixed_fps_deadline(slot, min_dt, prev_deadline);
                }
                VideoSendMode::OnDemand => {
                    if slot.delegate.needs_paint.replace(false) {
                        match paint_capture_send_ndi(
                            &slot.runtime,
                            &slot.app,
                            &slot.webview,
                            &slot.rendering_context,
                            &mut slot.sender,
                            &mut slot.ndi_rgba_buffer,
                            &slot.cfg,
                        ) {
                            Ok(_) => {}
                            Err(e) => {
                                emit_log_from_worker(
                                    &slot.runtime,
                                    slot.app.clone(),
                                    format!("NDI 送出: {e:#}"),
                                );
                            }
                        }
                    }
                }
            }
        }

        let any_fixed_fps = slots
            .values()
            .any(|s| matches!(s.cfg.video_send_mode, VideoSendMode::FixedFps));
        if !any_fixed_fps {
            std::thread::sleep(SERVO_HOST_LOOP_SLEEP);
        }
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
            ndi_rgba_buffer: _,
            ndi_alpha_enabled_at_start: _,
            fixed_fps_deadline: _,
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
    ndi_alpha_enabled: bool,
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

    // WebRender / GL クリア色を、新しい描画コンテキストが生える前に確定させる
    let want_shell = want_transparent_ndi_shell(slots, ndi_alpha_enabled);
    apply_servo_shell_clear_transparency(want_shell);

    if servo.is_none() {
        emit_log_from_worker(
            &runtime,
            app.clone(),
            format!("Servoを起動しています（ストリーム {stream_index} / NDI ランタイム共有）…"),
        );
        super::kvm_ndi::log_kvm_capability_once();
        let (w_tx, w_rx) = mpsc::channel::<()>();
        let waker: Box<dyn EventLoopWaker> = Box::new(ChannelWaker { tx: w_tx.clone() });
        let mut prefs = Preferences::default();
        if want_shell {
            prefs.shell_background_color_rgba = [0.0, 0.0, 0.0, 0.0];
        }
        let servo_inst = ServoBuilder::default()
            .preferences(prefs)
            .event_loop_waker(waker)
            .build();
        servo_inst.set_delegate(Rc::new(ServoBridge));
        *servo = Some(servo_inst);
        *wake_rx = Some(w_rx);
    }

    let w0 = cfg.width.max(1);
    let h0 = cfg.height.max(1);
    emit_log_from_worker(
        &runtime,
        app.clone(),
        format!("Servo WebView を登録しています（ストリーム {stream_index} / {w0}x{h0}）…",),
    );

    let Some(servo_ref) = servo.as_ref() else {
        let _ = done_tx.send(Err(anyhow::anyhow!("内部エラー: Servo が未初期化です")));
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
            ndi_rgba_buffer: Vec::new(),
            ndi_alpha_enabled_at_start: ndi_alpha_enabled,
            fixed_fps_deadline: None,
        },
    );
    apply_servo_shell_clear_transparency(want_transparent_ndi_shell(slots, false));
}

/// RGBA 非圧縮 1 フレーム分。`buffer` は `PixelFormat::RGBA` の最小長以上であること。
fn borrowed_rgba_frame<'a>(
    buffer: &'a [u8],
    width: i32,
    height: i32,
    frame_rate_n: i32,
    frame_rate_d: i32,
) -> anyhow::Result<BorrowedVideoFrame<'a>> {
    let stride = PixelFormat::RGBA.line_stride(width);
    let need = PixelFormat::RGBA.info().buffer_len(stride, height);
    if buffer.len() < need {
        anyhow::bail!(
            "RGBA buffer too small: got {}, need {} ({}x{})",
            buffer.len(),
            need,
            width,
            height
        );
    }
    Ok(unsafe {
        BorrowedVideoFrame::from_parts_unchecked(
            &buffer[..need],
            width,
            height,
            PixelFormat::RGBA,
            frame_rate_n,
            frame_rate_d,
            width as f32 / height.max(1) as f32,
            ScanType::Progressive,
            0,
            LineStrideOrSize::LineStrideBytes(stride),
            None,
            0,
        )
    })
}

/// NDI へ実際に 1 映像フレームを送れたら `Ok(true)`。スキップ・エラー時は `Ok(false)`。
fn paint_capture_send_ndi(
    runtime: &tokio::runtime::Handle,
    app: &AppHandle,
    webview: &WebView,
    rendering_context: &Rc<SoftwareRenderingContext>,
    sender: &mut NdiSender,
    ndi_rgba_buffer: &mut Vec<u8>,
    cfg: &StreamConfig,
) -> anyhow::Result<bool> {
    if let Err(e) = rendering_context.make_current() {
        emit_log_from_worker(runtime, app.clone(), format!("make_current: {e:?}"));
        return Ok(false);
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
        let mut rgba_bytes = rgba_img.into_raw();
        let fps_n = cfg.fps as i32;
        let fps_d = 1_i32;
        let w = out_w as i32;
        let h = out_h as i32;
        let stride = PixelFormat::RGBA.line_stride(w);
        let need = PixelFormat::RGBA.info().buffer_len(stride, h);
        if rgba_bytes.len() != need {
            emit_log_from_worker(
                runtime,
                app.clone(),
                format!(
                    "read_to_image size mismatch: rgba {} bytes, expected RGBA {} for {}x{}",
                    rgba_bytes.len(),
                    need,
                    out_w,
                    out_h
                ),
            );
            rendering_context.present();
            return Ok(false);
        }
        std::mem::swap(ndi_rgba_buffer, &mut rgba_bytes);
        let borrowed = borrowed_rgba_frame(ndi_rgba_buffer.as_slice(), w, h, fps_n, fps_d)
            .context("NDI borrowed frame")?;
        let token = sender.send_video_async(&borrowed);
        token
            .wait()
            .map_err(|e| anyhow::anyhow!("NDI async video wait: {e}"))?;
        rendering_context.present();
        return Ok(true);
    }
    rendering_context.present();
    Ok(false)
}
