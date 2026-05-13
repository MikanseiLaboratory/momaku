#!/usr/bin/env bash
# Tauri開発サーバ（`tauri dev`）。リポジトリルートで実行してください。
set -euo pipefail
cd "$(dirname "$0")/.."
npm run tauri -- dev
