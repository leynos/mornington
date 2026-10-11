#!/bin/bash
set -euo pipefail
case "$1" in
  -s) printf '%s\n' "$HOST_OS" ;;
  -m) printf '%s\n' "$HOST_ARCH" ;;
  *) exit 92 ;;
esac
