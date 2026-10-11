#!/bin/bash
set -euo pipefail
printf 'curl %s\n' "$*" >> "$COMMAND_LOG"
while [[ $# -gt 0 ]]; do
  if [[ "$1" == --output ]]; then
    /bin/cp -- "$FIXTURE_ARCHIVE" "$2"
    exit 0
  fi
  shift
done
exit 93
