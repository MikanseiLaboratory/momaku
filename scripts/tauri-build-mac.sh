#!/usr/bin/env bash
# Tauri本番ビルド（`tauri build`）。リポジトリルートで実行してください。
set -euo pipefail
cd "$(dirname "$0")/.."
npm run tauri -- build
