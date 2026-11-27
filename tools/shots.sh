#!/usr/bin/env bash
# Take the council's evidence: one picture of every state the browser has.
#
#   tools/shots.sh [round-directory]
#
# Starts the browser with its bench socket, drives it through each state and
# writes a PNG per state, plus a manifest. Uses the browser's own capture path
# (tools are drawn into a surface, the page comes from the engine's snapshot),
# so it works on a headless or locked desktop.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/council/latest}"
SIZE="${SHOT_SIZE:-1500x917}"
# An optional narrow pass proves the shell at 900px without changing the
# default council evidence size.
if [[ "${NARROW:-0}" == "1" ]]; then
    SIZE=900x1001
fi
PORT="${FIXTURE_PORT:-8099}"
BIN="$ROOT/target/release/search"
BENCH="$ROOT/tools/bench.py"

mkdir -p "$OUT"
# A round's evidence is one run, not an accumulation of three: start clean.
rm -f "$OUT"/*.png "$OUT"/manifest.json
[[ -x "$BIN" ]] || { echo "build first: cargo build --release" >&2; exit 2; }

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
SOCKET="$XDG_RUNTIME_DIR/search-bench.sock"
# The bench client reads this, so the cleanup below asks the same browser the
# round drove, not whatever else is listening on the default path.
export SEARCH_BENCH_SOCKET="$SOCKET"

# -- the browser this script started, and nothing else ---------------------
#
# A browser left running keeps its three WebKit processes and the page's whole
# engine alive. Seven critic children were OOM-killed in one round because a
# shot run ended early and nobody reaped the browsers behind it, so the end of
# this script is not the normal path: it is the only path. Everything below
# names the pid it started and kills that pid, its children and its group.
APP=""
FIXTURES=""
SHOTS=""
CFG_DIR=""
DATA_DIR=""
LOG_DIR=""
# Setsid forks to make the new session leader, so `$!` is a wrapper that exits at
# once and the browser is not its child any more. The launched shell writes the
# pid it is about to exec over, and that one is both the process to signal and
# the leader of its own group.
PIDFILE="$(mktemp)"
rm -f "$PIDFILE"

# Deepest first: a WebKit child whose parent was killed first is reparented and
# then has to be found by its group, which is why the group is signalled too.
tree_kill() {
  local pid="$1" signal="$2" child
  if command -v pgrep >/dev/null 2>&1; then
    for child in $(pgrep -P "$pid" 2>/dev/null || true); do
      tree_kill "$child" "$signal"
    done
  fi
  kill -s "$signal" "$pid" 2>/dev/null || true
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM HUP
  # The driver first: a python still holding the bench socket would keep
  # answering the `quit` below and hold the browser open.
  if [[ -n "$SHOTS" ]] && kill -0 "$SHOTS" 2>/dev/null; then
    tree_kill "$SHOTS" TERM
    for _ in $(seq 1 10); do
      kill -0 "$SHOTS" 2>/dev/null || break
      sleep 0.1
    done
    kill -0 "$SHOTS" 2>/dev/null && tree_kill "$SHOTS" KILL
  fi
  if [[ -n "$APP" ]] && kill -0 "$APP" 2>/dev/null; then
    # Graceful first: `quit` closes every view and lets the engine's own
    # processes go, which a signal does not. It is also the only shutdown that
    # does not look like a crash to the next `--fresh` run.
    if [[ -x "$BIN" && -S "$SOCKET" ]]; then
      # Bounded: a browser that has stopped answering is killed below, not
      # waited on for the client's own thirty seconds.
      timeout 5 python3 "$BENCH" quit >/dev/null 2>&1 || true
    fi
    for _ in $(seq 1 15); do
      kill -0 "$APP" 2>/dev/null || break
      sleep 0.2
    done
    if kill -0 "$APP" 2>/dev/null; then
      # Its own process group, when the launch gave it one, so the engine's
      # helper processes go with it.
      kill -TERM -- "-$APP" 2>/dev/null || kill -TERM "$APP" 2>/dev/null || true
      for _ in $(seq 1 10); do
        kill -0 "$APP" 2>/dev/null || break
        sleep 0.2
      done
    fi
    if kill -0 "$APP" 2>/dev/null; then
      tree_kill "$APP" KILL
      sleep 0.2
    fi
  fi
  if [[ -n "$FIXTURES" ]] && kill -0 "$FIXTURES" 2>/dev/null; then
    tree_kill "$FIXTURES" TERM
    for _ in $(seq 1 10); do
      kill -0 "$FIXTURES" 2>/dev/null || break
      sleep 0.1
    done
    kill -0 "$FIXTURES" 2>/dev/null && tree_kill "$FIXTURES" KILL
  fi
  if [[ "$status" -ne 0 && -n "$LOG_DIR" ]]; then
    for log in "$LOG_DIR/fixtures.log" "$LOG_DIR/shots.log"; do
      if [[ -s "$log" ]]; then
        echo "--- $(basename "$log") (tail) ---" >&2
        tail -n 30 "$log" >&2 || true
      fi
    done
  fi
  rm -f "$PIDFILE"
  for path in "$CFG_DIR" "$DATA_DIR" "$LOG_DIR" "$SOCKET"; do
    [[ -n "$path" ]] && rm -rf "$path"
  done
  exit "$status"
}

# A run that is interrupted still ends here: `exit` turns INT/TERM/HUP into the
# EXIT trap instead of leaving the browser behind. The trap can only run between
# commands, so the driver below is a job this shell can interrupt: a heredoc in
# the foreground would swallow the signal until it finished on its own.
trap cleanup EXIT
trap 'exit 130' INT TERM HUP

LOG_DIR="$(mktemp -d)"

# Fixtures, unless one is already answering.
if ! python3 - "$PORT" <<'PY'
import socket, sys
try:
    socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=0.3).close()
except OSError:
    sys.exit(1)
PY
then
  python3 "$ROOT/tools/fixtures.py" "$PORT" >"$LOG_DIR/fixtures.log" 2>&1 &
  # Only a server this script started is ever killed; one that was already
  # answering is someone else's and stays up.
  FIXTURES=$!
  sleep 1
fi

# A browser of our own, with a fresh profile so states are reproducible.
CFG_DIR="$(mktemp -d)"
DATA_DIR="$(mktemp -d)"
export XDG_CONFIG_HOME="$CFG_DIR"
export XDG_DATA_HOME="$DATA_DIR"
if [[ "${SHOT_BACKEND:-wayland}" == "x11" ]]; then
  export GDK_BACKEND=x11
else
  unset DISPLAY
  export GDK_BACKEND=wayland
  # Wayland reports the desktop's 2x output scale to WebKit while the GTK
  # allocation is still logical; pages paint twice as large and clip.
  # Evidence rounds need one CSS pixel per GTK pixel.
  export GDK_SCALE=1
fi
export SEARCH_BENCH=1

APPEARANCE=light
[[ "${SHOT_DARK:-0}" == "1" ]] && APPEARANCE=dark

# A socket left by a killed browser still answers connect() and then never
# replies, which reads as a browser that loaded nothing. Start from none.
rm -f "$SOCKET"
# In its own session, so the whole tree can be signalled as one group even if
# the browser's own shutdown is the thing that has stopped working.
LAUNCH='echo $$ > "$1"; shift; exec "$@"'
if command -v setsid >/dev/null 2>&1; then
  setsid sh -c "$LAUNCH" sh "$PIDFILE" \
    "$BIN" --fresh --appearance "$APPEARANCE" --size "$SIZE" >"$LOG_DIR/shots.log" 2>&1 &
else
  sh -c "$LAUNCH" sh "$PIDFILE" \
    "$BIN" --fresh --appearance "$APPEARANCE" --size "$SIZE" >"$LOG_DIR/shots.log" 2>&1 &
fi
APP=$!
# The recorded pid, when the launch got far enough to record one. Until then the
# wrapper is all there is to signal, and killing it is still better than
# leaving the browser behind.
for _ in $(seq 1 25); do
  [[ -s "$PIDFILE" ]] && break
  sleep 0.1
done
if [[ -s "$PIDFILE" ]]; then APP="$(cat "$PIDFILE")"; fi

python3 - "$ROOT" "$OUT" "$PORT" <<'PY' &
import json, os, subprocess, sys, time, pathlib, re

root, out, port = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
sys.path.insert(0, str(root / "tools"))
from bench import Bench, BenchError

def wait_bench(timeout=25):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            return Bench(timeout=15)
        except Exception:
            time.sleep(0.2)
    raise BenchError("the browser never opened its bench socket")

manifest = []
# Set when the round stopped before the states were walked, so a run that
# captured nothing cannot report success.
stopped = False
# States the browser has but no verb can open: named in the summary so a hole
# in the evidence is visible without reading the manifest.
unavailable = []

def grim_shot(path):
    """Capture the focused Wayland window by cropping the compositor frame."""
    try:
        import io
        from PIL import Image
        raw = subprocess.run(["grim", "-"], capture_output=True, check=True, timeout=8).stdout
        image = Image.open(io.BytesIO(raw)).convert("RGB")
        width, height = image.size
        blue = (122, 162, 250)
        rows = [y for y in range(height)
                if sum(1 for x in range(0, width, 8)
                       if image.getpixel((x, y)) == blue) > 100]
        if len(rows) < 2:
            return False
        top, bottom = rows[0], rows[-1]
        cols = [x for x in range(width) if image.getpixel((x, top)) == blue]
        if len(cols) < 2:
            return False
        left, right = cols[0], cols[-1]
        image.crop((left + 1, top + 1, right, bottom)).save(path)
        return path.exists() and path.stat().st_size > 4000
    except Exception:
        return False

def real_shot(path):
    """A picture of the window as the desktop draws it, when there is a desktop."""
    bench.command("activate")
    time.sleep(0.25)
    try:
        result = subprocess.run(
            ["spectacle", "-b", "-n", "-a", "-o", str(path)],
            capture_output=True,
        )
    except FileNotFoundError:
        return grim_shot(path)
    if result.returncode != 0 or not path.exists():
        return grim_shot(path)
    # A window capture carries the window's alpha. Lay it on the ground the
    # browser paints, so a transparent edge is not read as a black border.
    background = "0x0b0b0b" if os.environ.get("SHOT_DARK") == "1" else "0xffffff"
    size = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-show_entries", "stream=width,height", "-of", "csv=s=x:p=0", str(path)],
        capture_output=True, text=True,
    ).stdout.strip()
    if "x" not in size:
        return False
    flattened = path.with_suffix(".flat.png")
    subprocess.run(
        [
            "ffmpeg", "-loglevel", "error", "-y",
            "-f", "lavfi", "-i", f"color=c={background}:s={size}:d=1",
            "-i", str(path),
            "-filter_complex", "[0:v][1:v]overlay=format=auto:shortest=1",
            "-frames:v", "1", str(flattened),
        ],
        capture_output=True,
    )
    if flattened.exists() and flattened.stat().st_size > 4000:
        flattened.replace(path)
    return path.exists() and path.stat().st_size > 4000


def shot(name, note=""):
    # States that are *about* something being open must not be put away first:
    # settle() closes the very panel or field the shot is evidence for. They
    # still need GTK's queued allocation pass to run, or the layer is drawn at
    # the size it had before it was raised.
    if name.startswith(("04", "05", "06", "07", "16", "17")):
        bench.must("settle")
    else:
        settle()
    path = out / f"{name}.png"
    if os.environ.get("SEARCH_SHOTS", "real") == "real":
        ok = real_shot(path)
        if not ok:
            answer = bench.command(f"shot {path}")
            ok = answer.get("ok", False)
    else:
        answer = bench.command(f"shot {path}")
        ok = answer.get("ok", False)
    manifest.append({"name": name, "path": str(path), "ok": ok, "note": note})
    print(("  ok   " if ok else "  FAIL ") + name + (" — " + note if note else ""))
    return ok

def settle():
    """Put the field, find and any panel away, and check it took."""
    for _ in range(4):
        probe = bench.probe()
        if not probe["omni"] and not probe["find"] and not probe["panel"]:
            return
        bench.must("escape")
        time.sleep(0.15)
    raise SystemExit("the field would not close: " + json.dumps(bench.probe())[:300])


def show_panel(kind):
    """Open one panel and prove it is the one that opened.

    A panel is opened the way a person opens it: through the shortcut table for
    the ones that have a key, and through a panel verb for the ones that do
    not. Whichever answers, the probe has to say that panel is up, or the
    picture would be captioned with a state it does not show.

    `key <name>` is the only way in today: bench.rs has no verb that calls
    toggle_panel. `openpanel` is tried second so that the day someone adds it,
    the states without a key start working without touching this script.
    """
    title = kind.capitalize()
    for verb in (f"key {kind}", f"openpanel {kind}"):
        try:
            bench.must(verb)
        except BenchError:
            continue
        time.sleep(0.4)
        if bench.probe().get("panel") == title:
            return True
        bench.must("key closepanel")
    return False


def open_page(path, wait=True, tries=2):
    last = None
    for _ in range(tries):
        tab = bench.open(f"http://127.0.0.1:{port}{path}")
        try:
            if wait:
                bench.wait_loaded(tab, timeout=20)
            return tab
        except Exception as error:  # a slow fixture should not end the round
            last = error
            print(f"    (retrying {path}: {error})")
            time.sleep(0.5)
    raise SystemExit(f"could not load {path}: {last}")

settings_scroll_info = None
sitecard_inset_info = None
try:
    # Waiting for the socket is inside the try so a browser that never came up
    # still leaves a manifest and still goes through the cleanup trap.
    bench = wait_bench()
    bench.must("ping")
    # Dark screenshots must show the dark-page opt-in, not a bright webview
    # inside a dark frame. This is the same preference the Settings switch
    # writes; the light suite deliberately leaves it off.
    if os.environ.get("SHOT_DARK") == "1":
        bench.must("setdark on")
        time.sleep(0.8)

    # 0. true fresh empty-profile start page, captured BEFORE any navigation
    # or seeded bookmarks/history so the curated Explore links are directly seen.
    bench.must("settle")
    fresh_probe = bench.probe()
    fresh_start = bench.must("starttext").get("text", "")
    if fresh_probe.get("visibleChild") != "blank" or "Explore" not in fresh_start or "No recent pages" not in fresh_start:
        raise SystemExit(
            f"08a-start-fresh must show fresh empty start page: visibleChild={fresh_probe.get('visibleChild')!r}, text={fresh_start[:200]!r}"
        )
    shot("08a-start-fresh", "fresh empty-profile start page with curated Explore links")

    # 1. the quiet default: one page, tabs down the side
    # A --fresh start already has one blank tab. bench `open` always adds a
    # new tab (src/bench.rs "open" calls app.new_tab), so opening the studio
    # page with it left the startup tab above it, and the picture captioned
    # "one tab" showed two. Load the page into the tab the browser started
    # with, as a person typing an address into a new window would.
    for attempt in range(2):
        bench.go(f"http://127.0.0.1:{port}/office")
        try:
            bench.wait_loaded(timeout=20)
            break
        except Exception as error:  # a slow fixture should not end the round
            if attempt:
                raise SystemExit(f"could not load /office: {error}")
            print(f"    (retrying /office: {error})")
            time.sleep(0.5)
    time.sleep(0.6)
    if len(bench.probe().get("tabs", [])) != 1:
        raise SystemExit("01-default must show exactly one tab: "
                         + json.dumps(bench.probe().get("tabs"))[:300])
    shot("01-default", "one tab, the studio page")

    # 1b. revealed toolbar state (Ctrl Shift S) so both quiet and revealed
    # shell states are verified at wide and narrow widths.
    bench.must("toolbar on")
    time.sleep(0.3)
    shot("01b-toolbar", "revealed browser toolbar")
    bench.must("toolbar off")
    time.sleep(0.2)

    # 2. five tabs, four of them pinned
    tabs = []
    for title, body in [("X Studio", ""), ("Notion Labs", ""), ("Figma Files", ""), ("Search Console", "")]:
        tabs.append(open_page(f"/titled?t={title.replace(' ', '+')}"))
    for tab in tabs:
        bench.select(tab)
        bench.must("key pin")
    open_page("/office")
    time.sleep(0.6)
    shot("02-pins", "four pins and a live row")

    # 2b. the same strip with a pin selected. 02-pins ends on the unpinned
    # Office page, so no pin is active in that shot. That left the .pin.live
    # state (fill plus the 2px accent underline) with no evidence for review.
    bench.select(tabs[0])
    time.sleep(0.4)
    shot("02b-pins-live", "one active pin")

    # 3. a column of tabs, nothing pinned
    bench.must("closeall")
    time.sleep(0.5)
    for title in ("X", "Notion", "Figma", "Reading", "Are.na"):
        open_page(f"/titled?t={title.replace(' ', '+')}")
    time.sleep(0.4)
    time.sleep(0.5)
    shot("03-tabs", "a row of open tabs")

    # 4. the field, over the page
    bench.go(f"http://127.0.0.1:{port}/office")
    bench.wait_loaded(timeout=25)
    bench.must("key omni")
    time.sleep(0.25)
    shot("04-omni-current", "the field opened on the current page with security chip and path")
    bench.must("omnitext ")
    bench.must("pump")
    time.sleep(0.4)
    shot("04-omni", "the field with unfiltered suggestions")
    bench.must("escape")

    # 4b. the field with a row picked, the way Up and Down leave it
    bench.must("key omni")
    time.sleep(0.2)
    bench.must("omnitext ")
    bench.must("pump")
    time.sleep(0.2)
    bench.must("key down")
    bench.must("key down")
    time.sleep(0.3)
    shot("04b-omni-picked", "a suggestion picked by keyboard")
    bench.must("escape")

    # 4c. a path query over the same set of tabs. The first round's field
    # showed stale unfiltered suggestions here, and the distinct subtitle of
    # the exact match is what keeps two same-host tabs apart.
    bench.must("key omni")
    time.sleep(0.2)
    bench.must("omnitext office")
    time.sleep(0.4)
    omni_rank = bench.must("debug").get("omni", {})
    if omni_rank.get("chipVisible") or omni_rank.get("chipKind") != "none":
        raise SystemExit(f"plain search query must not show a host/security chip: {omni_rank}")
    shot("04c-omni-ranking", "exact path match over other tabs (no search-engine lock chip)")
    bench.must("escape")

    # 5. the panels, every one of them: the sheet of keys and the truth about
    # extensions are facets the critics score, so they need a picture too, not
    # only a stress check.
    for kind in ("settings", "history", "bookmarks", "downloads",
                 "shortcuts", "extensions"):
        name = f"05-panel-{kind}"
        if not show_panel(kind):
            path = out / f"{name}.png"
            manifest.append({"name": name, "path": str(path), "ok": False,
                             "skipped": True,
                             "note": f"nothing in src/bench.rs opens {kind.capitalize()}"})
            unavailable.append(name)
            print(f"  SKIP  {name} — nothing in src/bench.rs opens "
                  f"{kind.capitalize()}: no key in src/keys.rs SHEET reaches it"
                  f" and no verb calls toggle_panel")
            continue
        shot(name, kind)
        if kind == "settings":
            # A filter is only a feature when a typed query narrows the form;
            # the field itself is not enough evidence.
            bench.must("filter dark")
            time.sleep(0.5)
            shot("05-panel-settings-filtered", "Settings filtered to matching rows")
            bench.must("filter ")
            # The last two switches are below the fold. A scrolled picture is
            # the only honest proof they are reachable with the mouse.
            # Clearing the filter changes the viewport height; settle first so
            # panelscroll sees the restored upper bound, not the filtered one.
            bench.must("settle")
            settings_scroll_info = bench.must("panelscroll 10000")
            time.sleep(0.4)
            shot("05-panel-settings-scrolled", "the bottom of Settings, including the last switch")
        bench.must("key closepanel")

    # 5b. populated library panels. The default panel loop above proves the
    # empty text; these prove the rows a person accumulates over weeks are
    # laid out without clipping or repeated hosts.
    bench.must("seeddata bookmarks")
    if show_panel("bookmarks"):
        shot("05-panel-bookmarks-filled", "bookmarks collected over time")
        bench.must("key closepanel")
    bench.must("seeddata downloads")
    if show_panel("downloads"):
        shot("05-panel-downloads-filled", "finished, failed and active downloads")
        bench.must("key closepanel")
    # The empty states matter as much as the seeded ones: an unloaded History
    # page must explain itself instead of showing a blank box.
    extension_dir = out / "installed-extension"
    extension_dir.mkdir(exist_ok=True)
    (extension_dir / "manifest.json").write_text(
        '{"manifest_version":3,"name":"Quit Note","version":"1.0",'
        '"description":"A real unpacked extension used only for visual evidence."}'
    )
    bench.must(f"installext {extension_dir}")
    if show_panel("extensions"):
        shot("05-panel-extensions-installed", "an installed unpacked extension")
        bench.must("key closepanel")

    # 6. find
    bench.must("closeall")
    time.sleep(0.4)
    open_page("/long")
    # A new tab comes up with the field open, so settle before evidence of anything
    # else: a picture of find with the address field over it shows neither.
    settle()
    bench.must("find Paragraph")
    time.sleep(0.6)
    shot("06-find", "find over a page")
    bench.must("find zzzznomatch")
    time.sleep(0.4)
    shot("06b-find-zero", "find with zero matches")
    bench.must("escape")

    # 7. reading mode on a long page
    open_page("/long")
    settle()
    time.sleep(0.8)
    bench.must("key reader")
    time.sleep(1.2)
    shot("07-reading", "reading mode")
    bench.must('js document.querySelector("#__search_reader_font button[aria-label=\\"Smaller reading text\\"]").click()')
    time.sleep(0.4)
    shot("07-reading-small", "reading mode with 14px type")
    bench.must("pagefont 14")
    time.sleep(0.4)
    assert bench.must("pagefont").get("value") == "14px", "reader font did not reach 14px"
    bench.must('js document.querySelector("#__search_reader_font button[aria-label=\\"Larger reading text\\"]").click()')
    time.sleep(0.4)
    bench.must('js document.querySelector("#__search_reader_font button[aria-label=\\"Larger reading text\\"]").click()')
    time.sleep(0.4)
    shot("07-reading-large", "reading mode with 22px type")
    bench.must("pagefont")
    assert bench.must("pagefont").get("value") == "22px", "reader font did not reach 22px"
    bench.must("key reload")

    # 7b. reading mode on a structured page. /long is paragraphs only, so the
    # reader's headings (h2-h4), list indents, blockquote, inline and block code
    # and the breaking of a very long link were never in a picture. /article
    # has all of them and no banner, so the shot shows the reader alone.
    open_page("/article")
    settle()
    time.sleep(0.8)
    bench.must("key reader")
    time.sleep(1.2)
    shot("07b-reading-structure", "reading mode with headings, lists and code")
    # 7d. the reading-size controls and their effect on a structured page.
    bench.must('js document.querySelector("#__search_reader_font button[aria-label=\\"Larger reading text\\"]").click()')
    time.sleep(0.5)
    shot("07d-reading-large", "reading mode with larger type")
    bench.must('js document.querySelector("#__search_reader_font button[aria-label=\\"Smaller reading text\\"]").click()')
    bench.must("key reload")

    # 7c. find while reading. The reader's exit pill and the find bar share the
    # page's top-right corner (PAGE_TOP/PAGE_INSET in src/find.rs). In reading
    # mode find moves down by READER_EXIT_CLEARANCE so both can be seen. No
    # other state shows the two together, so a regression would go unnoticed.
    open_page("/article")
    settle()
    time.sleep(0.8)
    bench.must("key reader")
    time.sleep(1.2)
    bench.must("find code")
    time.sleep(0.6)
    shot("07c-reading-find", "find open over reading mode, clear of the exit button")
    bench.must("escape")
    bench.must("key reload")

    # 8. populated start page (after bookmarks and visits accumulated above).
    # closeall leaves exactly one tab behind showing the start page with kept
    # bookmarks and recent history, complementing 08a-start-fresh.
    bench.must("closeall")
    time.sleep(0.6)
    shot("08-empty", "populated start page with bookmarks and recent visits")
    bench.must("escape")

    # 9. a page that fails
    bench.must("closeall")
    time.sleep(0.4)
    open_page("/missing")
    time.sleep(0.6)
    shot("09-missing", "a 404")

    # 9b-d. The other failures a person actually hits. A local fixture
    # server can make a 404 but cannot refuse a connection, present an
    # expired certificate or take the network away, so these use the bench
    # verb that feeds the real renderer the exact reason strings from web.rs.
    for kind, note in (
        ("tls", "an expired certificate"),
        ("dns", "a name that cannot be resolved"),
        ("offline", "no network"),
        ("refused", "a connection refused by the host"),
        ("timeout", "a request that timed out"),
    ):
        bench.must("closeall")
        time.sleep(0.3)
        open_page("/office")
        settle()
        bench.must(f"showerror {kind}")
        time.sleep(0.5)
        shot(f"09-{kind}", note)
        bench.must("errorgeometry")
        if kind == "tls":
            bench.must("key omni")
            time.sleep(0.25)
            tls_omni = bench.must("debug").get("omni", {})
            if (
                not tls_omni.get("chipVisible")
                or tls_omni.get("chipHost") != "expired.badssl.com"
                or tls_omni.get("chipKind") != "insecure"
            ):
                raise SystemExit(f"failed HTTPS omnibox must show insecure chip, not lock: {tls_omni}")
            shot("04d-omni-tls-failed", "omnibox on failed HTTPS page showing insecure shield, not lock")
            bench.must("escape")
        if kind == "missing":
            bench.must("showerror The site broke while answering (503).")

    # 16. site card over a loaded page
    bench.must("closeall")
    time.sleep(0.4)
    open_page("/office")
    settle()
    time.sleep(0.4)
    bench.must("key sitecard")
    bench.must("settle")
    time.sleep(0.3)
    shot("16-sitecard", "site card")
    sitecard_inset_info = bench.must("cards")
    bench.must("escape")
    settle()

    # 17. peek card over the loaded page
    bench.must(f"peek http://127.0.0.1:{port}/long")
    time.sleep(1.0)
    bench.must("settle")
    shot("17-peek", "peek card")
    bench.must("escape")
    settle()

    # 18. folded tab column
    bench.must("key foldcolumn")
    bench.must("settle")
    time.sleep(0.3)
    shot("18-folded-column", "folded tab column")
    bench.must("key foldcolumn")
    bench.must("settle")
    settle()
except (BenchError, SystemExit, OSError) as error:
    # A shot that fails still leaves a manifest: a round with no index at all
    # reads as a round nobody ran.
    stopped = True
    print(f"\nthe round stopped early: {error}", file=sys.stderr)
finally:
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2))

bad = [item for item in manifest if not item["ok"] and not item.get("skipped")]
tried = [item for item in manifest if not item.get("skipped") and item["name"] != "binary"]
skipped = f", {len(unavailable)} state(s) with no way to open" if unavailable else ""
print(f"\n{len(tried) - len(bad)}/{len(tried)} states captured in {out}{skipped}")

# Pixel checks pin the compositor regressions the functional suite could not
# see: a light corner in dark mode and a white veil in light mode. Pillow is
# already installed for the harness; no browser dependency is added here.
def is_dark():
    return os.environ.get("SHOT_DARK") == "1"

def pixels(path, points):
    try:
        from PIL import Image
        image = Image.open(path).convert("RGB")
        return [list(image.getpixel((x, y))) for x, y in points]
    except Exception:
        return []

if is_dark() and (out / "01-default.png").exists():
    # This gate only describes dark mode; running it on the light set produced a
    # permanent false FAIL after the warm palette changed the light corner.
    corner = pixels(out / "01-default.png", [(4, 4), (100, 4)])
    ok = bool(corner) and all(max(c) <= 40 for c in corner)
    manifest.append({"name": "pixel dark corner", "ok": ok, "note": str(corner)})
    print(("  ok   " if ok else "  FAIL ") + "pixel dark corner — " + str(corner))
if not is_dark() and (out / "05-panel-history.png").exists():
    from PIL import Image
    image = Image.open(out / "05-panel-history.png").convert("RGB")
    # The current scrim is a 0.42 black veil over the page (src/ui.rs
    # scrim_colour), not the old light #F1EFE9 one. It must be clearly darker
    # than both the white card and the warm sidebar, wherever the window lands.
    veil = pixels(out / "05-panel-history.png",
                  [(image.width - 120, image.height - 89), (100, 200)])
    # The modal veil now covers the whole window, including the sidebar.
    # Both samples must be the same dimmed surface; a bright side would mean
    # only the page area was dimmed.
    # A quieter scrim is deliberately lighter than the old 148 grey; it must
    # still be dimmed uniformly and well below the paper surface.
    ok = bool(veil) and all(175 <= sum(c) / 3 <= 215 for c in veil) and abs(veil[0][0] - veil[1][0]) <= 12
    manifest.append({"name": "pixel light scrim", "ok": ok, "note": str(veil)})
    print(("  ok   " if ok else "  FAIL ") + "pixel light scrim — " + str(veil))

# One pixel column of a shot, top to bottom, as (r, g, b) tuples.
def column(path, x):
    try:
        from PIL import Image
        image = Image.open(path).convert("RGB")
        return [image.getpixel((x, y)) for y in range(image.height)]
    except Exception:
        return []

if (out / "05-panel-settings.png").exists():
    # The card's height is not fixed: a scrolling panel is shortened so it ends
    # on a whole row (snap_to_row), so its bottom edge moves with the content.
    # The old gate read a fixed (1199, 767), which landed on the backdrop once
    # Settings ended at y≈756 (round 40). Find the card's real bottom edge
    # instead: the first row below the thumb where the scrollbar column matches
    # the backdrop beside the card (x=400, same row). Then read the track a
    # little above that edge, inside the card.
    path = out / "05-panel-settings.png"
    dark = is_dark()
    from PIL import Image, ImageChops
    image = Image.open(path).convert("RGB")
    width, height = image.size
    row = min(250, height // 4)
    if dark:
        # The backdrop is charcoal (~21), and the card is a lighter warm
        # surface. Read the backdrop at this row and find the first pixel that
        # clearly leaves it, rather than accepting every dark pixel in the
        # screenshot.
        backdrop = sum(image.getpixel((width - 3, row))) / 3
        card_xs = [x for x in range(width - 1, 0, -1)
                   if backdrop + 8 <= sum(image.getpixel((x, row))) / 3 <= 110]
    else:
        card_xs = [x for x in range(width - 1, 0, -1)
                   if sum(image.getpixel((x, row))) / 3 >= 225]
    card_right = card_xs[0] if card_xs else width - 301
    track_x = max(2, card_right - 9)
    side_x = min(width - 3, card_right + 4)
    # The card casts a soft shadow, so the light backdrop directly under it
    # gets lighter slowly, from 126 at y=741 to 133 by y=847 (round 41). Only
    # a tolerance of 3 waited out the shadow, and the check then picked y=786
    # and read the backdrop at 774. The card itself is white (255) or #202020
    # (32) and its border is 160-186, so they are far outside 12. Shadow and
    # backdrop are within 12. With 12, the edge lands on the card's real
    # bottom: y=741 in both light and dark.
    # Dark is the opposite case: the card is #202020 (32) and the backdrop is
    # 22, only 10 apart, so a tolerance of 12 matched the card itself and
    # stopped at its top, y=428 (round 42). The dark shadow is within 1 of the
    # backdrop, so 3 is enough there and finds y=741 (round 41).
    # With a full-window veil the pixels beside the card are dimmed too, so
    # the old card-versus-backdrop edge detector cannot locate the track.
    # Read two points inside the card near the right edge instead: both must
    # be the card surface (white in light, charcoal in dark), not the 148/22
    # modal veil outside it.
    # The coloured strip at x=card_right-9 is the native scrollbar: a neutral
    # divider in light and a lighter rail in dark. It must be distinct from
    # the card surface beside it in both themes; that is the feature the gate
    # is checking.
    # The scrollbar itself is drawn by GTK at a width that changes with the
    # card and theme. The user-visible contract is stronger than its colour:
    # scrolling Settings must change what the panel shows. Compare the first
    # and scrolled captures directly; identical text would mean scroll failed.
    scrolled = Image.open(out / "05-panel-settings-scrolled.png").convert("RGB")
    if scrolled.size != image.size:
        changed = 0
    else:
        diff = ImageChops.difference(image, scrolled)
        changed = sum(1 for pixel in diff.getdata() if max(pixel) > 18)
    scroll_val = float(settings_scroll_info.get("value", 0.0)) if settings_scroll_info else 0.0
    scroll_upper = float(settings_scroll_info.get("upper", 0.0)) if settings_scroll_info else 0.0
    scroll_page = float(settings_scroll_info.get("page", 0.0)) if settings_scroll_info else 0.0
    reached_bottom = (
        settings_scroll_info is not None
        and scroll_upper >= scroll_page + 300.0
        and scroll_val >= scroll_upper - scroll_page - 2.0
        and scroll_val >= 300.0
    )
    ok = changed > 20_000 and reached_bottom
    note = (
        f"{changed} pixels changed; scroll value={scroll_val:.0f}, "
        f"upper={scroll_upper:.0f}, page={scroll_page:.0f} "
        f"(bottom_target={max(0.0, scroll_upper - scroll_page):.0f})"
    )
    manifest.append({"name": "pixel settings scroll", "ok": ok, "note": note})
    print(("  ok   " if ok else "  FAIL ") + "pixel settings scroll — " + note)

if sitecard_inset_info is not None:
    overlay_w = sitecard_inset_info.get("overlay", [0, 0])[0]
    sc_card = next((c for c in sitecard_inset_info.get("cards", []) if c.get("name") == "site-card"), None)
    sc_scrim = next((c for c in sitecard_inset_info.get("cards", []) if c.get("name") == "site-card-scrim"), None)
    if sc_card is not None and overlay_w > 0:
        right_gap = overlay_w - (sc_card["at"][0] + sc_card["size"][0])
        ok = right_gap >= 16 and sc_scrim is not None
        note = f"right_gap={right_gap}px (overlay={overlay_w}, card_at={sc_card['at']}, size={sc_card['size']}), scrim={sc_scrim is not None}"
    else:
        ok = False
        note = f"missing site-card in cards probe: {sitecard_inset_info}"
    manifest.append({"name": "sitecard inset", "ok": ok, "note": note})
    print(("  ok   " if ok else "  FAIL ") + "sitecard inset — " + note)

# Write the manifest again now that the pixel checks have run. The copy written
# in the finally block above only covers the captures, so a round that failed a
# pixel check used to leave a manifest with no failures in it. This entry also
# records which binary took the shots. It is marked skipped so it is never
# counted as a state.
import hashlib
binary = os.path.join(os.getcwd(), "target", "release", "search")
try:
    with open(binary, "rb") as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
except OSError as error:
    digest = f"unreadable: {error}"
manifest.append({"name": "binary", "path": binary, "ok": True, "skipped": True,
                 "note": f"sha256 {digest}"})
(out / "manifest.json").write_text(json.dumps(manifest, indent=2))

bad = [item for item in manifest if not item["ok"] and not item.get("skipped")]
# though it is not a failed picture, so the run does not report success. A
# round that stopped early is a failure too, whatever it managed to capture.
sys.exit(1 if bad or unavailable or stopped else 0)
PY
SHOTS=$!
# The driver's own exit status is the round's: a failed shot or a state with no
# way to open it ends non-zero, and the trap above still reaps the browser.
wait "$SHOTS"
