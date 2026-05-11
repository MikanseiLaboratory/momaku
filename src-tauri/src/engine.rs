use anyhow::Context;
use base64::Engine;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::emulation::{
    ClearDeviceMetricsOverrideParams, SetDeviceMetricsOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::page::{
    EventScreencastFrame, NavigateParams, ScreencastFrameAckParams, StartScreencastFormat,
    StartScreencastParams, StopScreencastParams,
};
use chromiumoxide::page::Page;
use futures::StreamExt;
use grafton_ndi::{NDI, PixelFormat, ScanType, Sender, SenderOptions, VideoFrame};
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use tokio::task::JoinSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamConfig {
    pub url: String,
    pub ndi_name: String,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: i64,
    pub screencast_every_nth_frame: Option<i64>,
}

fn default_width() -> u32 {
    1280
}

fn default_height() -> u32 {
    720
}

fn default_fps() -> u32 {
    30
}

fn default_jpeg_quality() -> i64 {
    85
}

impl StreamConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.url.trim().is_empty() {
            return Err("URL が空です".into());
        }
        if self.ndi_name.trim().is_empty() {
            return Err("NDI 名が空です".into());
        }
        if self.width < 64 || self.height < 64 {
            return Err("幅・高さは 64 以上にしてください".into());
        }
        if !(1..=120).contains(&self.fps) {
            return Err("FPS は 1〜120 の範囲にしてください".into());
        }
        if !(1..=100).contains(&self.jpeg_quality) {
            return Err("JPEG 品質は 1〜100 にしてください".into());
        }
        if let Some(n) = self.screencast_every_nth_frame {
            if n < 1 {
                return Err("everyNth は 1 以上か空にしてください".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, serde::Serialize)]
pub struct EngineLogPayload {
    pub message: String,
}

#[derive(Clone, serde::Serialize)]
pub struct EngineStatusPayload {
    pub running: bool,
}

fn emit_log(app: &AppHandle, message: String) -> tauri::Result<()> {
    app.emit("engine-log", EngineLogPayload { message })
}

pub async fn run_all(
    streams: Vec<StreamConfig>,
    app: AppHandle,
    shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    for s in &streams {
        s.validate().map_err(anyhow::Error::msg)?;
    }

    let _ = emit_log(&app, "NDI を初期化しています…".into());
    let ndi = NDI::new().context("NDI::new（NDI SDK / ランタイムを確認してください）")?;

    let browser_config = BrowserConfig::builder()
        .viewport(None)
        .new_headless_mode()
        .no_sandbox()
        .build()
        .map_err(|e| anyhow::anyhow!("BrowserConfig: {e}"))?;

    let _ = emit_log(&app, "Chromium を起動しています…".into());
    let (browser, mut handler) = Browser::launch(browser_config)
        .await
        .context("Chromium の起動に失敗しました（Chrome/Chromium のインストールを確認）")?;

    let _ = emit_log(&app, "Chromium に接続しました".into());

    let handler_task = tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    let mut pages = Vec::new();
    for _ in 0..streams.len() {
        if *shutdown_rx.borrow() {
            break;
        }
        pages.push(
            browser
                .new_page("about:blank")
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?,
        );
    }

    if pages.len() != streams.len() {
        handler_task.abort();
        anyhow::bail!("ページ作成が中断されました");
    }

    let mut set = JoinSet::new();
    for (cfg, page) in streams.into_iter().zip(pages) {
        let ndi_c = ndi.clone();
        let app_c = app.clone();
        let rx = shutdown_rx.clone();
        set.spawn(async move {
            if let Err(e) = run_stream(page, cfg, ndi_c, rx, app_c.clone()).await {
                let _ = emit_log(
                    &app_c,
                    format!("ストリーム終了: {:#}", e),
                );
            }
        });
    }

    let mut shutdown_wait = shutdown_rx.clone();
    let app_wait = app.clone();
    tokio::select! {
        _ = async {
            loop {
                if *shutdown_wait.borrow() {
                    break;
                }
                if shutdown_wait.changed().await.is_err() {
                    break;
                }
            }
        } => {
            set.abort_all();
            let _ = emit_log(&app_wait, "停止要求を受け取りました".into());
        }
        _ = async {
            while let Some(joined) = set.join_next().await {
                if let Err(e) = joined {
                    let _ = emit_log(&app_wait, format!("タスクエラー: {e}"));
                }
            }
        } => {}
    }

    handler_task.abort();
    let _ = emit_log(&app, "エンジンを終了しました".into());
    Ok(())
}

async fn run_stream(
    page: Page,
    cfg: StreamConfig,
    ndi: NDI,
    mut shutdown_rx: watch::Receiver<bool>,
    app: AppHandle,
) -> anyhow::Result<()> {
    let w = cfg.width as i64;
    let h = cfg.height as i64;

    page.execute(SetDeviceMetricsOverrideParams::new(w, h, 1.0_f64, false))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("SetDeviceMetricsOverride")?;

    page.goto(NavigateParams::new(cfg.url.clone()))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("navigate")?;

    page.wait_for_navigation()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("wait_for_navigation")?;

    let mut sc_events = page
        .event_listener::<EventScreencastFrame>()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("event_listener")?;

    let mut start = StartScreencastParams::builder()
        .format(StartScreencastFormat::Jpeg)
        .quality(cfg.jpeg_quality.clamp(1, 100))
        .max_width(w)
        .max_height(h);

    if let Some(n) = cfg.screencast_every_nth_frame.filter(|&n| n >= 1) {
        start = start.every_nth_frame(n);
    }

    page.execute(start.build())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("StartScreencast")?;

    let fps_n = cfg.fps as i32;
    let fps_d = 1_i32;

    let sender_opts = SenderOptions::builder(&cfg.ndi_name)
        .clock_video(true)
        .build();
    let sender = Sender::new(&ndi, &sender_opts).context("Sender::new")?;

    let _ = emit_log(
        &app,
        format!("NDI 送出開始: {} ({})", cfg.ndi_name, cfg.url),
    );

    let engine = base64::engine::general_purpose::STANDARD;

    loop {
        tokio::select! {
            biased;
            _ = async {
                loop {
                    if *shutdown_rx.borrow() {
                        return;
                    }
                    if shutdown_rx.changed().await.is_err() {
                        return;
                    }
                }
            } => {
                break;
            }
            ev = sc_events.next() => {
                let Some(ev) = ev else { break };
                if let Err(e) = process_screencast_frame(
                    &page,
                    &sender,
                    &engine,
                    &ev,
                    cfg.width,
                    cfg.height,
                    fps_n,
                    fps_d,
                )
                .await
                {
                    let _ = emit_log(&app, format!("フレーム処理: {:#}", e));
                }
            }
        }
    }

    let _ = page.execute(StopScreencastParams::default()).await;
    let _ = page
        .execute(ClearDeviceMetricsOverrideParams::default())
        .await;

    let _ = emit_log(
        &app,
        format!("NDI 送出停止: {} ({})", cfg.ndi_name, cfg.url),
    );

    Ok(())
}

async fn process_screencast_frame(
    page: &Page,
    sender: &Sender,
    engine: &base64::engine::general_purpose::GeneralPurpose,
    ev: &EventScreencastFrame,
    out_w: u32,
    out_h: u32,
    fps_n: i32,
    fps_d: i32,
) -> anyhow::Result<()> {
    let jpeg = engine
        .decode(std::convert::AsRef::<[u8]>::as_ref(&ev.data))
        .context("base64 decode")?;

    let img = image::load_from_memory(&jpeg).context("jpeg decode")?;
    let img = img.resize_exact(out_w, out_h, FilterType::Triangle);
    let rgba = img.to_rgba8();
    let rgba_bytes = rgba.as_raw();

    let mut video = VideoFrame::builder()
        .resolution(out_w as i32, out_h as i32)
        .pixel_format(PixelFormat::BGRA)
        .frame_rate(fps_n, fps_d)
        .aspect_ratio(out_w as f32 / out_h.max(1) as f32)
        .scan_type(ScanType::Progressive)
        .build()
        .context("VideoFrame::build")?;

    bgra_fill_from_rgba(&mut video.data, rgba_bytes);

    sender.send_video(&video);

    page.execute(ScreencastFrameAckParams::new(ev.session_id))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("ScreencastFrameAck")?;

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
