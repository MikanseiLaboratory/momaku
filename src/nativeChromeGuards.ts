/**
 * WebView / ブラウザの「Webページっぽい」挙動を抑止する。
 * React より先に main.tsx から登録する（useEffect より早く効かせる）。
 */

function elementFromTarget(target: EventTarget | null): Element | null {
  if (!target) return null;
  if (target instanceof Text) return target.parentElement;
  if (target instanceof Element) return target;
  return null;
}

/** テキスト入力・ログ以外では選択・右クリックを抑止する対象か */
function isDomTextEntryOrLogSurface(target: EventTarget | null): boolean {
  const el = elementFromTarget(target);
  if (!el) return false;
  if (el.closest(".log-viewport")) return true;
  if (el.closest("textarea, [contenteditable='true'], select, option")) return true;
  const input = el.closest("input");
  if (!(input instanceof HTMLInputElement)) return false;
  const type = (input.getAttribute("type") ?? "text").toLowerCase();
  const allowed = new Set([
    "text",
    "search",
    "url",
    "tel",
    "email",
    "password",
    "number",
    "date",
    "time",
    "datetime-local",
    "month",
    "week",
  ]);
  return allowed.has(type) || type === "";
}

const DOC_MARK = "__momakuNativeChromeGuards" as const;

export function registerNativeChromeGuards(): void {
  if (typeof document === "undefined") return;
  const d = document as Document & { [key: string]: boolean | undefined };
  if (d[DOC_MARK]) return;
  d[DOC_MARK] = true;

  const blockIfChromeOnly = (e: Event): void => {
    if (isDomTextEntryOrLogSurface(e.target)) return;
    e.preventDefault();
  };

  const blockRightMouseDown = (e: MouseEvent): void => {
    if (e.button !== 2) return;
    if (isDomTextEntryOrLogSurface(e.target)) return;
    e.preventDefault();
  };

  document.addEventListener("contextmenu", blockIfChromeOnly, true);
  document.addEventListener("selectstart", blockIfChromeOnly, true);
  document.addEventListener("dragstart", blockIfChromeOnly, true);
  document.addEventListener("mousedown", blockRightMouseDown, true);
  document.addEventListener("dblclick", blockIfChromeOnly, true);
}
