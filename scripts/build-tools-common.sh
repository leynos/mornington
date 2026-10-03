#!/usr/bin/env bash
# Shared, read-only helpers for the checked build-tool provisioner and probe.

set -euo pipefail

readonly BUILD_TOOLS_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly LINKER_VERSION_FILE="${BUILD_TOOLS_ROOT}/tools/mold/VERSION"
readonly LINKER_HASH_FILE="${BUILD_TOOLS_ROOT}/tools/mold/SHA256SUMS"
readonly TOOLCHAIN_FILE="${BUILD_TOOLS_ROOT}/rust-toolchain.toml"

fail() {
  printf 'build-tools: %s\n' "$*" >&2
  exit 1
}

require_file() {
  [[ -r "$1" ]] || fail "required file is not readable: $1"
}

linker_version() {
  require_file "${LINKER_VERSION_FILE}"
  local version
  version="$(tr -d '\r\n' < "${LINKER_VERSION_FILE}")"
  [[ "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "invalid mold version: ${version}"
  printf '%s\n' "${version}"
}

pinned_toolchain() {
  require_file "${TOOLCHAIN_FILE}"
  local -a channels=()
  local line value
  while IFS= read -r line; do
    [[ "${line}" =~ ^[[:space:]]*channel[[:space:]]*=[[:space:]]*\"([^\"]+)\"[[:space:]]*$ ]] || continue
    value="${BASH_REMATCH[1]}"
    channels+=("${value}")
  done < "${TOOLCHAIN_FILE}"
  [[ ${#channels[@]} -eq 1 ]] || fail "rust-toolchain.toml must contain one quoted channel"
  [[ "${channels[0]}" == nightly-* ]] || fail "toolchain must pin a dated nightly channel"
  printf '%s\n' "${channels[0]}"
}

required_components() {
  require_file "${TOOLCHAIN_FILE}"
  sed -n '/^[[:space:]]*components[[:space:]]*=/,/]/p' "${TOOLCHAIN_FILE}" |
    grep -oE '"[^"]+"' | tr -d '"'
}

linker_archive_name() {
  local version="$1"
  local machine
  machine="$(uname -m)"
  case "${machine}" in
    x86_64) printf 'mold-%s-x86_64-linux.tar.gz\n' "${version}" ;;
    aarch64) printf 'mold-%s-aarch64-linux.tar.gz\n' "${version}" ;;
    *) fail "mold ${version} has no approved archive for Linux architecture ${machine}" ;;
  esac
}

expected_linker_hash() {
  local archive="$1"
  require_file "${LINKER_HASH_FILE}"
  local -a hashes=()
  local hash name
  while read -r hash name; do
    [[ -n "${hash}" && -n "${name}" ]] || continue
    [[ "${name}" == "${archive}" ]] && hashes+=("${hash}")
  done < "${LINKER_HASH_FILE}"
  [[ ${#hashes[@]} -eq 1 ]] || fail "expected one checksum for ${archive}"
  [[ "${hashes[0]}" =~ ^[[:xdigit:]]{64}$ ]] || fail "invalid checksum for ${archive}"
  printf '%s\n' "${hashes[0]}"
}
