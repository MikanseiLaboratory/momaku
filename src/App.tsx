import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import {
  memo,
  startTransition,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import {
  APP_VERSION,
  type AppSettings,
  DEFAULT_APP_SETTINGS,
  DONATION_URL,
  LP_URL,
  REPO_URL,
} from "./appSettings";
import { IconGithub, IconGlobe, IconPlay, IconSpinner, IconStopSquare, IconTrash, IconTwitch } from "./externalIcons";

export type VideoSendMode = "fixedFps" | "onDemand";

export type StreamRow = {
  url: string;
  ndiName: string;
  ndiGroups: string;
  ndiClockVideo: boolean;
  ndiClockAudio: boolean;
  width: number;
  height: number;
  fps: number;
  videoSendMode: VideoSendMode;
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

function defaultRow(defaultNdiGroups = ""): StreamRow {
  return {
    url: "https://example.com",
    ndiName: "momaku-1",
    ndiGroups: defaultNdiGroups,
    ndiClockVideo: true,
    ndiClockAudio: true,
    width: 1280,
    height: 720,
    fps: 30,
    videoSendMode: "fixedFps",
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
    videoSendMode: r.videoSendMode === "onDemand" ? "onDemand" : "fixedFps",
  };
}

function resolveThemeAttr(theme: "light" | "dark" | null): "light" | "dark" {
  if (theme) return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function isTypingSurface(ev: KeyboardEvent): boolean {
  const n = ev.composedPath()[0];
  if (!(n instanceof Element)) return false;
  if (n instanceof HTMLInputElement || n instanceof HTMLTextAreaElement || n instanceof HTMLSelectElement) {
    return true;
  }
  if (n instanceof HTMLElement && n.isContentEditable) return true;
  return false;
}

function buttonLikeAllowsKey(ev: KeyboardEvent): boolean {
  const n = ev.composedPath()[0];
  if (n instanceof HTMLButtonElement) return ev.key === " " || ev.key === "Enter";
  if (n instanceof HTMLElement && n.getAttribute("role") === "button") {
    return ev.key === " " || ev.key === "Enter";
  }
  return false;
}

function isEventFromOpenDialog(ev: KeyboardEvent): boolean {
  return ev.composedPath().some((n) => n instanceof HTMLDialogElement && n.open);
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
    videoSendMode: row.videoSendMode,
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
        <button
          type="button"
          className="btn btn-ghost btn-icon"
          onClick={() => onRemove(index)}
          disabled={rowRunning}
          title="削除"
          aria-label="削除"
        >
          <IconTrash />
        </button>
        <button
          type="button"
          className="btn btn-primary btn-icon"
          onClick={() => onStart(index)}
          disabled={rowRunning || busy !== null}
          title={rowBusy === "start" ? "開始中…" : "開始"}
          aria-label={rowBusy === "start" ? "開始中…" : "開始"}
        >
          {rowBusy === "start" ? <IconSpinner /> : <IconPlay />}
        </button>
        <button
          type="button"
          className="btn btn-danger btn-icon"
          onClick={() => onStop(index)}
          disabled={!rowRunning || busy !== null}
          title={rowBusy === "stop" ? "停止中…" : "停止"}
          aria-label={rowBusy === "stop" ? "停止中…" : "停止"}
        >
          {rowBusy === "stop" ? <IconSpinner /> : <IconStopSquare />}
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
      <td>
        <select
          className="field"
          value={row.videoSendMode}
          onChange={(e) =>
            onPatch(index, {
              videoSendMode: e.target.value as VideoSendMode,
            })
          }
          aria-label={`ストリーム ${index + 1} の映像送出モード`}
        >
          <option value="fixedFps">常にFPSで送出</option>
          <option value="onDemand">更新時のみ送出</option>
        </select>
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
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<AppSettings>(DEFAULT_APP_SETTINGS);
  const [donationNeverAgain, setDonationNeverAgain] = useState(false);
  const donationRef = useRef<HTMLDialogElement>(null);

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
    const g = appSettings?.defaultNdiGroups ?? "";
    setRows((prev) => (prev ? [...prev, defaultRow(g)] : prev));
  }, [appSettings?.defaultNdiGroups]);

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
      const [stRes, streamRes, runningResult] = await Promise.allSettled([
        invoke<AppSettings>("get_app_settings"),
        invoke<StreamRow[]>("get_streams"),
        invoke<EngineRunningState>("get_engine_running"),
      ]);
      if (cancelled) return;

      const st = stRes.status === "fulfilled" ? stRes.value : DEFAULT_APP_SETTINGS;
      setAppSettings(st);

      if (streamRes.status === "fulfilled") {
        const list = streamRes.value.map((r) => normalizeRow(r));
        setRows(list.length ? list : [defaultRow(st.defaultNdiGroups)]);
      } else {
        appendLog(`読込エラー:${String(streamRes.reason)}`);
        setRows([defaultRow(st.defaultNdiGroups)]);
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
      appendLog(`保存エラー:${e}`);
    } finally {
      setBusy(null);
    }
  }, [rows, engine.running, appendLog]);

  useEffect(() => {
    if (!appSettings) return;
    const mode = appSettings.themeMode;
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const syncFromOs = () => {
      document.documentElement.dataset.theme = resolveThemeAttr(null);
    };

    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        const w = getCurrentWindow();
        mql.removeEventListener("change", syncFromOs);
        if (mode === "light") {
          document.documentElement.dataset.theme = "light";
          await w.setTheme("light");
          return;
        }
        if (mode === "dark") {
          document.documentElement.dataset.theme = "dark";
          await w.setTheme("dark");
          return;
        }
        await w.setTheme(null);
        const t = await w.theme();
        document.documentElement.dataset.theme = resolveThemeAttr(t);
        if (t === null) mql.addEventListener("change", syncFromOs);
        unlisten = await w.onThemeChanged(({ payload }) => {
          document.documentElement.dataset.theme = payload;
        });
      } catch {
        if (mode === "light") document.documentElement.dataset.theme = "light";
        else if (mode === "dark") document.documentElement.dataset.theme = "dark";
        else {
          syncFromOs();
          mql.addEventListener("change", syncFromOs);
        }
      }
    })();

    return () => {
      unlisten?.();
      mql.removeEventListener("change", syncFromOs);
    };
  }, [appSettings?.themeMode]);

  useEffect(() => {
    if (!appSettings || appSettings.hideDonationPrompt) return;
    const id = window.setTimeout(() => {
      try {
        donationRef.current?.showModal();
      } catch {
        /* 既に開いている */
      }
    }, 0);
    return () => clearTimeout(id);
  }, [appSettings, appSettings?.hideDonationPrompt]);

  useEffect(() => {
    const onKeyDownCapture = (ev: KeyboardEvent) => {
      if (isEventFromOpenDialog(ev)) return;

      if (engine.running) return;

      if (ev.key === "Escape") return;
      if (ev.ctrlKey || ev.metaKey || ev.altKey) return;
      if (ev.key === "Tab") return;
      if (/^F\d{1,2}$/i.test(ev.key)) return;

      if (isTypingSurface(ev) || buttonLikeAllowsKey(ev)) return;

      ev.preventDefault();
    };

    window.addEventListener("keydown", onKeyDownCapture, true);
    return () => window.removeEventListener("keydown", onKeyDownCapture, true);
  }, [engine.running]);

  const handleStartRow = useCallback(
    async (index: number) => {
      if (!rows) return;
      setBusy({ kind: "start", index });
      try {
        await invoke("save_streams", { streams: toInvokePayload(rows) });
        await invoke("start_stream", { index });
        appendLog(`ストリーム${index + 1}の送出を開始しました`);
      } catch (e) {
        appendLog(`開始エラー(行${index + 1}):${e}`);
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
      appendLog(`ストリーム${index + 1}を停止しました`);
    } catch (e) {
      appendLog(`停止エラー(行${index + 1}):${e}`);
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
      appendLog(`一括開始エラー:${e}`);
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
      appendLog(`一括停止エラー:${e}`);
    } finally {
      setBusy(null);
    }
  }, [appendLog]);

  useEffect(() => {
    if (!engine.running) return;
    const sendKey = (kind: "keyDown" | "keyUp", ev: KeyboardEvent) => {
      const t = ev.target as HTMLElement | null;
      if (t?.closest?.("dialog[open]")) return;
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
      appendLog(`更新${update.version}をダウンロードしています…`);
      await update.downloadAndInstall();
      appendLog("インストール完了。再起動します。");
      await relaunch();
    } catch (e) {
      appendLog(`更新エラー:${e}`);
    } finally {
      setBusy(null);
    }
  }, [appendLog]);

  const openExternalUrl = useCallback((url: string) => {
    void invoke("open_external_url", { url }).catch((e) => appendLog(`リンクを開けません:${e}`));
  }, [appendLog]);

  const persistAppSettings = useCallback(
    async (next: AppSettings) => {
      try {
        await invoke("save_app_settings", { settings: next });
        setAppSettings(next);
        return true;
      } catch (e) {
        appendLog(`アプリ設定の保存エラー:${e}`);
        return false;
      }
    },
    [appendLog],
  );

  const openSettingsModal = useCallback(() => {
    if (appSettings) setSettingsDraft({ ...appSettings });
    else setSettingsDraft({ ...DEFAULT_APP_SETTINGS });
    setSettingsOpen(true);
  }, [appSettings]);

  const handleSaveAppSettingsFromModal = useCallback(async () => {
    const ok = await persistAppSettings(settingsDraft);
    if (ok) {
      appendLog("アプリ設定を保存しました");
      setSettingsOpen(false);
    }
  }, [persistAppSettings, settingsDraft, appendLog]);

  const handleDonationDialogClose = useCallback(() => {
    if (donationNeverAgain && appSettings) {
      void persistAppSettings({ ...appSettings, hideDonationPrompt: true });
    }
    setDonationNeverAgain(false);
  }, [donationNeverAgain, appSettings, persistAppSettings]);

  const settingsDlgRef = useRef<HTMLDialogElement>(null);
  const aboutDlgRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const el = settingsDlgRef.current;
    if (!el) return;
    if (settingsOpen) {
      if (!el.open) el.showModal();
    } else if (el.open) el.close();
  }, [settingsOpen]);

  useEffect(() => {
    const el = aboutDlgRef.current;
    if (!el) return;
    if (aboutOpen) {
      if (!el.open) el.showModal();
    } else if (el.open) el.close();
  }, [aboutOpen]);

  const ready = rows !== null;
  const anyRunning = engine.running;
  const colCount = 10;

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
            <button type="button" className="btn btn-ghost" onClick={openSettingsModal}>
              アプリ設定
            </button>
            <button type="button" className="btn btn-ghost" onClick={() => setAboutOpen(true)}>
              バージョン情報
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
                  <th scope="col">
                    NDIグループ
                    <span className="th-hint">空欄の行はアプリ設定の既定グループを使用して送出します</span>
                  </th>
                  <th scope="col">CLK動画</th>
                  <th scope="col">CLK音声</th>
                  <th scope="col">幅</th>
                  <th scope="col">高さ</th>
                  <th scope="col">FPS</th>
                  <th scope="col">送出モード</th>
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

        <dialog
          ref={donationRef}
          className="modal-sheet"
          onClose={handleDonationDialogClose}
          aria-labelledby="donation-title"
        >
          <h2 className="modal-head" id="donation-title">
            ご支援のお願い
          </h2>
          <div className="modal-body">
            <p>
              momakuの開発・配信の継続のため、可能であればTwitchのサブスクリプションでのご支援をご検討ください。
            </p>
            <div className="modal-external-stack">
              <button
                type="button"
                className="btn btn-external btn-external--twitch"
                onClick={() => openExternalUrl(DONATION_URL)}
              >
                <IconTwitch />
                Twitchでサブスク登録ページを開く
              </button>
            </div>
          </div>
          <div className="modal-actions modal-actions--spread">
            <label className="modal-check">
              <input
                type="checkbox"
                checked={donationNeverAgain}
                onChange={(e) => setDonationNeverAgain(e.target.checked)}
              />
              二度と表示しない
            </label>
            <button type="button" className="btn btn-primary" onClick={() => donationRef.current?.close()}>
              閉じる
            </button>
          </div>
        </dialog>

        <dialog
          ref={settingsDlgRef}
          className="modal-sheet modal-sheet--wide"
          onClose={() => setSettingsOpen(false)}
          aria-labelledby="settings-title"
        >
          <h2 className="modal-head" id="settings-title">
            アプリ設定
          </h2>
          <div className="modal-body">
            <label className="modal-field-label" htmlFor="app-default-ndi">
              既定のNDI送出グループ
            </label>
            <input
              id="app-default-ndi"
              className="field"
              type="text"
              value={settingsDraft.defaultNdiGroups}
              onChange={(e) => setSettingsDraft((d) => ({ ...d, defaultNdiGroups: e.target.value }))}
              spellCheck={false}
              autoComplete="off"
              placeholder="例:MyGroup（各行が空のときに使用）"
              aria-describedby="app-default-ndi-hint"
            />
            <p id="app-default-ndi-hint" className="log-hint modal-field-hint">
              各行の「NDIグループ」が空のとき、この値が送出に使われます（最大256文字）。
            </p>
            <label className="modal-field-label modal-stack" htmlFor="app-theme">
              テーマ
            </label>
            <select
              id="app-theme"
              className="field"
              value={settingsDraft.themeMode}
              onChange={(e) =>
                setSettingsDraft((d) => ({
                  ...d,
                  themeMode: e.target.value as AppSettings["themeMode"],
                }))
              }
            >
              <option value="system">システムに合わせる</option>
              <option value="light">ライト</option>
              <option value="dark">ダーク</option>
            </select>
            <label className="modal-check modal-stack">
              <input
                type="checkbox"
                checked={!settingsDraft.hideDonationPrompt}
                onChange={(e) =>
                  setSettingsDraft((d) => ({ ...d, hideDonationPrompt: !e.target.checked }))
                }
              />
              起動時に寄付の案内を表示する
            </label>
          </div>
          <div className="modal-actions">
            <button type="button" className="btn btn-ghost" onClick={() => setSettingsOpen(false)}>
              キャンセル
            </button>
            <button type="button" className="btn btn-primary" onClick={() => void handleSaveAppSettingsFromModal()}>
              保存
            </button>
          </div>
        </dialog>

        <dialog
          ref={aboutDlgRef}
          className="modal-sheet modal-sheet--wide"
          onClose={() => setAboutOpen(false)}
          aria-labelledby="about-title"
        >
          <h2 className="modal-head" id="about-title">
            momakuについて
          </h2>
          <div className="modal-body">
            <p>
              バージョン<strong>{APP_VERSION}</strong>
            </p>
            <p>ソースコード・Issue・リリースはGitHubで公開しています。</p>
            <div className="modal-external-stack">
              <button type="button" className="btn btn-external" onClick={() => openExternalUrl(REPO_URL)}>
                <IconGithub />
                GitHubで開く
              </button>
            </div>
            <p>未完成成果物研究所のプロジェクト一覧・紹介ページです。</p>
            <div className="modal-external-stack">
              <button type="button" className="btn btn-external" onClick={() => openExternalUrl(LP_URL)}>
                <IconGlobe />
                公式サイトを開く
              </button>
            </div>
            <p>開発の継続のため、Twitchのサブスクリプションでのご支援も受け付けています。</p>
            <div className="modal-external-stack">
              <button
                type="button"
                className="btn btn-external btn-external--twitch"
                onClick={() => openExternalUrl(DONATION_URL)}
              >
                <IconTwitch />
                Twitchでサブスク登録ページを開く
              </button>
            </div>
          </div>
          <div className="modal-actions">
            <button type="button" className="btn btn-primary" onClick={() => setAboutOpen(false)}>
              閉じる
            </button>
          </div>
        </dialog>
      </div>
    </div>
  );
}
