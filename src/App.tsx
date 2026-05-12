import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import {
  memo,
  startTransition,
  useCallback,
  useEffect,
  useState,
} from "react";

export type StreamRow = {
  url: string;
  ndiName: string;
  width: number;
  height: number;
  fps: number;
};

type LogEntry = { id: string; text: string };

function defaultRow(): StreamRow {
  return {
    url: "https://example.com",
    ndiName: "momaku-1",
    width: 1280,
    height: 720,
    fps: 30,
  };
}

const StreamRowEditor = memo(function StreamRowEditor({
  index,
  row,
  onPatch,
  onRemove,
}: {
  index: number;
  row: StreamRow;
  onPatch: (i: number, patch: Partial<StreamRow>) => void;
  onRemove: (i: number) => void;
}) {
  return (
    <tr>
      <td className="cell-actions">
        <button type="button" className="btn btn-ghost" onClick={() => onRemove(index)}>
          削除
        </button>
      </td>
      <td>
        <input
          className="field"
          type="text"
          value={row.url}
          onChange={(e) => onPatch(index, { url: e.target.value })}
          spellCheck={false}
          autoComplete="off"
          aria-label={`ストリーム ${index + 1} の URL`}
        />
      </td>
      <td>
        <input
          className="field"
          type="text"
          value={row.ndiName}
          onChange={(e) => onPatch(index, { ndiName: e.target.value })}
          spellCheck={false}
          autoComplete="off"
          aria-label={`ストリーム ${index + 1} の NDI 名`}
        />
      </td>
      <td>
        <input
          className="field field-num"
          type="number"
          value={row.width}
          min={64}
          onChange={(e) => onPatch(index, { width: Number(e.target.value) })}
          aria-label={`ストリーム ${index + 1} の幅`}
        />
      </td>
      <td>
        <input
          className="field field-num"
          type="number"
          value={row.height}
          min={64}
          onChange={(e) => onPatch(index, { height: Number(e.target.value) })}
          aria-label={`ストリーム ${index + 1} の高さ`}
        />
      </td>
      <td>
        <input
          className="field field-num"
          type="number"
          value={row.fps}
          min={1}
          max={120}
          onChange={(e) => onPatch(index, { fps: Number(e.target.value) })}
          aria-label={`ストリーム ${index + 1} の FPS`}
        />
      </td>
    </tr>
  );
});

export function App() {
  const [rows, setRows] = useState<StreamRow[] | null>(null);
  const [running, setRunning] = useState(false);
  const [logLines, setLogLines] = useState<LogEntry[]>([]);
  const [busy, setBusy] = useState<"save" | "start" | "stop" | "update" | null>(null);

  const appendLog = useCallback((line: string) => {
    const id = crypto.randomUUID();
    const text = `[${new Date().toISOString()}] ${line}`;
    startTransition(() => {
      setLogLines((prev) => [{ id, text }, ...prev].slice(0, 300));
    });
  }, []);

  const patchRow = useCallback((index: number, patch: Partial<StreamRow>) => {
    setRows((prev) => {
      if (!prev) return prev;
      return prev.map((r, i) => (i === index ? { ...r, ...patch } : r));
    });
  }, []);

  const addRow = useCallback(() => {
    setRows((prev) => (prev ? [...prev, defaultRow()] : prev));
  }, []);

  const removeRow = useCallback((index: number) => {
    setRows((prev) => (prev ? prev.filter((_, i) => i !== index) : prev));
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const [streamsResult, runningResult] = await Promise.allSettled([
        invoke<StreamRow[]>("get_streams"),
        invoke<boolean>("get_engine_running"),
      ]);
      if (cancelled) return;

      if (streamsResult.status === "fulfilled") {
        const list = streamsResult.value;
        setRows(list.length ? list : [defaultRow()]);
      } else {
        appendLog(`読込エラー: ${String(streamsResult.reason)}`);
        setRows([defaultRow()]);
      }

      if (runningResult.status === "fulfilled") {
        setRunning(runningResult.value);
      } else {
        setRunning(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [appendLog]);

  useEffect(() => {
    let active = true;
    const unlisteners: (() => void)[] = [];

    void (async () => {
      const [uLog, uStatus] = await Promise.all([
        listen<{ message: string }>("engine-log", (ev) => {
          appendLog(ev.payload.message);
        }),
        listen<{ running: boolean }>("engine-status", (ev) => {
          setRunning(ev.payload.running);
        }),
      ]);
      if (!active) {
        uLog();
        uStatus();
        return;
      }
      unlisteners.push(uLog, uStatus);
    })();

    return () => {
      active = false;
      unlisteners.splice(0).forEach((f) => f());
    };
  }, [appendLog]);

  const handleSave = useCallback(async () => {
    if (!rows) return;
    setBusy("save");
    try {
      await invoke("save_streams", { streams: rows });
      appendLog("設定を保存しました");
    } catch (e) {
      appendLog(`保存エラー: ${e}`);
    } finally {
      setBusy(null);
    }
  }, [rows, appendLog]);

  const handleStart = useCallback(async () => {
    if (!rows) return;
    setBusy("start");
    try {
      await invoke("save_streams", { streams: rows });
      await invoke("start_outputs");
      setRunning(true);
      appendLog("送出を開始しました");
    } catch (e) {
      appendLog(`開始エラー: ${e}`);
    } finally {
      setBusy(null);
    }
  }, [rows, appendLog]);

  const handleStop = useCallback(async () => {
    setBusy("stop");
    try {
      await invoke("stop_outputs");
      setRunning(false);
      appendLog("送出を停止しました");
    } catch (e) {
      appendLog(`停止エラー: ${e}`);
    } finally {
      setBusy(null);
    }
  }, [appendLog]);

  useEffect(() => {
    if (!running) return;
    const sendKey = (kind: "keyDown" | "keyUp", ev: KeyboardEvent) => {
      const t = ev.target as HTMLElement | null;
      if (t?.closest?.("input, textarea, select, button")) return;
      if (ev.repeat) return;
      ev.preventDefault();
      void invoke("submit_remote_input", {
        input: {
          streamIndex: 0,
          event: { kind, key: ev.key, keysym: null as number | null },
        },
      }).catch(() => {});
    };
    const down = (e: KeyboardEvent) => sendKey("keyDown", e);
    const up = (e: KeyboardEvent) => sendKey("keyUp", e);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, [running]);

  const handleCheckUpdate = useCallback(async () => {
    appendLog("アップデートを確認しています…");
    setBusy("update");
    try {
      const update = await check({ timeout: 60_000 });
      if (!update) {
        appendLog("利用可能な更新はありません。");
        return;
      }
      appendLog(`更新 ${update.version} をダウンロードしています…`);
      await update.downloadAndInstall();
      appendLog("インストール完了。再起動します。");
      await relaunch();
    } catch (e) {
      appendLog(`更新エラー: ${e}`);
    } finally {
      setBusy(null);
    }
  }, [appendLog]);

  const ready = rows !== null;
  const colCount = 6;

  return (
    <div className="app-shell">
      <div className="app-inner">
        <header className="hero">
          <div className="hero-badge">NDI</div>
          <div className="hero-text">
            <h1 className="title">momaku</h1>
            <p className="subtitle">Web をキャプチャして NDI で配信（ビデオのみ）</p>
          </div>
          <div
            className={`status-pill ${running ? "status-pill--live" : "status-pill--idle"}`}
            role="status"
            aria-live="polite"
          >
            <span className="status-dot" aria-hidden />
            {running ? "送出中" : "停止中"}
          </div>
        </header>

        <section className="panel toolbar" aria-label="操作">
          <div className="toolbar-cluster">
            <button type="button" className="btn" onClick={addRow} disabled={!ready}>
              行を追加
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => void handleSave()}
              disabled={!ready || busy !== null}
            >
              {busy === "save" ? "保存中…" : "設定を保存"}
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void handleStart()}
              disabled={!ready || running || busy !== null}
            >
              {busy === "start" ? "開始中…" : "開始"}
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() => void handleStop()}
              disabled={!running || busy !== null}
            >
              {busy === "stop" ? "停止中…" : "停止"}
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => void handleCheckUpdate()}
              disabled={busy !== null}
            >
              {busy === "update" ? "更新確認中…" : "更新を確認"}
            </button>
          </div>
        </section>

        <section className="panel table-panel" aria-label="ストリーム一覧">
          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th className="th-actions" scope="col" />
                  <th scope="col">URL</th>
                  <th scope="col">NDI 名</th>
                  <th scope="col">幅</th>
                  <th scope="col">高さ</th>
                  <th scope="col">FPS</th>
                </tr>
              </thead>
              <tbody>
                {!ready ? (
                  <tr>
                    <td colSpan={colCount} className="loading-row">
                      <span className="loading-shimmer">読み込み中…</span>
                    </td>
                  </tr>
                ) : (
                  rows.map((row, index) => (
                    <StreamRowEditor
                      key={index}
                      index={index}
                      row={row}
                      onPatch={patchRow}
                      onRemove={removeRow}
                    />
                  ))
                )}
              </tbody>
            </table>
          </div>
        </section>

        <section className="panel log-panel" aria-label="ログ">
          <div className="log-header">
            <h2 className="log-title">ログ</h2>
            <span className="log-hint">最新が上に表示されます</span>
          </div>
          <div className="log-viewport" role="log" aria-live="polite" aria-relevant="additions">
            {logLines.length === 0 ? (
              <p className="log-empty">ここにエンジンと操作のログが表示されます。</p>
            ) : (
              logLines.map((entry) => (
                <div key={entry.id} className="log-line">
                  {entry.text}
                </div>
              ))
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
