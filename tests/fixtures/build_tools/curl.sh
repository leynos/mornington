#!/bin/bash
set -euo pipefail
printf 'curl %s\n' "$*" >> "$COMMAND_LOG"
if [[ "${FAIL_DOWNLOAD:-0}" == 1 ]]; then
  printf 'fixture download refused\n' >&2
  exit 22
fi
while [[ $# -gt 0 ]]; do
  if [[ "$1" == --output ]]; then
    /bin/cp -- "$FIXTURE_ARCHIVE" "$2"
    exit 0
  fi
  shift
done
exit 93
