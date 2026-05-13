# momaku

Desktop app that sends web pages to **NDI** as video only, without audio. Cross-platform for **Windows**, **macOS**, and **Linux**.

Inspired by [Vingester](https://github.com/rse/vingester).

## Stack

Tauri 2, NDI, embedded **Servo** (no separate Chrome install).

## Prerequisites

- [Rust](https://rustup.rs/) (workspace pins **1.88** via `rust-toolchain.toml`)
- [Node.js](https://nodejs.org/)
- [NDI SDK](https://ndi.video/type/developer/)
- **Windows**: Visual Studio **C++ build tools** (for `mozangle` / Servo). Use a **Developer** shell or CI’s `ilammy/msvc-dev-cmd` so MSVC is on `PATH`.

On Windows, a 64-bit [LLVM](https://releases.llvm.org/) installation may be required to build NDI-related bindings. This repo includes [`.cargo/config.toml`](.cargo/config.toml) with `BINDGEN_EXTRA_CLANG_ARGS = "-m64"` for MSVC.

## GitHub Actions

Workflows live under [`.github/workflows/`](.github/workflows/).

| Workflow | When | What |
|----------|------|------|
| **CI** (`ci.yml`) | Push / PR to `main` or `master` | **check**: `cargo fmt`; **test**: `npm ci` + `npm run build`; **build** (Windows): LLVM via Chocolatey, then `cargo test` / `cargo build --release` if the [NDI 6 SDK](https://ndi.video/type/developer/) is installed on the runner (otherwise those steps are skipped; job is `continue-on-error` so PRs stay mergeable). |
| **Release** (`release.yml`) | Push a tag `v*` (e.g. `v0.1.0`) | Same as above, plus `--config bundle-with-updater.conf.json` so signed **`latest.json`** / updater artifacts are produced (requires NDI SDK + `TAURI_SIGNING_PRIVATE_KEY`). Local `npm run tauri build` skips updater artifact signing unless you set that env and merge the same config. |

### Release secrets

Create a minisign key pair (private key only in GitHub Secrets; public key is already set in `src-tauri/tauri.conf.json` for this repo—replace both if you rotate keys):

```bash
npm run tauri signer generate -w momaku-signing.key
```

Then add to the repository:

- **`TAURI_SIGNING_PRIVATE_KEY`**: contents of the generated **private** key (or use `TAURI_SIGNING_PRIVATE_KEY_PATH` in local builds only; not for Actions).
- **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`**: optional, if you set a password when generating.

Releases are created as **drafts**; review and publish them on GitHub. The updater endpoint is:

`https://github.com/MikanseiLaboratory/momaku/releases/latest/download/latest.json`

## Auto updater

The app uses [Tauri’s updater plugin](https://v2.tauri.app/plugin/updater/) (`tauri-plugin-updater` + `@tauri-apps/plugin-updater`). Use **「更新を確認」** in the UI to check, download, install, and restart via `tauri-plugin-process`.

Update signing is mandatory: the public key in [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) must match the private key used when building release artifacts (`TAURI_SIGNING_PRIVATE_KEY` in CI).

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
