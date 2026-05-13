mod engine;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use engine::{EngineLogPayload, EngineStatusPayload, InputQueue, StreamConfig};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{watch, Mutex};

/// 送出中ストリームindex→入力キュー（`submit_remote_input`が参照）
static ENGINE_INPUTS: StdMutex<HashMap<usize, InputQueue>> = StdMutex::new(HashMap::new());

pub fn register_stream_input(index: usize, q: InputQueue) {
    ENGINE_INPUTS
        .lock()
        .expect("ENGINE_INPUTS lock poisoned")
        .insert(index, q);
}

pub fn unregister_stream_input(index: usize) {
    ENGINE_INPUTS
        .lock()
        .expect("ENGINE_INPUTS lock poisoned")
        .remove(&index);
}

pub struct RunningEngine {
    pub shutdown_tx: watch::Sender<bool>,
    pub join: tokio::task::JoinHandle<()>,
}

pub struct AppState {
    pub streams_path: PathBuf,
    pub engines: Arc<Mutex<HashMap<usize, RunningEngine>>>,
}

async fn load_streams(path: &PathBuf) -> Result<Vec<StreamConfig>, String> {
    if !path.exists() {
        return Ok(vec![StreamConfig {
            url: "https://example.com".into(),
            ndi_name: "momaku-1".into(),
            width: 1280,
            height: 720,
            fps: 30,
            ndi_groups: None,
            ndi_clock_video: true,
            ndi_clock_audio: true,
        }]);
    }
    let t = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("設定読込: {e}"))?;
    serde_json::from_str(&t).map_err(|e| format!("JSON: {e}"))
}

async fn compute_engine_status(
    engines: &Arc<Mutex<HashMap<usize, RunningEngine>>>,
    streams_path: &PathBuf,
) -> EngineStatusPayload {
    let n = load_streams(streams_path).await.map(|s| s.len()).unwrap_or(0);
    let streams_running = {
        let map = engines.lock().await;
        (0..n).map(|i| map.contains_key(&i)).collect::<Vec<bool>>()
    };
    let running = streams_running.iter().any(|&x| x);
    EngineStatusPayload {
        running,
        streams_running,
    }
}

#[tauri::command]
async fn get_streams(state: State<'_, AppState>) -> Result<Vec<StreamConfig>, String> {
    load_streams(&state.streams_path).await
}

#[tauri::command]
async fn save_streams(
    state: State<'_, AppState>,
    streams: Vec<StreamConfig>,
) -> Result<(), String> {
    if !state.engines.lock().await.is_empty() {
        return Err("送出中は設定を保存できません。先にすべてのストリームを停止してください。".into());
    }
    for s in &streams {
        s.validate().map_err(|e| e.to_string())?;
    }
    let t = serde_json::to_string_pretty(&streams).map_err(|e| e.to_string())?;
    if let Some(parent) = state.streams_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&state.streams_path, t)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_engine_running(state: State<'_, AppState>) -> Result<EngineStatusPayload, String> {
    Ok(compute_engine_status(&state.engines, &state.streams_path).await)
}

/// 付属ビューア等からのリモート操作（当該ストリームが送出中のときのみ有効）。
#[tauri::command]
fn submit_remote_input(input: engine::RemoteInput) -> Result<(), String> {
    let g = ENGINE_INPUTS
        .lock()
        .map_err(|_| "入力キューがロックできません".to_string())?;
    let Some(q) = g.get(&input.stream_index) else {
        return Err("該当ストリームは送出中ではないため入力を受け付けません".into());
    };
    let mut inner = q
        .lock()
        .map_err(|_| "入力キュー内部のロックに失敗しました".to_string())?;
    inner.push_back(input);
    Ok(())
}

async fn start_stream_inner(
    app: AppHandle,
    engines: Arc<Mutex<HashMap<usize, RunningEngine>>>,
    streams_path: PathBuf,
    index: usize,
) -> Result<(), String> {
    let streams = load_streams(&streams_path).await?;
    let Some(cfg) = streams.get(index) else {
        return Err(format!("ストリーム {index} が存在しません"));
    };
    cfg.validate().map_err(|e| e.to_string())?;

    {
        let map = engines.lock().await;
        if map.contains_key(&index) {
            return Err("この行は既に送出中です".into());
        }
    }

    let engines_slot = engines.clone();
    let streams_path_bg = streams_path.clone();
    let app_task = app.clone();
    let cfg = cfg.clone();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let join = tokio::spawn(async move {
        let res = engine::run_single_stream(index, cfg, app_task.clone(), shutdown_rx).await;
        if let Err(e) = res {
            let _ = app_task.emit(
                "engine-log",
                EngineLogPayload {
                    message: format!("ストリーム {index} エラー: {:#}", e),
                },
            );
        }
        engines_slot.lock().await.remove(&index);
        let st = compute_engine_status(&engines_slot, &streams_path_bg).await;
        let _ = app_task.emit("engine-status", st);
    });

    {
        let mut map = engines.lock().await;
        map.insert(
            index,
            RunningEngine {
                shutdown_tx,
                join,
            },
        );
    }

    let st = compute_engine_status(&engines, &streams_path).await;
    app.emit("engine-status", st)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn start_stream(app: AppHandle, state: State<'_, AppState>, index: usize) -> Result<(), String> {
    start_stream_inner(
        app,
        state.engines.clone(),
        state.streams_path.clone(),
        index,
    )
    .await
}

async fn stop_stream_inner(
    app: AppHandle,
    engines: Arc<Mutex<HashMap<usize, RunningEngine>>>,
    streams_path: PathBuf,
    index: usize,
) -> Result<(), String> {
    let running = {
        let mut map = engines.lock().await;
        map.remove(&index)
    };
    let Some(r) = running else {
        return Err("この行は送出中ではありません".into());
    };
    let _ = r.shutdown_tx.send(true);
    tokio::time::timeout(std::time::Duration::from_secs(45), r.join)
        .await
        .map_err(|_| "停止がタイムアウトしました".to_string())?
        .map_err(|e| format!("タスク終了: {e}"))?;

    let st = compute_engine_status(&engines, &streams_path).await;
    app.emit("engine-status", st)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn stop_stream(app: AppHandle, state: State<'_, AppState>, index: usize) -> Result<(), String> {
    stop_stream_inner(
        app,
        state.engines.clone(),
        state.streams_path.clone(),
        index,
    )
    .await
}

#[tauri::command]
async fn start_outputs(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let streams = load_streams(&state.streams_path).await?;
    if streams.is_empty() {
        return Err("ストリームが1件以上必要です".into());
    }
    let n = streams.len();
    let engines = state.engines.clone();
    let path = state.streams_path.clone();
    for i in 0..n {
        if engines.lock().await.contains_key(&i) {
            continue;
        }
        start_stream_inner(app.clone(), engines.clone(), path.clone(), i).await?;
    }
    Ok(())
}

#[tauri::command]
async fn stop_outputs(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let keys: Vec<usize> = state.engines.lock().await.keys().copied().collect();
    let engines = state.engines.clone();
    let path = state.streams_path.clone();
    for k in keys {
        let _ = stop_stream_inner(app.clone(), engines.clone(), path.clone(), k).await;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // rustls 0.23: `ring`と`aws_lc_rs`が併用されると既定CryptoProviderが決まらない。
    // 他クレートより先にaws-lc-rsを選ぶ（既に設定済みなら二重呼び出しは無視する）。
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("warn,momaku_lib=info,servo=warn")
            }),
        )
        .try_init();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
        .setup(|app| {
            let dir = directories::ProjectDirs::from("com", "flowing", "momaku")
                .expect("ProjectDirs::from (ホームディレクトリが利用できません)")
                .config_dir()
                .to_path_buf();
            std::fs::create_dir_all(&dir).expect("create_dir_all config");
            let streams_path = dir.join("streams.json");
            app.manage(AppState {
                streams_path,
                engines: Arc::new(Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_streams,
            save_streams,
            get_engine_running,
            submit_remote_input,
            start_stream,
            stop_stream,
            start_outputs,
            stop_outputs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_semver() {
        let v = env!("CARGO_PKG_VERSION");
        assert!(!v.is_empty());
        assert!(v.chars().next().unwrap().is_ascii_digit());
    }
}
