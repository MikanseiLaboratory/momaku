#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"

ROOT="${GITHUB_WORKSPACE}/.ndi"
mkdir -p "${ROOT}/tmp"
cd "${ROOT}/tmp"
rm -f ndi-mac.tar.gz

DEFAULT_URLS=(
  "https://downloads.ndi.tv/SDK/NDI_SDK_Apple/Install_NDI_SDK_v6_Apple.tar.gz"
  "https://downloads.ndi.tv/SDK/NDI_SDK_MacOS/Install_NDI_SDK_v6_Apple.tar.gz"
)

echo "::group::Download NDI SDK for macOS"
if [[ -n "${NDI_SDK_MACOS_URL:-}" ]]; then
  curl -fL --retry 5 -A "Mozilla/5.0" "${NDI_SDK_MACOS_URL}" -o ndi-mac.tar.gz
else
  ok=""
  for u in "${DEFAULT_URLS[@]}"; do
    if curl -fL --retry 3 -A "Mozilla/5.0" "$u" -o ndi-mac.tar.gz; then
      ok=1
      break
    fi
    rm -f ndi-mac.tar.gz
  done
  if [[ -z "${ok}" ]]; then
    echo "::error::Could not download NDI SDK for macOS. Set repository secret NDI_SDK_MACOS_URL to a direct .tar.gz URL (NDI SDK for Apple / macOS)."
    exit 1
  fi
fi
echo "::endgroup::"

echo "::group::Install NDI SDK for macOS (non-interactive)"
tar -xzf ndi-mac.tar.gz
shopt -s nullglob
for s in ./Install_NDI_SDK*.sh; do
  yes | PAGER=cat sh "${s}" || true
done
shopt -u nullglob
echo "::endgroup::"

for p in "/Library/NDI SDK for macOS" "/Library/NDI 6 SDK" "/Library/NDI SDK for Apple"; do
  if [[ -f "${p}/include/Processing.NDI.Lib.h" ]] || [[ -f "${p}/include/Processing.NDI.lib.h" ]]; then
    echo "NDI_SDK_DIR=${p}" >>"${GITHUB_ENV}"
    echo "Configured NDI_SDK_DIR=${p} (system install)"
    exit 0
  fi
done

hdr="$(find "${ROOT}/tmp" -type f \( -name 'Processing.NDI.Lib.h' -o -name 'Processing.NDI.lib.h' \) 2>/dev/null | head -1 || true)"
if [[ -z "${hdr}" ]]; then
  echo "::error::NDI header not found under ${ROOT}/tmp or /Library after install."
  exit 1
fi
SDK="$(cd "$(dirname "${hdr}")/.." && pwd)"
rm -rf "${ROOT}/ndisdk-pre"
mv "${SDK}" "${ROOT}/ndisdk-pre"

echo "NDI_SDK_DIR=${ROOT}/ndisdk-pre" >>"${GITHUB_ENV}"
echo "Configured NDI_SDK_DIR=${ROOT}/ndisdk-pre"
