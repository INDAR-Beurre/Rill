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
unset DISPLAY
export GDK_BACKEND=wayland
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

def real_shot(path):
    """A picture of the window as the desktop draws it, when there is a desktop."""
    bench.command("activate")
    time.sleep(0.25)
    result = subprocess.run(
        ["spectacle", "-b", "-n", "-a", "-o", str(path)],
        capture_output=True,
    )
    if result.returncode != 0 or not path.exists():
        return False
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

try:
    # Waiting for the socket is inside the try so a browser that never came up
    # still leaves a manifest and still goes through the cleanup trap.
    bench = wait_bench()
    bench.must("ping")

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

    # 2. five tabs, four of them pinned
    tabs = []
    for title, body in [("X", ""), ("Notion", ""), ("Figma", ""), ("Search", "")]:
        tabs.append(open_page(f"/titled?t={title.replace(' ', '+')}"))
    for tab in tabs:
        bench.select(tab)
        bench.must("key pin")
    open_page("/office")
    time.sleep(0.6)
    shot("02-pins", "four pins and a live row")

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
    time.sleep(0.4)
    shot("04-omni", "the field with suggestions")
    bench.must("escape")

    # 4b. the field with a row picked, the way Up and Down leave it
    bench.must("key omni")
    time.sleep(0.3)
    bench.must("key down")
    bench.must("key down")
    time.sleep(0.3)
    shot("04b-omni-picked", "a suggestion picked by keyboard")
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
    bench.must("escape")

    # 7. reading mode on a long page
    open_page("/long")
    settle()
    time.sleep(0.8)
    bench.must("key reader")
    time.sleep(1.2)
    shot("07-reading", "reading mode")
    bench.must("key reload")

    # 8. empty tab. closeall leaves exactly one fresh tab behind (closing the last
    # tab makes a new one), so asking for another here would put two "New tab"
    # rows in a picture that is meant to show one.
    bench.must("closeall")
    time.sleep(0.6)
    shot("08-empty", "an empty tab")
    bench.must("escape")

    # 9. a page that fails
    bench.must("closeall")
    time.sleep(0.4)
    open_page("/missing")
    time.sleep(0.6)
    shot("09-missing", "a 404")

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
tried = [item for item in manifest if not item.get("skipped")]
skipped = f", {len(unavailable)} state(s) with no way to open" if unavailable else ""
print(f"\n{len(tried) - len(bad)}/{len(tried)} states captured in {out}{skipped}")

# Pixel checks pin the compositor regressions the functional suite could not
# see: a light corner in dark mode and a white veil in light mode. Use the
# installed ImageMagick CLI rather than adding a Python imaging dependency.
def pixels(path, points):
    result = subprocess.run(
        ["magick", str(path), "-format", " ".join(f"%[pixel:p{{{x},{y}}}]" for x, y in points), "info:"],
        capture_output=True, text=True)
    if result.returncode:
        return []
    return [[int(float(v)) for v in re.findall(r"[\d.]+", part)[:3]] for part in result.stdout.split("srgb")[1:]]

if (out / "01-default.png").exists():
    dark = bool(os.environ.get("SHOT_DARK"))
    corner = pixels(out / "01-default.png", [(4, 4), (100, 4)])
    if dark:
        ok = bool(corner) and all(max(c) <= 25 for c in corner)
    else:
        ok = bool(corner) and all(min(c) >= 245 for c in corner)
    manifest.append({"name": "pixel dark corner", "ok": ok, "note": str(corner)})
    print(("  ok   " if ok else "  FAIL ") + "pixel dark corner — " + str(corner))
if not os.environ.get("SHOT_DARK") and (out / "05-panel-history.png").exists():
    veil = pixels(out / "05-panel-history.png", [(500, 500), (100, 200)])
    ok = bool(veil) and 120 <= sum(veil[0]) / 3 <= 220 and min(veil[1]) >= 245
    manifest.append({"name": "pixel light scrim", "ok": ok, "note": str(veil)})
    print(("  ok   " if ok else "  FAIL ") + "pixel light scrim — " + str(veil))

# One pixel column of a shot, top to bottom, as (r, g, b) tuples.
def column(path, x):
    result = subprocess.run(
        ["magick", str(path), "-crop", f"1x10000+{x}+0", "+repage", "-depth", "8", "rgb:-"],
        capture_output=True)
    if result.returncode:
        return []
    raw = result.stdout
    return [tuple(raw[i:i + 3]) for i in range(0, len(raw) - 2, 3)]

if (out / "05-panel-settings.png").exists():
    # The card's height is not fixed: a scrolling panel is shortened so it ends
    # on a whole row (snap_to_row), so its bottom edge moves with the content.
    # The old gate read a fixed (1199, 767), which landed on the backdrop once
    # Settings ended at y≈756 (round 40). Find the card's real bottom edge
    # instead: the first row below the thumb where the scrollbar column matches
    # the backdrop beside the card (x=400, same row). Then read the track a
    # little above that edge, inside the card.
    path = out / "05-panel-settings.png"
    dark = os.environ.get("SHOT_DARK")
    track_col, side_col = column(path, 1199), column(path, 400)
    rows = min(len(track_col), len(side_col))
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
    tolerance = 3 if dark else 12
    bottom = next((y for y in range(301, rows)
                   if max(abs(a - b) for a, b in zip(track_col[y], side_col[y])) <= tolerance), None)
    probe_y = bottom - 12 if bottom and bottom - 12 > 300 else None
    thumb = pixels(path, [(1199, 300), (1199, probe_y)]) if probe_y else []
    ok = bool(thumb) and ((max(thumb[0]) < 230 and min(thumb[1]) > 225) if not dark else (40 <= sum(thumb[0]) / 3 <= 110 and sum(thumb[1]) / 3 < 35))
    note = f"{thumb} (card bottom y={bottom}, track read at y={probe_y})"
    manifest.append({"name": "pixel panel scrollbar", "ok": ok, "note": note})
    print(("  ok   " if ok else "  FAIL ") + "pixel panel scrollbar — " + note)

bad = [item for item in manifest if not item["ok"] and not item.get("skipped")]
# though it is not a failed picture, so the run does not report success. A
# round that stopped early is a failure too, whatever it managed to capture.
sys.exit(1 if bad or unavailable or stopped else 0)
PY
SHOTS=$!
# The driver's own exit status is the round's: a failed shot or a state with no
# way to open it ends non-zero, and the trap above still reaps the browser.
wait "$SHOTS"
