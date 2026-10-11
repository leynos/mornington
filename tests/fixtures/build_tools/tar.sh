#!/bin/bash
set -euo pipefail
printf 'tar %s\n' "$*" >> "$COMMAND_LOG"
exec /usr/bin/tar "$@"
