#!/usr/bin/env python3
"""
lrc-stamper — stamp plain lyrics with live MPD timestamps, or edit existing .lrc files.

Launched via rmpc ExternalCommand (env: FILE, ARTIST, TITLE set by rmpc) or standalone.

Controls:
  Space / Enter  stamp current line with playback position, advance
  j / ↓          move down (no stamp change)
  k / ↑          move up   (no stamp change)
  b / ←          go back one line and clear its stamp
  s              skip line (no timestamp), advance
  e              edit current line's timestamp manually
  o              shift all timestamps by an offset (+/- seconds)
  d              delete .lrc and fetch next result  [edit mode only]
  r              restart from line 0, clear all stamps  [stamp mode only]
  q / Esc        save .lrc and quit
"""

import curses
import os
import re
import socket
import subprocess
from pathlib import Path

LYRICS_DIR  = Path.home() / ".config/rmpc/lyrics"
MPD_HOST    = os.environ.get("MPD_HOST", "127.0.0.1")
MPD_PORT    = int(os.environ.get("MPD_PORT", "6600"))

LRC_TS_RE   = re.compile(r"^\[(\d{1,2}):(\d{2}(?:\.\d+)?)\](.*)")
LRC_META_RE = re.compile(r"^\[[a-zA-Z]+:[^\]]*\]\s*$")

HELP_STAMP = " [SPC] Stamp  [j/k] Nav  [e] Edit ts  [o] Offset  [b] Back  [s] Skip  [r] Restart  [q] Save+quit"
HELP_EDIT  = " [SPC] Re-stamp  [j/k] Nav  [e] Edit ts  [o] Offset  [b] Back  [d] Refetch  [q] Save+quit"


# ---------------------------------------------------------------------------
# MPD
# ---------------------------------------------------------------------------

def _mpd_cmd(cmd: str) -> dict[str, str]:
    try:
        with socket.create_connection((MPD_HOST, MPD_PORT), timeout=2) as conn:
            rfile = conn.makefile("rb")
            rfile.readline()
            conn.sendall((cmd + "\n").encode())
            result: dict[str, str] = {}
            for raw in rfile:
                line = raw.decode(errors="replace").rstrip("\n")
                if line == "OK" or line.startswith("ACK"):
                    break
                if ": " in line:
                    k, v = line.split(": ", 1)
                    result[k] = v
            return result
    except OSError:
        return {}


def mpd_elapsed() -> float:
    return float(_mpd_cmd("status").get("elapsed", 0.0))


def mpd_currentsong() -> dict[str, str]:
    return _mpd_cmd("currentsong")


def mpd_seek_start() -> None:
    _mpd_cmd("seekcur 0")


# ---------------------------------------------------------------------------
# LRC helpers
# ---------------------------------------------------------------------------

def fmt_lrc(secs: float) -> str:
    m = int(secs) // 60
    s = secs - m * 60
    return f"[{m:02d}:{s:05.2f}]"


def fmt_clock(secs: float) -> str:
    m = int(secs) // 60
    s = int(secs) % 60
    return f"{m}:{s:02d}"


def parse_ts(s: str) -> float | None:
    """Parse mm:ss.cs, mm:ss, or bare seconds into a float."""
    s = s.strip()
    try:
        if ":" in s:
            m_str, rest = s.split(":", 1)
            return int(m_str) * 60 + float(rest)
        return float(s)
    except ValueError:
        return None


def apply_offset(stamps: list[float | None], delta: float) -> list[float | None]:
    return [max(0.0, ts + delta) if ts is not None else None for ts in stamps]


def refetch_lyrics(lrc_path: Path, song_file: str, artist: str, title: str,
                   album: str, duration: str, skip: int) -> bool:
    """Delete lrc_path, run fetch-lyrics.sh with LYRICS_SKIP=skip. Returns True if a new .lrc appeared."""
    lrc_path.unlink(missing_ok=True)
    env = os.environ.copy()
    env.update({
        "FILE": song_file, "ARTIST": artist, "TITLE": title,
        "ALBUM": album, "DURATION": duration,
        "LYRICS_SKIP": str(skip),
    })
    script = Path(__file__).parent / "fetch-lyrics.sh"
    try:
        subprocess.run([str(script)], env=env, timeout=15)
    except (subprocess.TimeoutExpired, OSError):
        pass
    return lrc_path.exists()


# ---------------------------------------------------------------------------
# File I/O
# ---------------------------------------------------------------------------

def load_txt(path: Path) -> list[str]:
    raw = path.read_text(encoding="utf-8").splitlines()
    while raw and not raw[0].strip():
        raw.pop(0)
    while raw and not raw[-1].strip():
        raw.pop()
    return raw


def load_lrc(path: Path) -> tuple[list[str], list[float | None]]:
    lines: list[str] = []
    stamps: list[float | None] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        m = LRC_TS_RE.match(raw)
        if m:
            mm, ss_cs, text = m.groups()
            stamps.append(int(mm) * 60 + float(ss_cs))
            lines.append(text)
        elif LRC_META_RE.match(raw) or not raw.strip():
            pass  # skip metadata and blank lines
        else:
            stamps.append(None)
            lines.append(raw)
    return lines, stamps


def save_lrc(path: Path, lines: list[str], stamps: list[float | None]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        for line, ts in zip(lines, stamps):
            prefix = fmt_lrc(ts) if ts is not None else ""
            f.write(f"{prefix}{line}\n")


# ---------------------------------------------------------------------------
# TUI
# ---------------------------------------------------------------------------

def prompt(scr, *lines: str) -> int:
    scr.erase()
    for i, line in enumerate(lines):
        try:
            scr.addstr(i, 0, line)
        except curses.error:
            pass
    scr.nodelay(False)
    return scr.getch()


def get_input(scr, prompt_text: str, initial: str = "") -> str | None:
    """Single-line text input at the bottom row. Returns None on Esc/cancel."""
    h, w = scr.getmaxyx()
    curses.echo()
    curses.curs_set(1)
    scr.nodelay(False)
    buf = list(initial)

    while True:
        display = (prompt_text + "".join(buf))[:w - 1]
        try:
            scr.move(h - 1, 0)
            scr.clrtoeol()
            scr.addstr(h - 1, 0, display, curses.A_REVERSE)
        except curses.error:
            pass
        scr.refresh()

        key = scr.getch()
        if key in (curses.KEY_ENTER, ord('\n'), ord('\r')):
            break
        elif key == 27:  # Esc
            curses.noecho()
            curses.curs_set(0)
            scr.nodelay(True)
            return None
        elif key in (curses.KEY_BACKSPACE, 127, ord('\b')):
            if buf:
                buf.pop()
        elif 32 <= key < 127:
            buf.append(chr(key))

    curses.noecho()
    curses.curs_set(0)
    scr.nodelay(True)
    return "".join(buf)


def draw(scr, lines: list[str], stamps: list[float | None],
         cur: int, pos: float, artist: str, title: str, help_text: str) -> None:
    scr.erase()
    h, w = scr.getmaxyx()

    clock  = f" {fmt_clock(pos)} "
    header = f" {artist} — {title}"
    header = header[:w - len(clock) - 1].ljust(w - len(clock) - 1)
    try:
        scr.addstr(0, 0, header + clock, curses.A_REVERSE)
    except curses.error:
        pass

    try:
        scr.addstr(1, 0, "─" * (w - 1))
    except curses.error:
        pass

    try:
        scr.addstr(h - 1, 0, help_text[:w - 1], curses.A_REVERSE)
    except curses.error:
        pass

    area_top = 2
    area_h   = h - 1 - area_top
    if area_h <= 0:
        scr.refresh()
        return

    center = area_h // 2
    for row in range(area_h):
        idx = cur - center + row
        if idx < 0 or idx >= len(lines):
            continue

        ts     = stamps[idx]
        prefix = (fmt_lrc(ts) + " ") if ts is not None else (" " * 12)
        text   = (prefix + lines[idx])[:w - 1]

        if idx == cur:
            attr = curses.A_REVERSE | curses.A_BOLD
        elif idx < cur:
            attr = curses.A_DIM
        else:
            attr = curses.A_NORMAL

        try:
            scr.addstr(area_top + row, 0, text, attr)
        except curses.error:
            pass

    scr.refresh()


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def run(scr) -> None:
    curses.curs_set(0)
    curses.start_color()
    curses.use_default_colors()
    scr.timeout(80)

    song_file = os.environ.get("FILE", "")
    artist    = os.environ.get("ARTIST", "")
    title     = os.environ.get("TITLE", "")
    album     = os.environ.get("ALBUM", "")
    duration  = os.environ.get("DURATION", "")

    if not (song_file and artist and title and album and duration):
        info      = mpd_currentsong()
        song_file = song_file or info.get("file", "")
        artist    = artist    or info.get("Artist", "Unknown")
        title     = title     or info.get("Title",  "Unknown")
        album     = album     or info.get("Album", "")
        duration  = duration  or info.get("duration", "")

    if not song_file:
        prompt(scr, "No song playing.", "Press any key to exit.")
        return

    stem     = Path(song_file).with_suffix("")
    txt_path = LYRICS_DIR / f"{stem}.txt"
    lrc_path = LYRICS_DIR / f"{stem}.lrc"

    has_txt = txt_path.exists()
    has_lrc = lrc_path.exists()

    # ── mode selection ─────────────────────────────────────────────────────
    if has_lrc and has_txt:
        key = prompt(scr,
                     "Lyrics found. Choose mode:",
                     "",
                     "  [e]  Edit existing .lrc",
                     "  [n]  Stamp new from .txt  (overwrites .lrc)",
                     "  [q]  Cancel")
        if key == ord('e'):
            edit_mode = True
        elif key == ord('n'):
            edit_mode = False
        else:
            return
    elif has_lrc:
        edit_mode = True
    elif has_txt:
        edit_mode = False
    else:
        prompt(scr,
               "No lyrics file found:",
               f"  expected: {txt_path.name}  or  {lrc_path.name}",
               "",
               "Run fetch-lyrics or check the lyrics directory.",
               "Press any key to exit.")
        return

    scr.nodelay(True)

    # ── load ───────────────────────────────────────────────────────────────
    if edit_mode:
        lines, stamps = load_lrc(lrc_path)
        help_text = HELP_EDIT
    else:
        lines  = load_txt(txt_path)
        stamps = [None] * len(lines)
        help_text = HELP_STAMP

    if not lines:
        prompt(scr, "Lyrics file is empty.", "Press any key to exit.")
        return

    cur        = 0
    pos        = 0.0
    tick       = 0
    fetch_skip = 0  # tracks which lrclib result is currently loaded

    # ── event loop ─────────────────────────────────────────────────────────
    while True:
        tick += 1
        if tick % 4 == 0:
            pos = mpd_elapsed()

        draw(scr, lines, stamps, cur, pos, artist, title, help_text)

        key = scr.getch()

        if key in (ord(' '), ord('\n'), curses.KEY_ENTER):
            stamps[cur] = mpd_elapsed()
            if cur < len(lines) - 1:
                cur += 1
            elif not edit_mode:
                save_lrc(lrc_path, lines, stamps)
                break

        elif key in (ord('j'), curses.KEY_DOWN):
            if cur < len(lines) - 1:
                cur += 1

        elif key in (ord('k'), curses.KEY_UP):
            if cur > 0:
                cur -= 1

        elif key in (ord('b'), curses.KEY_LEFT):
            if cur > 0:
                cur -= 1
                stamps[cur] = None

        elif key == ord('s'):
            if cur < len(lines) - 1:
                cur += 1

        elif key == ord('e'):
            ts = stamps[cur]
            initial = ""
            if ts is not None:
                m = int(ts) // 60
                initial = f"{m:02d}:{ts - m * 60:05.2f}"
            val = get_input(scr, f"Timestamp for line {cur + 1} (mm:ss.cs): ", initial)
            if val is not None:
                parsed = parse_ts(val)
                if parsed is not None:
                    stamps[cur] = parsed

        elif key == ord('o'):
            val = get_input(scr, "Shift all timestamps by seconds (e.g. +1.5 or -0.5): ")
            if val is not None:
                try:
                    stamps = apply_offset(stamps, float(val.strip()))
                except ValueError:
                    pass

        elif key in (ord('d'), ord('D')) and edit_mode:
            going_next = key == ord('d')
            next_skip  = fetch_skip + 1 if going_next else fetch_skip - 1

            if next_skip < 0:
                prompt(scr, "Already at the first result.", "", "Press any key to continue.")
                scr.nodelay(True)
            else:
                direction = "next" if going_next else "previous"
                ch = prompt(scr,
                            f"Discard and fetch {direction} result from lrclib.net?",
                            "",
                            f"  result #{next_skip + 1}  ({lrc_path.name})",
                            "",
                            "[y] Yes    [any other key] Cancel")
                if ch == ord('y'):
                    scr.erase()
                    try:
                        scr.addstr(0, 0, f"Fetching result #{next_skip + 1} from lrclib.net…")
                    except curses.error:
                        pass
                    scr.refresh()

                    found = refetch_lyrics(lrc_path, song_file, artist, title,
                                          album, duration, next_skip)
                    if found:
                        fetch_skip = next_skip
                        lines, stamps = load_lrc(lrc_path)
                        cur = 0
                    else:
                        prompt(scr,
                               f"No result #{next_skip + 1} found on lrclib.net.",
                               "", "Press any key to continue.")
                scr.nodelay(True)

        elif key == ord('r') and not edit_mode:
            cur    = 0
            stamps = [None] * len(lines)
            mpd_seek_start()

        elif key in (ord('q'), 27):
            save_lrc(lrc_path, lines, stamps)
            break

    prompt(scr, f"Saved: {lrc_path}", "", "Press any key to exit.")


def main() -> None:
    curses.wrapper(run)


if __name__ == "__main__":
    main()
