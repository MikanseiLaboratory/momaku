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
  ndiGroups: string;
  ndiClockVideo: boolean;
  ndiClockAudio: boolean;
  width: number;
  height: number;
  fps: number;
};

type LogEntry = { id: string; text: string };

type EngineRunningState = {
  running: boolean;
  streamsRunning: boolean[];
};

type BusyState =
  | null
  | { kind: "save" }
  | { kind: "start"; index: number }
  | { kind: "stop"; index: number }
  | { kind: "startAll" }
  | { kind: "stopAll" }
  | { kind: "update" };

function defaultRow(): StreamRow {
  return {
    url: "https://example.com",
    ndiName: "momaku-1",
    ndiGroups: "",
    ndiClockVideo: true,
    ndiClockAudio: true,
    width: 1280,
    height: 720,
    fps: 30,
  };
}

function normalizeRow(r: Partial<StreamRow> & Pick<StreamRow, "url" | "ndiName" | "width" | "height" | "fps">): StreamRow {
  const d = defaultRow();
  return {
    ...d,
    ...r,
    ndiGroups: r.ndiGroups ?? "",
    ndiClockVideo: r.ndiClockVideo ?? true,
    ndiClockAudio: r.ndiClockAudio ?? true,
  };
}

function toInvokePayload(rows: StreamRow[]) {
  return rows.map((row) => ({
    url: row.url,
    ndiName: row.ndiName,
    width: row.width,
    height: row.height,
    fps: row.fps,
    ndiGroups: row.ndiGroups.trim() ? row.ndiGroups.trim() : null,
    ndiClockVideo: row.ndiClockVideo,
    ndiClockAudio: row.ndiClockAudio,
  }));
}

const StreamRowEditor = memo(function StreamRowEditor({
  index,
  row,
  rowRunning,
  busy,
  onPatch,
  onRemove,
  onStart,
  onStop,
}: {
  index: number;
  row: StreamRow;
  rowRunning: boolean;
  busy: BusyState;
  onPatch: (i: number, patch: Partial<StreamRow>) => void;
  onRemove: (i: number) => void;
  onStart: (i: number) => void;
  onStop: (i: number) => void;
}) {
  const rowBusy =
    busy?.kind === "start" && busy.index === index
      ? "start"
      : busy?.kind === "stop" && busy.index === index
        ? "stop"
        : null;

  return (
    <tr>
      <td className="cell-actions">
        <button type="button" className="btn btn-ghost" onClick={() => onRemove(index)} disabled={rowRunning}>
          削除
        </button>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => onStart(index)}
          disabled={rowRunning || busy !== null}
        >
          {rowBusy === "start" ? "開始中…" : "開始"}
        </button>
        <button
          type="button"
          className="btn btn-danger"
          onClick={() => onStop(index)}
          disabled={!rowRunning || busy !== null}
        >
          {rowBusy === "stop" ? "停止中…" : "停止"}
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
          aria-label={`ストリーム ${index + 1} のURL`}
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
          aria-label={`ストリーム ${index + 1} のNDI名`}
        />
      </td>
      <td>
        <input
          className="field"
          type="text"
          value={row.ndiGroups}
          onChange={(e) => onPatch(index, { ndiGroups: e.target.value })}
          spellCheck={false}
          autoComplete="off"
          aria-label={`ストリーム ${index + 1} のNDIグループ`}
        />
      </td>
      <td className="cell-check">
        <label className="check-label">
          <input
            type="checkbox"
            checked={row.ndiClockVideo}
            onChange={(e) => onPatch(index, { ndiClockVideo: e.target.checked })}
            aria-label={`ストリーム ${index + 1} の動画クロック`}
          />
          <span>動画</span>
        </label>
      </td>
      <td className="cell-check">
        <label className="check-label">
          <input
            type="checkbox"
            checked={row.ndiClockAudio}
            onChange={(e) => onPatch(index, { ndiClockAudio: e.target.checked })}
            aria-label={`ストリーム ${index + 1} の音声クロック`}
          />
          <span>音声</span>
        </label>
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
          aria-label={`ストリーム ${index + 1} のFPS`}
        />
      </td>
    </tr>
  );
});

export function App() {
  const [rows, setRows] = useState<StreamRow[] | null>(null);
  const [engine, setEngine] = useState<EngineRunningState>({
    running: false,
    streamsRunning: [],
  });
  const [logLines, setLogLines] = useState<LogEntry[]>([]);
  const [busy, setBusy] = useState<BusyState>(null);

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

  const applyEnginePayload = useCallback((p: EngineRunningState) => {
    setEngine({
      running: p.running,
      streamsRunning: [...p.streamsRunning],
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const [streamsResult, runningResult] = await Promise.allSettled([
        invoke<StreamRow[]>("get_streams"),
        invoke<EngineRunningState>("get_engine_running"),
      ]);
      if (cancelled) return;

      if (streamsResult.status === "fulfilled") {
        const list = streamsResult.value.map((r) => normalizeRow(r));
        setRows(list.length ? list : [defaultRow()]);
      } else {
        appendLog(`読込エラー: ${String(streamsResult.reason)}`);
        setRows([defaultRow()]);
      }

      if (runningResult.status === "fulfilled") {
        applyEnginePayload(runningResult.value);
      } else {
        applyEnginePayload({ running: false, streamsRunning: [] });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [appendLog, applyEnginePayload]);

  useEffect(() => {
    let active = true;
    const unlisteners: (() => void)[] = [];

    void (async () => {
      const [uLog, uStatus] = await Promise.all([
        listen<{ message: string }>("engine-log", (ev) => {
          appendLog(ev.payload.message);
        }),
        listen<EngineRunningState>("engine-status", (ev) => {
          applyEnginePayload(ev.payload);
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
  }, [appendLog, applyEnginePayload]);

  const handleSave = useCallback(async () => {
    if (!rows) return;
    if (engine.running) {
      appendLog("送出中は保存できません。先にすべて停止してください。");
      return;
    }
    setBusy({ kind: "save" });
    try {
      await invoke("save_streams", { streams: toInvokePayload(rows) });
      appendLog("設定を保存しました");
    } catch (e) {
      appendLog(`保存エラー: ${e}`);
    } finally {
      setBusy(null);
    }
  }, [rows, engine.running, appendLog]);

  const handleStartRow = useCallback(
    async (index: number) => {
      if (!rows) return;
      setBusy({ kind: "start", index });
      try {
        await invoke("save_streams", { streams: toInvokePayload(rows) });
        await invoke("start_stream", { index });
        appendLog(`ストリーム ${index + 1} の送出を開始しました`);
      } catch (e) {
        appendLog(`開始エラー (行 ${index + 1}): ${e}`);
      } finally {
        setBusy(null);
      }
    },
    [rows, appendLog],
  );

  const handleStopRow = useCallback(async (index: number) => {
    setBusy({ kind: "stop", index });
    try {
      await invoke("stop_stream", { index });
      appendLog(`ストリーム ${index + 1} を停止しました`);
    } catch (e) {
      appendLog(`停止エラー (行 ${index + 1}): ${e}`);
    } finally {
      setBusy(null);
    }
  }, [appendLog]);

  const handleStartAll = useCallback(async () => {
    if (!rows) return;
    setBusy({ kind: "startAll" });
    try {
      await invoke("save_streams", { streams: toInvokePayload(rows) });
      await invoke("start_outputs");
      appendLog("すべてのストリームの送出を開始しました（未送出の行のみ）");
    } catch (e) {
      appendLog(`一括開始エラー: ${e}`);
    } finally {
      setBusy(null);
    }
  }, [rows, appendLog]);

  const handleStopAll = useCallback(async () => {
    setBusy({ kind: "stopAll" });
    try {
      await invoke("stop_outputs");
      appendLog("すべてのストリームを停止しました");
    } catch (e) {
      appendLog(`一括停止エラー: ${e}`);
    } finally {
      setBusy(null);
    }
  }, [appendLog]);

  useEffect(() => {
    if (!engine.running) return;
    const sendKey = (kind: "keyDown" | "keyUp", ev: KeyboardEvent) => {
      const t = ev.target as HTMLElement | null;
      if (t?.closest?.("input, textarea, select, button")) return;
      if (ev.repeat) return;
      ev.preventDefault();
      const idx = engine.streamsRunning.findIndex(Boolean);
      if (idx < 0) return;
      void invoke("submit_remote_input", {
        input: {
          streamIndex: idx,
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
  }, [engine.running, engine.streamsRunning]);

  const handleCheckUpdate = useCallback(async () => {
    appendLog("アップデートを確認しています…");
    setBusy({ kind: "update" });
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
  const anyRunning = engine.running;
  const colCount = 9;

  const rowRunningAt = (i: number) => Boolean(engine.streamsRunning[i]);

  return (
    <div className="app-shell">
      <div className="app-inner">
        <header className="hero">
          <div className="hero-badge">NDI</div>
          <div className="hero-text">
            <h1 className="title">momaku</h1>
            <p className="subtitle">WebをキャプチャしてNDIで配信（ビデオのみ）</p>
          </div>
          <div
            className={`status-pill ${anyRunning ? "status-pill--live" : "status-pill--idle"}`}
            role="status"
            aria-live="polite"
          >
            <span className="status-dot" aria-hidden />
            {anyRunning ? "一部またはすべて送出中" : "すべて停止中"}
          </div>
        </header>

        <section className="panel toolbar" aria-label="操作">
          <div className="toolbar-cluster">
            <button type="button" className="btn" onClick={addRow} disabled={!ready || anyRunning}>
              行を追加
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => void handleSave()}
              disabled={!ready || busy !== null || anyRunning}
            >
              {busy?.kind === "save" ? "保存中…" : "設定を保存"}
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => void handleStartAll()}
              disabled={!ready || busy !== null}
            >
              {busy?.kind === "startAll" ? "一括開始中…" : "すべて開始"}
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() => void handleStopAll()}
              disabled={!anyRunning || busy !== null}
            >
              {busy?.kind === "stopAll" ? "一括停止中…" : "すべて停止"}
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => void handleCheckUpdate()}
              disabled={busy !== null}
            >
              {busy?.kind === "update" ? "更新確認中…" : "更新を確認"}
            </button>
          </div>
        </section>

        <section className="panel table-panel" aria-label="ストリーム一覧">
          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th className="th-actions" scope="col">
                    操作
                  </th>
                  <th scope="col">URL</th>
                  <th scope="col">NDI名</th>
                  <th scope="col">NDIグループ</th>
                  <th scope="col">CLK動画</th>
                  <th scope="col">CLK音声</th>
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
                      rowRunning={rowRunningAt(index)}
                      busy={busy}
                      onPatch={patchRow}
                      onRemove={removeRow}
                      onStart={handleStartRow}
                      onStop={handleStopRow}
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
