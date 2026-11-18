#!/usr/bin/env python3
"""Measure the chrome in a screenshot and in the reference, and say what differs.

A visual score is an opinion; a divider at x=232 in one picture and x=200 in the
other is a fact. Everything here is measured from pixels, so a critic that says
"the column is too narrow" can be checked before anyone changes a number.

    tools/compare.py council/round-22/01-default.png
    tools/compare.py --all council/round-22

Landmarks, in the order they are reported:
  ground         the window's own background
  column         where the tab column's hairline divider sits
  lights         the left edge and the width of the traffic lights
  band           the height of the top band, from the top to the first row
  first row      the top of the first tab row
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REFERENCE = ROOT / "reference" / "reference-window.png"


def raw(path, crop, fmt="rgb24"):
    """One crop of a picture, as bytes."""
    out = subprocess.run(
        [
            "ffmpeg", "-loglevel", "error", "-i", str(path),
            "-vf", f"crop={crop}", "-frames:v", "1",
            "-f", "rawvideo", "-pix_fmt", fmt, "-",
        ],
        capture_output=True,
    )
    return out.stdout


def size(path):
    text = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-show_entries", "stream=width,height", "-of", "csv=s=x:p=0", str(path)],
        capture_output=True, text=True,
    ).stdout.strip()
    width, height = text.split("x")
    return int(width), int(height)


def row_runs(path, y, x0, width, threshold=250):
    """Where a horizontal band of pixels stops being the background."""
    data = raw(path, f"{width}:1:{x0}:{y}")
    return [
        (x0 + i, data[i * 3], data[i * 3 + 1], data[i * 3 + 2])
        for i in range(width)
        if min(data[i * 3], data[i * 3 + 1], data[i * 3 + 2]) < threshold
    ]


def lights(path):
    """The traffic lights: red-ish pixels in the top-left corner."""
    hits = []
    for y in range(0, 60, 2):
        data = raw(path, f"120:1:0:{y}")
        for x in range(120):
            r, g, b = data[x * 3], data[x * 3 + 1], data[x * 3 + 2]
            if r > 170 and g < 170 and b < 170:
                hits.append((x, y))
    if not hits:
        return None
    xs = [x for x, _ in hits]
    ys = [y for _, y in hits]
    return {
        "left": min(xs),
        "top": min(ys),
        "right": max(xs),
        "bottom": max(ys),
        "width": max(xs) - min(xs) + 1,
        "height": max(ys) - min(ys) + 1,
    }


def column(path):
    """The hairline between the tab column and the page."""
    width, height = size(path)
    y = height // 2
    for x, r, g, b in row_runs(path, y, 120, 320, threshold=248):
        # A hairline is darker than both its neighbours and one or two pixels
        # wide; the leftmost such run past the lights is the divider.
        return x
    return None


def first_text_row(path, x0=None):
    """The topmost row of the page that has ink in it."""
    width, height = size(path)
    left = x0 if x0 is not None else 260
    span = width - left - 20
    data = raw(path, f"{span}:{height}:{left}:0", fmt="gray")
    for y in range(height):
        line = data[y * span:(y + 1) * span]
        if line and min(line) < 120:
            return y
    return None


def measure(path):
    width, height = size(path)
    ground = raw(path, f"1:1:{width - 8}:{height - 8}")
    return {
        "picture": f"{width}x{height}",
        "ground": "#%02x%02x%02x" % (ground[0], ground[1], ground[2]),
        "column": column(path),
        "lights": lights(path),
        "first ink": first_text_row(path),
    }


def compare(shot, reference=None):
    mine = measure(shot)
    if reference is None:
        return {"shot": mine}
    theirs = measure(reference)
    delta = {}
    for key in ("column", "first ink"):
        if mine[key] is not None and theirs[key] is not None:
            delta[key] = mine[key] - theirs[key]
    if mine["lights"] and theirs["lights"]:
        delta["lights left"] = mine["lights"]["left"] - theirs["lights"]["left"]
        delta["lights top"] = mine["lights"]["top"] - theirs["lights"]["top"]
        delta["lights width"] = mine["lights"]["width"] - theirs["lights"]["width"]
    return {"shot": mine, "reference": theirs, "difference": delta}


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    if argv[1] == "--all":
        out = Path(argv[2])
        for shot in sorted(out.glob("*.png")):
            print(shot.name, json.dumps(compare(shot)["shot"]))
        return 0
    shot = Path(argv[1])
    if not shot.is_absolute():
        shot = (Path.cwd() / shot).resolve()
    if not shot.exists():
        print(f"no such picture: {shot}")
        return 2
    reference = Path(argv[2]) if len(argv) > 2 else (REFERENCE if REFERENCE.exists() else None)
    result = compare(shot, reference)
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
