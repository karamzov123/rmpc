#!/bin/bash
# Runs both scripts on song change
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$SCRIPT_DIR/playcount"
"$SCRIPT_DIR/fetch-lyrics.sh"
