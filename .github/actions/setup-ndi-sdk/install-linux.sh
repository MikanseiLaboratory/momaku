#!/usr/bin/env bash
# Adapted from DistroAV CI/libndi-get.sh (NDI SDK v6 for Linux).
# https://github.com/DistroAV/DistroAV/blob/master/CI/libndi-get.sh
set -euo pipefail

: "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"

ROOT="${GITHUB_WORKSPACE}/.ndi"
mkdir -p "${ROOT}/tmp"
cd "${ROOT}/tmp"

INSTALLER_NAME="Install_NDI_SDK_v6_Linux"
INSTALLER="${INSTALLER_NAME}.tar.gz"
DEFAULT_URL="https://downloads.ndi.tv/SDK/NDI_SDK_Linux/${INSTALLER}"
URL="${NDI_SDK_LINUX_URL:-$DEFAULT_URL}"

echo "::group::Download NDI SDK for Linux"
curl -fL --retry 5 -A "Mozilla/5.0" "${URL}" -o "${INSTALLER}"
echo "::endgroup::"

echo "::group::Extract NDI SDK for Linux"
tar -xzf "${INSTALLER}"
yes | PAGER="cat" sh "${INSTALLER_NAME}.sh"
rm -rf "${ROOT}/ndisdk-pre"
if [[ -d "NDI SDK for Linux" ]]; then
  mv "NDI SDK for Linux" "${ROOT}/ndisdk-pre"
else
  echo "::error::Expected 'NDI SDK for Linux' directory after running ${INSTALLER_NAME}.sh"
  exit 1
fi
echo "::endgroup::"

echo "NDI_SDK_DIR=${ROOT}/ndisdk-pre" >>"${GITHUB_ENV}"
echo "Configured NDI_SDK_DIR=${ROOT}/ndisdk-pre"
