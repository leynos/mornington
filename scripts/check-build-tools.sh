#!/usr/bin/env bash
# Verifies the local prerequisites for development and LLVM coverage builds.

set -euo pipefail

readonly SCRIPT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=build-tools-common.sh
source "${SCRIPT_ROOT}/build-tools-common.sh"

pinned_host() {
  local toolchain host
  toolchain="$(pinned_toolchain)"
  host="$(rustup run "${toolchain}" rustc -vV | awk -F ': ' '$1 == "host" { print $2 }')"
  [[ "${host}" =~ ^[[:alnum:]_]+(-[[:alnum:]_]+)+$ ]] ||
    fail "could not read one Rust host triple from ${toolchain}"
  printf '%s\n' "${host}"
}

installed_component_name() {
  local requested="$1"
  local host="$2"
  local installed_base
  case "${requested}" in
    llvm-tools-preview) installed_base="llvm-tools" ;;
    rustc-codegen-cranelift-preview) installed_base="rustc-codegen-cranelift" ;;
    clippy|rustfmt) installed_base="${requested}" ;;
    rust-src) printf '%s\n' "${requested}"; return 0 ;;
    *) fail "unsupported rustup component alias in rust-toolchain.toml: ${requested}" ;;
  esac
  printf '%s-%s\n' "${installed_base}" "${host}"
}

component_is_installed() {
  local expected="$1"
  local installed="$2"
  grep -Fxq "${expected}" <<< "${installed}"
}

check_rust_toolchain() {
  command -v rustup >/dev/null 2>&1 || fail "rustup is required; run make install-build-tools"
  local toolchain host component expected installed
  toolchain="$(pinned_toolchain)"
  rustup run "${toolchain}" rustc --version >/dev/null ||
    fail "missing toolchain ${toolchain}; run make install-build-tools"
  host="$(pinned_host)"
  installed="$(rustup component list --toolchain "${toolchain}" --installed)"
  while IFS= read -r component; do
    expected="$(installed_component_name "${component}" "${host}")"
    component_is_installed "${expected}" "${installed}" ||
      fail "toolchain ${toolchain} lacks ${component} (expected installed name ${expected}); run make install-build-tools"
  done < <(required_components)
}

check_linux_linkers() {
  [[ "$(uname -s)" == Linux ]] || return 0
  command -v clang >/dev/null 2>&1 || fail "clang is required for the configured Linux linker"
  command -v mold >/dev/null 2>&1 || fail "mold is required for the configured Linux linker"
  local expected actual
  expected="$(linker_version)"
  actual="$(mold --version | awk 'NR == 1 {print $2}')"
  [[ "${actual}" == "${expected}" ]] || fail "mold ${expected} is required; found ${actual:-unreadable}"
}

check_coverage_linker() {
  [[ "$1" == --coverage ]] || return 0
  [[ "$(uname -s)" == Linux ]] || return 0
  command -v ld.lld >/dev/null 2>&1 || fail "ld.lld is required for LLVM coverage"
}

main() {
  case "${1:-}" in
    ""|--coverage) ;;
    *) fail "unknown argument: $1" ;;
  esac
  check_rust_toolchain
  check_linux_linkers
  check_coverage_linker "${1:-}"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
