mod engine;

use std::path::PathBuf;
use std::sync::Arc;

use engine::{EngineLogPayload, EngineStatusPayload, StreamConfig};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{watch, Mutex};

pub struct RunningEngine {
    pub shutdown_tx: watch::Sender<bool>,
    pub join: tokio::task::JoinHandle<()>,
}

pub struct AppState {
    pub streams_path: PathBuf,
    pub engine: Arc<Mutex<Option<RunningEngine>>>,
}

async fn load_streams(path: &PathBuf) -> Result<Vec<StreamConfig>, String> {
    if !path.exists() {
        return Ok(vec![StreamConfig {
            url: "https://example.com".into(),
            ndi_name: "momaku-1".into(),
            width: 1280,
            height: 720,
            fps: 30,
            jpeg_quality: 85,
            screencast_every_nth_frame: None,
        }]);
    }
    let t = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("設定読込: {e}"))?;
    serde_json::from_str(&t).map_err(|e| format!("JSON: {e}"))
}

#[tauri::command]
async fn get_streams(state: State<'_, AppState>) -> Result<Vec<StreamConfig>, String> {
    load_streams(&state.streams_path).await
}

#[tauri::command]
async fn save_streams(state: State<'_, AppState>, streams: Vec<StreamConfig>) -> Result<(), String> {
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
async fn get_engine_running(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.engine.lock().await.is_some())
}

#[tauri::command]
async fn start_outputs(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let streams = load_streams(&state.streams_path).await?;
    if streams.is_empty() {
        return Err("ストリームが1件以上必要です".into());
    }
    for s in &streams {
        s.validate().map_err(|e| e.to_string())?;
    }

    let mut g = state.engine.lock().await;
    if g.is_some() {
        return Err("既に送出中です".into());
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let engine_slot = state.engine.clone();
    let app_task = app.clone();

    let task = tokio::spawn(async move {
        let res = engine::run_all(streams, app_task.clone(), shutdown_rx).await;
        if let Err(e) = res {
            let _ = app_task.emit(
                "engine-log",
                EngineLogPayload {
                    message: format!("エンジンエラー: {:#}", e),
                },
            );
        }
        let mut slot = engine_slot.lock().await;
        *slot = None;
        let _ = app_task.emit(
            "engine-status",
            EngineStatusPayload { running: false },
        );
    });

    *g = Some(RunningEngine {
        shutdown_tx,
        join: task,
    });
    drop(g);

    app.emit(
        "engine-status",
        EngineStatusPayload { running: true },
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn stop_outputs(state: State<'_, AppState>) -> Result<(), String> {
    let running = {
        let mut g = state.engine.lock().await;
        g.take()
    };
    let Some(r) = running else {
        return Ok(());
    };
    let _ = r.shutdown_tx.send(true);
    tokio::time::timeout(std::time::Duration::from_secs(45), r.join)
        .await
        .map_err(|_| "停止がタイムアウトしました".to_string())?
        .map_err(|e| format!("タスク終了: {e}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("warn,momaku_lib=info,chromiumoxide=warn")
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
                engine: Arc::new(Mutex::new(None)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_streams,
            save_streams,
            get_engine_running,
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
