#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"

ROOT="${GITHUB_WORKSPACE}/third_party/ndi-sdk-6"
if [[ ! -f "${ROOT}/include/Processing.NDI.Lib.h" ]]; then
  echo "::error::Vendored NDI SDK missing at ${ROOT}/include. Run scripts/vendor-ndi-sdk-from-local.ps1 on Windows (see NOTICE)."
  exit 1
fi

os="$(uname -s)"
case "${os}" in
  Linux)
    L="${ROOT}/lib/x86_64-linux-gnu"
    real="$(find "${L}" -maxdepth 1 -name 'libndi.so.*.*.*' -type f 2>/dev/null | head -1 || true)"
    if [[ -z "${real}" ]]; then
      echo "::error::No versioned libndi.so.*.*.* under ${L}"
      exit 1
    fi
    target="$(basename "${real}")"
    ln -sfn "${target}" "${L}/libndi.so"
    ln -sfn "${target}" "${L}/libndi.so.6"
    ;;
  Darwin)
    if [[ ! -f "${ROOT}/lib/macOS/libndi.dylib" ]]; then
      echo "::error::Missing ${ROOT}/lib/macOS/libndi.dylib"
      exit 1
    fi
    ;;
  *)
    echo "::error::Unsupported OS: ${os}"
    exit 1
    ;;
esac

echo "NDI_SDK_DIR=${ROOT}" >>"${GITHUB_ENV}"
echo "NDI_SDK_DIR=${ROOT}"
