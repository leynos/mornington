#!/usr/bin/env bash
# Installs the project-pinned Rust toolchain and verified Linux mold binary.

set -euo pipefail

readonly SCRIPT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=build-tools-common.sh
source "${SCRIPT_ROOT}/build-tools-common.sh"

readonly BUILD_TOOLS_PREFIX="${BUILD_TOOLS_PREFIX:-${HOME}/.local}"
readonly LINKER_RELEASE_BASE_URL="https://github.com/rui314/mold/releases/download"

install_toolchain() {
  command -v rustup >/dev/null 2>&1 || fail "rustup is required to install the pinned toolchain"
  local toolchain component
  toolchain="$(pinned_toolchain)"
  local -a component_args=()
  while IFS= read -r component; do
    component_args+=(--component "${component}")
  done < <(required_components)
  rustup toolchain install "${toolchain}" --profile minimal "${component_args[@]}"
}

install_linker() {
  [[ "$(uname -s)" == Linux ]] || return 0
  command -v curl >/dev/null 2>&1 || fail "curl is required to download mold"
  command -v sha256sum >/dev/null 2>&1 || fail "sha256sum is required to verify mold"
  command -v tar >/dev/null 2>&1 || fail "tar is required to install mold"

  local version archive expected archive_path actual
  version="$(linker_version)"
  archive="$(linker_archive_name "${version}")"
  expected="$(expected_linker_hash "${archive}")"
  archive_path="$(mktemp --suffix=.tar.gz)"
  trap 'rm -f -- "${archive_path}"' RETURN
  curl --fail --location --proto '=https' --tlsv1.2 --retry 3 --silent --show-error \
    "${LINKER_RELEASE_BASE_URL}/v${version}/${archive}" --output "${archive_path}"
  actual="$(sha256sum "${archive_path}" | awk '{print $1}')"
  [[ "${actual}" == "${expected}" ]] || fail "checksum mismatch for ${archive}"
  mkdir -p -- "${BUILD_TOOLS_PREFIX}"
  tar --extract --gzip --file "${archive_path}" --strip-components=1 --directory "${BUILD_TOOLS_PREFIX}"
}

install_toolchain
install_linker
