# momaku

Desktop app that sends web pages to **NDI** as video only, without audio. Cross-platform for **Windows**, **macOS**, and **Linux**.

Inspired by [Vingester](https://github.com/rse/vingester).

## Stack

Tauri 2, NDI, headless Chromium.

## Prerequisites

- [Rust](https://rustup.rs/)
- [Node.js](https://nodejs.org/)
- [NDI SDK](https://ndi.video/type/developer/)
- [Google Chrome](https://www.google.com/chrome/) or [Chromium](https://www.chromium.org/chromium-projects/)

On Windows, a 64-bit [LLVM](https://releases.llvm.org/) installation may be required to build NDI-related bindings.

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

## Settings

The stream list is saved as `streams.json` in the app config directory for `com.flowing.momaku`.
