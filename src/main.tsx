import { registerNativeChromeGuards } from "./nativeChromeGuards";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";

registerNativeChromeGuards();

const el = document.getElementById("root");
if (!el) {
  throw new Error("Missing #root");
}

createRoot(el).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
