# momaku

網膜（Moumaku）— Web ページを NDI で送出するデスクトップアプリ（ビデオのみ）。

## 構成

- **GUI**: Tauri 2 + Vite + TypeScript（操作はすべてウィンドウから）
- **送出**: [grafton-ndi](https://crates.io/crates/grafton-ndi)（NDI 6 SDK）
- **ブラウザ**: [chromiumoxide](https://crates.io/crates/chromiumoxide)（ヘッドレス Chromium / CDP Screencast）

## 前提

1. **NDI 6 SDK**（ビルド用）と **NDI ランタイム**（実行用）をインストールする。  
   [NDI SDK](https://ndi.video/type/developer/)

2. **Google Chrome または Chromium**（Chromiumoxide が起動に利用）

3. **Windows でのビルド**: `grafton-ndi` が bindgen / libclang を使うため、**64 ビットの LLVM（libclang）** が PATH または `LIBCLANG_PATH` で参照できること。  
   Visual Studio の「C++ 用 Clang ツール」や LLVM 公式ビルドの **x64** を指してください。  
   32 ビットの libclang が先に見つかると、bindgen でポインタ幅の不一致（`left: 4 right: 8`）が発生することがあります。

4. リポジトリ直下の **[`.cargo/config.toml`](.cargo/config.toml)** で `BINDGEN_EXTRA_CLANG_ARGS = "-m64"` を設定しており、上記のポインタ幅不一致を避けるための既定値です（必要に応じて環境に合わせて変更してください）。

## 開発

```bash
npm install
npm run tauri dev
```

## ビルド

```bash
npm run tauri build
```

## 設定の保存場所

ストリーム一覧は OS のアプリ設定ディレクトリに `streams.json` として保存されます（`directories` クレート: `com.flowing.momaku`）。
