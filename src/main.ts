import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type StreamRow = {
  url: string;
  ndiName: string;
  width: number;
  height: number;
  fps: number;
  jpegQuality: number;
  screencastEveryNthFrame: number | null;
};

function defaultRow(): StreamRow {
  return {
    url: "https://example.com",
    ndiName: "momaku-1",
    width: 1280,
    height: 720,
    fps: 30,
    jpegQuality: 85,
    screencastEveryNthFrame: null,
  };
}

let rows: StreamRow[] = [];

const tbody = document.getElementById("streams-body")!;
const logEl = document.getElementById("log")!;
const statusEl = document.getElementById("status")!;
const btnStart = document.getElementById("btn-start") as HTMLButtonElement;
const btnStop = document.getElementById("btn-stop") as HTMLButtonElement;

function appendLog(line: string) {
  const t = new Date().toISOString();
  logEl.textContent = `[${t}] ${line}\n` + logEl.textContent;
}

function setRunning(running: boolean) {
  statusEl.textContent = running ? "送出中" : "停止中";
  statusEl.classList.toggle("running", running);
  btnStart.disabled = running;
  btnStop.disabled = !running;
}

function renderTable() {
  tbody.innerHTML = "";
  rows.forEach((row, i) => {
    const tr = document.createElement("tr");
    tr.innerHTML = `
      <td class="narrow"><button type="button" data-del="${i}">削除</button></td>
      <td><input type="text" data-f="url" data-i="${i}" value="${escapeAttr(row.url)}" /></td>
      <td><input type="text" data-f="ndiName" data-i="${i}" value="${escapeAttr(row.ndiName)}" /></td>
      <td><input type="number" data-f="width" data-i="${i}" value="${row.width}" min="64" /></td>
      <td><input type="number" data-f="height" data-i="${i}" value="${row.height}" min="64" /></td>
      <td><input type="number" data-f="fps" data-i="${i}" value="${row.fps}" min="1" max="120" /></td>
      <td><input type="number" data-f="jpegQuality" data-i="${i}" value="${row.jpegQuality}" min="1" max="100" /></td>
      <td><input type="number" data-f="screencastEveryNthFrame" data-i="${i}" value="${row.screencastEveryNthFrame ?? ""}" min="1" placeholder="空" /></td>
    `;
    tbody.appendChild(tr);
  });

  tbody.querySelectorAll("button[data-del]").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      const i = Number((e.currentTarget as HTMLButtonElement).dataset.del);
      rows.splice(i, 1);
      renderTable();
    });
  });

  tbody.querySelectorAll("input[data-f]").forEach((inp) => {
    inp.addEventListener("change", (e) => {
      const el = e.target as HTMLInputElement;
      const i = Number(el.dataset.i);
      const f = el.dataset.f as keyof StreamRow;
      const v = el.value;
      if (f === "screencastEveryNthFrame") {
        rows[i].screencastEveryNthFrame = v === "" ? null : Number(v);
      } else if (f === "url" || f === "ndiName") {
        rows[i][f] = v;
      } else {
        (rows[i] as unknown as Record<string, number>)[f] = Number(v);
      }
    });
  });
}

function escapeAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

function collectRowsFromDom(): StreamRow[] {
  const out: StreamRow[] = [];
  const trs = tbody.querySelectorAll("tr");
  trs.forEach((tr, i) => {
    const inputs = tr.querySelectorAll<HTMLInputElement>("input[data-f]");
    const r = { ...rows[i] };
    inputs.forEach((el) => {
      const f = el.dataset.f as keyof StreamRow;
      const v = el.value;
      if (f === "screencastEveryNthFrame") {
        r.screencastEveryNthFrame = v === "" ? null : Number(v);
      } else if (f === "url" || f === "ndiName") {
        (r as unknown as Record<string, string>)[f] = v;
      } else {
        (r as unknown as Record<string, number>)[f] = Number(v);
      }
    });
    out.push(r);
  });
  return out.length ? out : rows;
}

async function loadStreams() {
  try {
    rows = await invoke<StreamRow[]>("get_streams");
    if (!rows.length) rows = [defaultRow()];
  } catch (e) {
    appendLog(`読込エラー: ${e}`);
    rows = [defaultRow()];
  }
  renderTable();
}

document.getElementById("btn-add")!.addEventListener("click", () => {
  rows = collectRowsFromDom();
  rows.push(defaultRow());
  renderTable();
});

document.getElementById("btn-save")!.addEventListener("click", async () => {
  rows = collectRowsFromDom();
  try {
    await invoke("save_streams", { streams: rows });
    appendLog("設定を保存しました");
  } catch (e) {
    appendLog(`保存エラー: ${e}`);
  }
});

btnStart.addEventListener("click", async () => {
  rows = collectRowsFromDom();
  try {
    await invoke("save_streams", { streams: rows });
    await invoke("start_outputs");
    setRunning(true);
    appendLog("送出を開始しました");
  } catch (e) {
    appendLog(`開始エラー: ${e}`);
  }
});

btnStop.addEventListener("click", async () => {
  try {
    await invoke("stop_outputs");
    setRunning(false);
    appendLog("送出を停止しました");
  } catch (e) {
    appendLog(`停止エラー: ${e}`);
  }
});

void listen<{ message: string }>("engine-log", (ev) => {
  appendLog(ev.payload.message);
});

void listen<{ running: boolean }>("engine-status", (ev) => {
  setRunning(ev.payload.running);
});

void (async () => {
  await loadStreams();
  try {
    const running = await invoke<boolean>("get_engine_running");
    setRunning(running);
  } catch {
    setRunning(false);
  }
})();
