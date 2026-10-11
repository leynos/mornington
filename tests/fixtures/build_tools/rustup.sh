#!/bin/bash
set -euo pipefail
printf 'rustup %s\n' "$*" >> "$COMMAND_LOG"
case "$1 $2" in
  'toolchain install') exit 0 ;;
  'component list')
    for component in clippy llvm-tools rustc-codegen-cranelift rustfmt; do
      [[ "$component" == "${MISSING_COMPONENT:-}" ]] && continue
      printf '%s-x86_64-unknown-linux-gnu\n' "$component"
    done
    ;;
  'run nightly-2026-08-27')
    if [[ "$4" == '-vV' ]]; then
      printf 'host: x86_64-unknown-linux-gnu\n'
    else
      printf 'rustc fixture\n'
    fi
    ;;
  *) exit 91 ;;
esac
