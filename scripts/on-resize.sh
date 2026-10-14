#!/bin/bash
# ~/.config/rmpc/scripts/on-resize.sh
# Swaps rmpc theme based on terminal width.
# $COLUMNS and $LINES are set by rmpc's on_resize hook.
# $PID targets the specific rmpc instance.

MINI_THEME="$HOME/.config/rmpc/themes/afterglow-mini.ron"
FULL_THEME="$HOME/.config/rmpc/themes/afterglow-compact.ron"
STATE_FILE="/tmp/rmpc_theme_${PID:-$$}"

TARGET="$FULL_THEME"
[ "${COLUMNS:-999}" -le 45 ] && TARGET="$MINI_THEME"

LAST=$(cat "$STATE_FILE" 2>/dev/null || true)
if [ "$LAST" != "$TARGET" ]; then
    echo "$TARGET" > "$STATE_FILE"
    rmpc remote ${PID:+--pid "$PID"} set theme "$TARGET" 2>/dev/null || true
fi
exit 0
