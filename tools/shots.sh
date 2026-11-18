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
SIZE="${SHOT_SIZE:-1740x1012}"
PORT="${FIXTURE_PORT:-8099}"
BIN="$ROOT/target/release/search"

mkdir -p "$OUT"
# A round's evidence is one run, not an accumulation of three: start clean.
rm -f "$OUT"/*.png "$OUT"/manifest.json
[[ -x "$BIN" ]] || { echo "build first: cargo build --release" >&2; exit 2; }

# Fixtures, unless one is already answering.
if ! python3 - "$PORT" <<'PY'
import socket, sys
try:
    socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=0.3).close()
except OSError:
    sys.exit(1)
PY
then
  python3 "$ROOT/tools/fixtures.py" "$PORT" >/tmp/search-fixtures.log 2>&1 &
  FIXTURES=$!
  trap 'kill $FIXTURES 2>/dev/null || true' EXIT
  sleep 1
fi

# A browser of our own, with a fresh profile so states are reproducible.
export XDG_CONFIG_HOME="$(mktemp -d)"
export XDG_DATA_HOME="$(mktemp -d)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
unset DISPLAY
export GDK_BACKEND=wayland
export SEARCH_BENCH=1

APPEARANCE="${1:-}"
if [[ "${SHOT_DARK:-0}" == "1" ]]; then
  APPEARANCE=dark
else
  APPEARANCE=light
fi

# A socket left by a killed browser still answers connect() and then never
# replies, which reads as a browser that loaded nothing. Start from none.
rm -f "${XDG_RUNTIME_DIR:-/tmp}/search-bench.sock"
"$BIN" --fresh --appearance "$APPEARANCE" --size "$SIZE" >/tmp/search-shots.log 2>&1 &
APP=$!
trap 'kill $APP 2>/dev/null || true; [[ -n "${FIXTURES:-}" ]] && kill $FIXTURES 2>/dev/null || true' EXIT

python3 - "$ROOT" "$OUT" "$PORT" <<'PY'
import json, os, subprocess, sys, time, pathlib

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
    raise SystemExit("the browser never opened its bench socket")

bench = wait_bench()
wait_bench.ping = bench.must("ping")

manifest = []

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
    # States that are *about* something being open must not be settled first:
    # settling puts away the very panel or field the shot is evidence for.
    if not name.startswith(("04", "05", "06", "07")):
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

# 1. the quiet default: one page, tabs down the side
open_page("/office")
time.sleep(0.6)
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

# 5. the panels
for kind in ("settings", "history", "bookmarks", "downloads"):
    bench.must(f"key {kind}")
    time.sleep(0.4)
    shot(f"05-panel-{kind}", kind)
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

# 8. empty tab. One tab, so the picture shows the empty state and not the
# leftovers of every state before it.
bench.must("closeall")
time.sleep(0.4)
bench.must("key newtab")
time.sleep(0.5)
shot("08-empty", "an empty tab")
bench.must("escape")

# 9. a page that fails
bench.must("closeall")
time.sleep(0.4)
open_page("/missing")
time.sleep(0.6)
shot("09-missing", "a 404")

(out / "manifest.json").write_text(json.dumps(manifest, indent=2))
bad = [item for item in manifest if not item["ok"]]
print(f"\n{len(manifest) - len(bad)}/{len(manifest)} states captured in {out}")
sys.exit(1 if bad else 0)
PY
