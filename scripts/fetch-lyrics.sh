#!/usr/bin/env bash
# ~/.config/rmpc/scripts/fetch-lyrics.sh
# Fetches synced (.lrc) or plain (.txt) lyrics from lrclib.net.
# Synced lyrics are always preferred. Plain lyrics are saved as .txt
# for later stamping with lrc-stamper.py — never as .lrc.

set -uo pipefail

LYRICS_DIR="$HOME/.config/rmpc/lyrics"

# Fallback to rmpc song if called directly without environment variables
if [[ -z "${TITLE:-}" || -z "${ARTIST:-}" || -z "${FILE:-}" ]]; then
    SONG_JSON=$(rmpc song 2>/dev/null || true)
    if [[ -n "$SONG_JSON" ]]; then
        FILE="${FILE:-$(printf '%s' "$SONG_JSON" | jq -r '.file // empty' 2>/dev/null || true)}"
        TITLE="${TITLE:-$(printf '%s' "$SONG_JSON" | jq -r '.metadata.title // empty' 2>/dev/null || true)}"
        ARTIST="${ARTIST:-$(printf '%s' "$SONG_JSON" | jq -r '.metadata.artist // empty' 2>/dev/null || true)}"
        ALBUM="${ALBUM:-$(printf '%s' "$SONG_JSON" | jq -r '.metadata.album // empty' 2>/dev/null || true)}"
        DURATION="${DURATION:-$(printf '%s' "$SONG_JSON" | jq -r '.duration.secs // empty' 2>/dev/null || true)}"
    fi
fi

[[ -z "${TITLE:-}" || -z "${ARTIST:-}" || -z "${FILE:-}" ]] && exit 0
# rmpc supplies a library-relative path. Keep it relative and reject parent
# traversal (or control characters) before using it below the lyrics root.
[[ "$FILE" != /* && "$FILE" != *$'\n'* && "$FILE" != *$'\r'* ]] || exit 0
case "/$FILE/" in
    */../*) exit 0 ;;
esac

SKIP=${LYRICS_SKIP:-0}
FORCE=${LYRICS_FORCE:-0}

LRC_FILE="$LYRICS_DIR/${FILE%.*}.lrc"
TXT_FILE="$LYRICS_DIR/${FILE%.*}.txt"
NOTFOUND_FILE="$LYRICS_DIR/${FILE%.*}.notfound"

# Force refetch clears negative cache and placeholder lyrics
if [[ "$FORCE" -eq 1 ]]; then
    rm -f "$NOTFOUND_FILE"
    if [[ -f "$LRC_FILE" ]] && grep -q "<no lyrics>" "$LRC_FILE" 2>/dev/null; then
        rm -f "$LRC_FILE"
    fi
fi

# Already have real synced lyrics — nothing to do.
if [[ -f "$LRC_FILE" ]] && ! grep -q "<no lyrics>" "$LRC_FILE" 2>/dev/null; then
    exit 0
fi

# Negative cache: lrclib had no lyrics when last checked.
# Auto-expire after 14 days. Bypass if user requests alternatives (SKIP > 0).
if [[ "$SKIP" -eq 0 && -f "$NOTFOUND_FILE" ]]; then
    find "$NOTFOUND_FILE" -mtime +14 -delete 2>/dev/null || true
    if [[ -f "$NOTFOUND_FILE" ]]; then
        if [[ ! -f "$LRC_FILE" ]]; then
            printf '[00:00.00] <no lyrics>\n' > "$LRC_FILE"
            rmpc remote indexlrc --path "$LRC_FILE" 2>/dev/null || true
        fi
        exit 0
    fi
    rm -f "$LRC_FILE"
fi

mkdir -p "$(dirname "$LRC_FILE")"

duration_to_sec() {
    local d="${1%%.*}" # strip fractional seconds if present (.183, etc.)
    local IFS=':'
    local -a parts=($d)
    case ${#parts[@]} in
        1) echo "${parts[0]:-0}" ;;
        2) echo $(( 10#${parts[0]:-0} * 60  + 10#${parts[1]:-0} )) ;;
        3) echo $(( 10#${parts[0]:-0} * 3600 + 10#${parts[1]:-0} * 60 + 10#${parts[2]:-0} )) ;;
        *) echo 0 ;;
    esac
}

DURATION_SEC=$(duration_to_sec "${DURATION:-0}")

USER_AGENT="rmpc-lyrics/1.0 (+https://github.com/mierak/rmpc)"

api_get() {
    local artist="$1"
    local title="$2"
    local album="${3:-}"
    local duration="${4:-}"
    local -a args=(
        -sG "https://lrclib.net/api/get"
        -A "$USER_AGENT"
        --connect-timeout 2
        --max-time 8
        --data-urlencode "artist_name=${artist}"
        --data-urlencode "track_name=${title}"
    )
    [[ -n "$album" ]] && args+=(--data-urlencode "album_name=${album}")
    [[ -n "$duration" && "$duration" -gt 0 ]] 2>/dev/null && args+=(--data-urlencode "duration=${duration}")
    curl "${args[@]}" 2>/dev/null || true
}

api_search_structured() {
    local artist="$1"
    local title="$2"
    curl -sG "https://lrclib.net/api/search" \
        -A "$USER_AGENT" \
        --connect-timeout 2 \
        --max-time 8 \
        --data-urlencode "artist_name=${artist}" \
        --data-urlencode "track_name=${title}" 2>/dev/null || true
}

api_search_q() {
    local q="$1"
    curl -sG "https://lrclib.net/api/search" \
        -A "$USER_AGENT" \
        --connect-timeout 2 \
        --max-time 8 \
        --data-urlencode "q=${q}" 2>/dev/null || true
}

save_synced_and_exit() {
    local synced="$1"
    if [[ -n "$synced" ]]; then
        printf '%s\n' "$synced" > "$LRC_FILE"
        rm -f "$NOTFOUND_FILE"
        rmpc remote indexlrc --path "$LRC_FILE" 2>/dev/null || true
        exit 0
    fi
}

is_server_error() {
    local resp="$1"
    [[ -z "$resp" ]] && return 0
    if printf '%s' "$resp" | jq -e 'if type == "object" and ((.name // "") == "ServerOverloaded" or ((.statusCode // 0) >= 500)) then true else false end' 2>/dev/null | grep -q true; then
        return 0
    fi
    return 1
}

# Title normalization: strip track number prefixes and tags like (Remastered)
CLEAN_TITLE=$(printf '%s' "$TITLE" | sed -E \
    -e 's/^[0-9]+[ -.]+//' \
    -e 's/\s*[\(\[](remaster(ed)?|anniversary|deluxe|mono|stereo|expanded|bonus track|explicit|live|version|edit|take [0-9]+)[^\)\]]*[\)\]]//gI' \
    -e 's/\s*-\s*(remaster(ed)?|anniversary|deluxe|mono|stereo|live|single).*$//gI' \
    -e 's/\s+-\s*$//' || echo "$TITLE")

PLAIN_LYRICS=""
HAD_SUCCESSFUL_QUERY=0

# --- Attempt 1: Exact and relaxed /api/get matches (skipped when cycling alternatives) ---
if [[ "$SKIP" -eq 0 ]]; then
    # 1A: Exact match with album and duration
    RESP=$(api_get "$ARTIST" "$TITLE" "${ALBUM:-}" "$DURATION_SEC")
    if ! is_server_error "$RESP"; then
        HAD_SUCCESSFUL_QUERY=1
        SYNCED=$(printf '%s' "$RESP" | jq -r 'if type == "object" then .syncedLyrics // empty else empty end' 2>/dev/null || true)
        save_synced_and_exit "$SYNCED"
        PLAIN_LYRICS=$(printf '%s' "$RESP" | jq -r 'if type == "object" then .plainLyrics // empty else empty end' 2>/dev/null || true)
    fi

    # 1B: Exact match by artist + title, omitting album and duration restrictions
    RESP=$(api_get "$ARTIST" "$TITLE")
    if ! is_server_error "$RESP"; then
        HAD_SUCCESSFUL_QUERY=1
        SYNCED=$(printf '%s' "$RESP" | jq -r 'if type == "object" then .syncedLyrics // empty else empty end' 2>/dev/null || true)
        save_synced_and_exit "$SYNCED"
        [[ -z "$PLAIN_LYRICS" ]] && PLAIN_LYRICS=$(printf '%s' "$RESP" | jq -r 'if type == "object" then .plainLyrics // empty else empty end' 2>/dev/null || true)
    fi

    # 1C: If clean title differs, try with clean title
    if [[ "$CLEAN_TITLE" != "$TITLE" ]]; then
        RESP=$(api_get "$ARTIST" "$CLEAN_TITLE")
        if ! is_server_error "$RESP"; then
            HAD_SUCCESSFUL_QUERY=1
            SYNCED=$(printf '%s' "$RESP" | jq -r 'if type == "object" then .syncedLyrics // empty else empty end' 2>/dev/null || true)
            save_synced_and_exit "$SYNCED"
            [[ -z "$PLAIN_LYRICS" ]] && PLAIN_LYRICS=$(printf '%s' "$RESP" | jq -r 'if type == "object" then .plainLyrics // empty else empty end' 2>/dev/null || true)
        fi
    fi
fi

# --- Attempt 2: Search with candidate scoring ---
jq_parse_search='
def normalize_title:
    ascii_downcase
    | gsub("\\s*[\\[\\(](remaster(ed)?|anniversary|deluxe|mono|stereo|live|version|edit|take [0-9]+|feat\\.?|ft\\.?)[^\\]\\)]*[\\]\\)]"; "")
    | gsub("\\s*-\\s*(remaster(ed)?|anniversary|deluxe|mono|stereo|live).*$"; "")
    | gsub("[-_\\s]+$"; "")
    | gsub("^[-_\\s]+"; "");

def norm_art: ascii_downcase | gsub("^the\\s+"; "");

def match_score:
    .artistName as $a | .trackName as $t |
    if (($a | norm_art) != ($artist | norm_art)) then
        -1
    elif (($t | ascii_downcase) == ($title | ascii_downcase)) then
        3
    elif (($t | normalize_title) == ($title | normalize_title)) then
        2
    elif (($title | normalize_title | length > 3) and ((($t | normalize_title) | contains($title | normalize_title)) or (($title | normalize_title) | contains($t | normalize_title)))) then
        1
    else
        -1
    end;

if type == "array" then . else [] end
| map(. + { score: match_score })
| map(select(.score >= 0))
| sort_by(-.score) as $matches
| {
    synced: (($matches | map(select(.syncedLyrics | (. != null and . != ""))))[$skip].syncedLyrics // null),
    plain:  (($matches | map(select(.plainLyrics  | (. != null and . != ""))))[$skip].plainLyrics  // null)
  }
'

# 2A: Structured search by artist & clean title
SEARCH=$(api_search_structured "$ARTIST" "$CLEAN_TITLE")
if ! is_server_error "$SEARCH"; then
    HAD_SUCCESSFUL_QUERY=1
    PARSED=$(printf '%s' "$SEARCH" | jq -c --arg artist "$ARTIST" --arg title "$TITLE" --argjson skip "$SKIP" "$jq_parse_search" 2>/dev/null || true)
    if [[ -n "$PARSED" ]]; then
        SYNCED=$(printf '%s' "$PARSED" | jq -r '.synced // empty' 2>/dev/null || true)
        save_synced_and_exit "$SYNCED"
        [[ -z "$PLAIN_LYRICS" ]] && PLAIN_LYRICS=$(printf '%s' "$PARSED" | jq -r '.plain // empty' 2>/dev/null || true)
    fi
fi

# 2B: Structured search by artist & raw title (if distinct)
if [[ "$CLEAN_TITLE" != "$TITLE" ]]; then
    SEARCH=$(api_search_structured "$ARTIST" "$TITLE")
    if ! is_server_error "$SEARCH"; then
        HAD_SUCCESSFUL_QUERY=1
        PARSED=$(printf '%s' "$SEARCH" | jq -c --arg artist "$ARTIST" --arg title "$TITLE" --argjson skip "$SKIP" "$jq_parse_search" 2>/dev/null || true)
        if [[ -n "$PARSED" ]]; then
            SYNCED=$(printf '%s' "$PARSED" | jq -r '.synced // empty' 2>/dev/null || true)
            save_synced_and_exit "$SYNCED"
            [[ -z "$PLAIN_LYRICS" ]] && PLAIN_LYRICS=$(printf '%s' "$PARSED" | jq -r '.plain // empty' 2>/dev/null || true)
        fi
    fi
fi

# 2C: Full-text search fallback (q=)
SEARCH=$(api_search_q "${ARTIST} ${CLEAN_TITLE}")
if ! is_server_error "$SEARCH"; then
    HAD_SUCCESSFUL_QUERY=1
    PARSED=$(printf '%s' "$SEARCH" | jq -c --arg artist "$ARTIST" --arg title "$TITLE" --argjson skip "$SKIP" "$jq_parse_search" 2>/dev/null || true)
    if [[ -n "$PARSED" ]]; then
        SYNCED=$(printf '%s' "$PARSED" | jq -r '.synced // empty' 2>/dev/null || true)
        save_synced_and_exit "$SYNCED"
        [[ -z "$PLAIN_LYRICS" ]] && PLAIN_LYRICS=$(printf '%s' "$PARSED" | jq -r '.plain // empty' 2>/dev/null || true)
    fi
fi

# --- Fallback: Plain lyrics (.txt) for later stamping ---
if [[ -n "$PLAIN_LYRICS" ]]; then
    if [[ ! -f "$TXT_FILE" ]]; then
        printf '%s\n' "$PLAIN_LYRICS" > "$TXT_FILE"
    fi
    printf '[00:00.00] <no lyrics>\n' > "$LRC_FILE"
    touch "$NOTFOUND_FILE"
    rmpc remote indexlrc --path "$LRC_FILE" 2>/dev/null || true
    exit 0
fi

# If at least one query completed successfully without server error and returned no lyrics
if [[ "$HAD_SUCCESSFUL_QUERY" -eq 1 ]]; then
    printf '[00:00.00] <no lyrics>\n' > "$LRC_FILE"
    touch "$NOTFOUND_FILE"
    rmpc remote indexlrc --path "$LRC_FILE" 2>/dev/null || true
fi

exit 0
