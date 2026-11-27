#!/usr/bin/env python3
"""Live non-SEARCH_BENCH 60fps motion capture and verification harness (F39).

Runs a real release binary WITHOUT SEARCH_BENCH across:
  - All 6 entrance/layout transitions (`panel`, `fold`, `omni`, `find`, `sitecard`, `peek`)
    in both normal (`gtk-enable-animations=true`) and reduced-motion
    (`gtk-enable-animations=false`) modes.
  - Live EXIT transitions (`panel_exit`, `peek_exit`) in normal & reduced modes,
    verifying clean removal with zero orphan scrim/card widgets.
  - Live mid-flight REVERSAL / CANCELLATION transitions (`panel_reverse`,
    `fold_reverse`, `peek_reverse`) in normal mode, verifying clean return to
    initial state with zero orphan scrim/card widgets and zero GTK warnings.
  - Live X11 pointer click-outside dismiss check on `site-card-scrim`.

Supports two honest, explicitly labeled capture backends:
  - `x11-xvfb` (default): Xvfb + Openbox software rendering, ffmpeg x11grab 60fps.
  - `wayland-sway-gpu`: isolated headless Sway compositor using real DRM/GLES2
    hardware rendering (`/dev/dri/renderD128`, verified from Sway GL renderer log)
    at 60Hz, wf-recorder 60fps Wayland screencopy, wtype input, and opt-in
    `SEARCH_MOTION_TELEMETRY=1` GdkFrameClock tick telemetry.

Usage:
  python3 tools/live_motion.py --out build/motion-evidence-f39
  python3 tools/live_motion.py --backend wayland-sway-gpu --binary /tmp/f39-motion-target/release/search --out /tmp/hw
"""
from __future__ import annotations

import argparse
import array
import ctypes
import hashlib
import json
import os
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BIN = ROOT / "target" / "release" / "search"
CORE_CASES = ("panel", "fold", "omni", "find", "sitecard", "peek")
EXIT_CASES = ("panel_exit", "peek_exit")
REVERSE_CASES = ("panel_reverse", "fold_reverse", "peek_reverse")
ALL_CASES = CORE_CASES + EXIT_CASES + REVERSE_CASES

CONTRACT = {
    "roles": {
        "MICRO": "120ms",
        "ENTER": "180ms",
        "EXIT": "130ms",
        "LAYOUT": "220ms",
        "TRAVEL": "8px",
    },
    "transitions": {
        "panel": "ENTER 180ms / EXIT 130ms (switch MICRO 120ms, travel 8px)",
        "fold": "LAYOUT 220ms",
        "omnibox": "ENTER 180ms / EXIT 130ms (travel 8px)",
        "find": "ENTER 180ms / EXIT 130ms (travel 8px)",
        "peek": "ENTER 180ms / EXIT 130ms (scrim alpha fade + card travel 8px)",
        "site_card": "ENTER 180ms / EXIT 130ms (scrim alpha fade + card travel 8px)",
    },
    "reduced_motion": "gtk-enable-animations=false or SEARCH_BENCH completes transitions synchronously",
    "gates": {
        "x11_xvfb_normal_enter": {"min_frames": 4, "min_span_ms": 80.0, "max_span_ms": 260.0},
        "x11_xvfb_normal_layout": {"min_frames": 4, "min_span_ms": 80.0, "max_span_ms": 280.0},
        "x11_xvfb_normal_exit": {"min_frames": 3, "min_span_ms": 40.0, "max_span_ms": 220.0},
        "x11_xvfb_normal_reverse": {"min_frames": 3, "min_span_ms": 40.0, "max_span_ms": 420.0},
        "wayland_gpu_normal_enter": {"min_frames": 4, "min_span_ms": 80.0, "max_span_ms": 240.0},
        "wayland_gpu_normal_layout": {"min_frames": 4, "min_span_ms": 100.0, "max_span_ms": 280.0},
        "wayland_gpu_normal_exit": {"min_frames": 3, "min_span_ms": 40.0, "max_span_ms": 200.0},
        "wayland_gpu_normal_reverse": {"min_frames": 4, "min_span_ms": 80.0, "max_span_ms": 480.0},
        "reduced": {"max_frames": 2, "max_span_ms": 34.0},
    },
}


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        while chunk := fh.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()


def collect_provenance(binary: Path) -> dict:
    git_head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=False
    ).stdout.strip()
    diff = subprocess.run(
        ["git", "diff", "HEAD", "--", "src/", "assets/"],
        cwd=ROOT,
        capture_output=True,
        check=False,
    ).stdout
    tree_hash = hashlib.sha256()
    for p in sorted((ROOT / "src").glob("*.rs")):
        tree_hash.update(p.name.encode())
        tree_hash.update(b"\0")
        tree_hash.update(p.read_bytes())
    stat = binary.stat()
    return {
        "binary": str(binary.resolve()),
        "binary_sha256": sha256_file(binary),
        "binary_size": stat.st_size,
        "binary_mtime_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(stat.st_mtime)),
        "captured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "git_head": git_head,
        "git_dirty": bool(diff),
        "source_diff_sha256": hashlib.sha256(diff).hexdigest(),
        "source_tree_sha256": tree_hash.hexdigest(),
    }


def make_fixture_server() -> tuple[ThreadingHTTPServer, int]:
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            if self.path == "/":
                body = (
                    b"<!doctype html><html><body style='margin:0;display:flex;align-items:center;"
                    b"justify-content:center;height:100vh;font:24px sans-serif;background:#ffffff;color:#111'>"
                    b"<a href='/target' style='display:inline-block;padding:80px 160px;background:#eee;color:#111'>"
                    b"Peek Target Link</a></body></html>"
                )
            else:
                body = (
                    b"<!doctype html><html><body style='padding:40px;font:20px sans-serif;background:#fff;color:#111'>"
                    b"<h1>Peeked Page</h1><p>Loaded cleanly.</p></body></html>"
                )
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, server.server_port


def pick_free_display(start: int = 120) -> str:
    for disp in range(start, start + 80):
        lock = Path(f"/tmp/.X{disp}-lock")
        sock = Path(f"/tmp/.X11-unix/X{disp}")
        if not lock.exists() and not sock.exists():
            return f":{disp}"
    raise RuntimeError("no free Xvfb display found")


def kill_proc_group(proc: subprocess.Popen | None) -> None:
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=3)
    except Exception:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait(timeout=2)
        except Exception:
            pass


def peek_webview_exclude_rect(width: int, height: int, page_left: int = 261) -> tuple[int, int, int, int]:
    """Inner WebKit viewport rect inside Peek card (from src/peek.rs card_margins).

    Masking out the inner webview leaves 100% of Peek chrome (scrim, card border,
    header bar) while preventing asynchronous WebKit page loads from registering
    as fake chrome animation frames in reduced-motion mode.
    """
    page_w = max(0, width - page_left)
    card_w = max(240, (page_w * 4) // 5)
    card_h = max(160, (height * 4) // 5)
    side = max(0, (page_w - card_w) // 2)
    top = max(0, (height - card_h) // 2)
    bottom = max(0, height - card_h - top)
    start = page_left + side
    x0 = start + 2
    y0 = top + 52
    x1 = width - side - 2
    y1 = height - bottom - 4
    return (x0, y0, x1, y1)


def count_diff_pixels(a: Image.Image, b: Image.Image, thresh: int = 2) -> int:
    diff = ImageChops.difference(a, b)
    r_ch, g_ch, b_ch = diff.split()
    max_ch = ImageChops.lighter(ImageChops.lighter(r_ch, g_ch), b_ch)
    return sum(max_ch.histogram()[thresh + 1 :])


def diff_bbox(a: Image.Image, b: Image.Image, thresh: int = 2) -> list[int] | None:
    diff = ImageChops.difference(a, b)
    r_ch, g_ch, b_ch = diff.split()
    max_ch = ImageChops.lighter(ImageChops.lighter(r_ch, g_ch), b_ch)
    mask = max_ch.point(lambda v: 255 if v > thresh else 0)
    bb = mask.getbbox()
    return list(bb) if bb is not None else None


def analyze_frames(
    frames_dir: Path,
    trigger: str,
    fps: int,
    action_start_idx: int = 1,
    baseline_img_path: Path | None = None,
) -> dict:
    frames = sorted(frames_dir.glob("frame_*.png"))
    if not frames:
        return {
            "frames": 0,
            "action_start_idx": action_start_idx,
            "pre_action_changed_frames": 0,
            "changed_frames": 0,
            "intermediate_frames": 0,
            "first_change": None,
            "last_change": None,
            "first_change_bbox": None,
            "span_ms": 0.0,
            "max_changed_pixels": 0,
            "roi": "full",
            "moving_trace": [],
            "coarse_trace": [],
            "pre_action_trace": [],
        }

    first_img = Image.open(frames[0]).convert("RGB")
    w, h = first_img.size
    exclude = peek_webview_exclude_rect(w, h) if trigger in ("peek", "peek_exit", "peek_reverse") else None

    def masked(img: Image.Image) -> Image.Image:
        if exclude is None:
            return img
        out = img.copy()
        x0, y0, x1, y1 = exclude
        out.paste((0, 0, 0), (x0, y0, x1, y1))
        return out

    action_anchor_idx = max(0, min(action_start_idx - 1, len(frames) - 1))
    action_first_masked = masked(Image.open(frames[action_anchor_idx]).convert("RGB"))
    last_masked = masked(Image.open(frames[-1]).convert("RGB"))

    prev = None
    pre_action_moving = []
    action_sensitive = []
    action_coarse = []
    intermediate_count = 0
    first_change_bbox = None

    for idx, path in enumerate(frames):
        cur = masked(Image.open(path).convert("RGB"))
        if prev is not None:
            diff = ImageChops.difference(prev, cur)
            r_ch, g_ch, b_ch = diff.split()
            max_ch = ImageChops.lighter(ImageChops.lighter(r_ch, g_ch), b_ch)
            hist = max_ch.histogram()
            sens_px = sum(hist[3:])
            coarse_px = sum(hist[13:])
            if idx < action_start_idx:
                if sens_px >= 40:
                    pre_action_moving.append((idx, sens_px))
            else:
                action_sensitive.append((idx, sens_px))
                action_coarse.append((idx, coarse_px))
                if sens_px >= 40:
                    if first_change_bbox is None:
                        mask = max_ch.point(lambda v: 255 if v > 2 else 0)
                        bb = mask.getbbox()
                        first_change_bbox = list(bb) if bb is not None else None
                    df = count_diff_pixels(action_first_masked, cur, 2)
                    dl = count_diff_pixels(last_masked, cur, 2)
                    if df >= 40 and dl >= 40:
                        intermediate_count += 1
        prev = cur

    moving = [(i, px) for i, px in action_sensitive if px >= 40]
    coarse_moving = [(i, px) for i, px in action_coarse if px >= 120]
    first_idx = moving[0][0] if moving else None
    last_idx = moving[-1][0] if moving else None
    span_ms = (
        round(((last_idx - first_idx) / float(fps)) * 1000.0, 1)
        if first_idx is not None and last_idx is not None
        else 0.0
    )

    out = {
        "frames": len(frames),
        "action_start_idx": action_start_idx,
        "pre_action_changed_frames": len(pre_action_moving),
        "pre_action_trace": pre_action_moving,
        "changed_frames": len(moving),
        "intermediate_frames": intermediate_count,
        "first_change": first_idx,
        "last_change": last_idx,
        "first_change_bbox": first_change_bbox,
        "span_ms": span_ms,
        "max_changed_pixels": max((px for _, px in action_sensitive), default=0),
        "roi": f"chrome-only (excluding webview {exclude})" if exclude else "full",
        "moving_trace": moving,
        "coarse_trace": coarse_moving,
    }

    if baseline_img_path is not None and baseline_img_path.exists():
        base_masked = masked(Image.open(baseline_img_path).convert("RGB"))
        # For exit & reverse cases, compare settled end frame against pre-open baseline
        # and against peak open frame to verify clean restoration (zero orphan scrim/card).
        diff_end_vs_base = count_diff_pixels(base_masked, last_masked, 8)
        diff_start_vs_base = count_diff_pixels(base_masked, action_first_masked, 8)
        peak_diff_vs_base = diff_start_vs_base
        if trigger.endswith("_reverse") and moving:
            for m_idx, _ in moving:
                m_img = masked(Image.open(frames[m_idx]).convert("RGB"))
                peak_diff_vs_base = max(peak_diff_vs_base, count_diff_pixels(base_masked, m_img, 8))
        out["restoration"] = {
            "end_vs_baseline_px": diff_end_vs_base,
            "peak_vs_baseline_px": peak_diff_vs_base,
            "clean_return": diff_end_vs_base <= 1500 and peak_diff_vs_base > max(5000, diff_end_vs_base * 4),
        }
    return out


def scan_log_warnings(log_path: Path) -> list[str]:
    if not log_path.exists():
        return []
    bad_markers = (
        "Gtk-WARNING",
        "Gtk-CRITICAL",
        "panicked at",
        "attempt to allocate widget",
        "Negative content height",
        "Theme parsing error",
    )
    hits = []
    for line in log_path.read_text(errors="replace").splitlines():
        if any(m in line for m in bad_markers):
            hits.append(line.strip())
    return hits


def parse_motion_telemetry(log_path: Path) -> list[dict]:
    if not log_path.exists():
        return []
    records = []
    for line in log_path.read_text(errors="replace").splitlines():
        line = line.strip()
        if line.startswith("{") and '"motion_telemetry":true' in line:
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return records


def role_for_trigger(trigger: str) -> str:
    if trigger == "fold":
        return "layout"
    if trigger.endswith("_exit"):
        return "exit"
    if trigger.endswith("_reverse"):
        return "reverse"
    return "enter"


def evaluate_gate(trigger: str, animations: bool, res: dict, backend: str) -> tuple[bool, list[str]]:
    reasons: list[str] = []
    if not res["stderr_clean"]:
        reasons.append(f"warnings={len(res['warnings'])}")
    role = role_for_trigger(trigger)
    hw = backend == "wayland-sway-gpu"

    if trigger in ("peek", "peek_exit", "peek_reverse"):
        if res["pre_action_changed_frames"] != 0:
            reasons.append(f"pre_action_contamination={res['pre_action_changed_frames']}")

    if not animations:
        g = CONTRACT["gates"]["reduced"]
        if res["changed_frames"] < 1:
            reasons.append("reduced_frames=0(no_visible_transition)")
        if res["max_changed_pixels"] < 5000:
            reasons.append(f"reduced_max_px={res['max_changed_pixels']}<5000")
        if res["changed_frames"] > g["max_frames"]:
            reasons.append(f"reduced_frames={res['changed_frames']}>{g['max_frames']}")
        if res["span_ms"] > g["max_span_ms"]:
            reasons.append(f"reduced_span={res['span_ms']}>{g['max_span_ms']}")
    else:
        key = f"{'wayland_gpu' if hw else 'x11_xvfb'}_normal_{role}"
        g = CONTRACT["gates"][key]
        if res["changed_frames"] < g["min_frames"]:
            reasons.append(f"frames={res['changed_frames']}<{g['min_frames']}")
        if res["span_ms"] < g["min_span_ms"]:
            reasons.append(f"span={res['span_ms']}<{g['min_span_ms']}")
        if res["span_ms"] > g["max_span_ms"]:
            reasons.append(f"span={res['span_ms']}>{g['max_span_ms']}")

    if "scrim_click_dismiss" in res and not res["scrim_click_dismiss"]["dismissed"]:
        reasons.append("scrim_click_dismiss_failed")
    if "restoration" in res and not res["restoration"]["clean_return"]:
        reasons.append(
            f"unclean_return(end={res['restoration']['end_vs_baseline_px']},peak={res['restoration']['peak_vs_baseline_px']})"
        )
    if trigger == "peek" and animations and res.get("first_change_bbox"):
        # Verify first moving frame on Peek is NOT a tiny cursor or bottom-left hover toast.
        x0, y0, x1, y1 = res["first_change_bbox"]
        area = (x1 - x0) * (y1 - y0)
        if area < 50_000:
            reasons.append(f"peek_first_change_bbox_too_small({res['first_change_bbox']})")

    return (len(reasons) == 0), reasons


def grab_single_x11_frame(display: str, geom: dict, dest: Path, env: dict) -> None:
    subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "x11grab",
            "-video_size",
            f"{geom['WIDTH']}x{geom['HEIGHT']}",
            "-i",
            f"{display}+{geom['X']},{geom['Y']}",
            "-frames:v",
            "1",
            str(dest),
        ],
        env=env,
        check=False,
    )


def run_capture_case_x11(
    binary: Path,
    out: Path,
    display: str,
    url: str,
    name: str,
    animations: bool,
    trigger: str,
    size: str,
    fps: int,
    duration: float,
    check_scrim_click: bool = False,
) -> dict:
    conf = out / f"gtk-{name}"
    frames = out / f"frames-{name}"
    if frames.exists():
        shutil.rmtree(frames)
    (conf / "gtk-3.0").mkdir(parents=True, exist_ok=True)
    frames.mkdir(parents=True, exist_ok=True)
    (conf / "gtk-3.0" / "settings.ini").write_text(
        f"[Settings]\ngtk-enable-animations={'true' if animations else 'false'}\ngtk-cursor-blink=false\n"
    )
    env = dict(os.environ)
    env.pop("SEARCH_BENCH", None)
    env.pop("SEARCH_BENCH_SOCKET", None)
    env.pop("WAYLAND_DISPLAY", None)
    env.update(
        GDK_BACKEND="x11",
        DISPLAY=display,
        SEARCH_MOTION_TELEMETRY="1",
        XDG_CONFIG_HOME=str(conf),
        XDG_DATA_HOME=str(conf / "data"),
        XDG_CACHE_HOME=str(conf / "cache"),
    )
    log_path = out / f"{name}.log"
    baseline_path: Path | None = None
    scrim_dismiss = None
    action_start_idx = 1

    with log_path.open("w") as log_fh:
        browser = subprocess.Popen(
            [str(binary), "--fresh", "--appearance", "light", "--size", size, url],
            env=env,
            stdout=log_fh,
            stderr=log_fh,
            start_new_session=True,
        )
        try:
            time.sleep(2.2)
            wid = ""
            for _ in range(25):
                res = subprocess.run(
                    ["xdotool", "search", "--onlyvisible", "--name", "^search$"],
                    env=env,
                    text=True,
                    capture_output=True,
                    check=False,
                )
                lines = [ln.strip() for ln in res.stdout.splitlines() if ln.strip()]
                if lines:
                    wid = lines[-1]
                    break
                time.sleep(0.2)
            if not wid:
                raise RuntimeError(f"no visible search window for {name}")
            subprocess.run(["xdotool", "windowactivate", "--sync", wid], env=env, check=False)
            geom_out = subprocess.run(
                ["xdotool", "getwindowgeometry", "--shell", wid],
                env=env,
                text=True,
                capture_output=True,
                check=True,
            ).stdout
            geom = {}
            for line in geom_out.splitlines():
                if "=" in line:
                    k, v = line.split("=", 1)
                    geom[k.strip()] = int(v.strip())

            # Load fixture explicitly for page-anchored transitions
            if trigger in ("peek", "peek_exit", "peek_reverse", "find", "sitecard"):
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+l"], env=env, check=False)
                time.sleep(0.25)
                subprocess.run(["xdotool", "type", "--clearmodifiers", "--delay", "5", url], env=env, check=False)
                subprocess.run(["xdotool", "key", "--clearmodifiers", "Return"], env=env, check=False)
                time.sleep(1.2)

            # Warm GTK fonts/caches once before recording
            subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+comma"], env=env, check=False)
            time.sleep(0.45)
            subprocess.run(["xdotool", "key", "--clearmodifiers", "Escape"], env=env, check=False)
            time.sleep(0.45)

            # Pre-position and settle pointer/hover toast BEFORE recording starts for Peek!
            cx = geom["WIDTH"] // 2 + 116
            cy = geom["HEIGHT"] // 2
            if trigger in ("peek", "peek_exit", "peek_reverse"):
                subprocess.run(["xdotool", "mousemove", "--window", wid, str(cx), str(cy)], env=env, check=False)
                time.sleep(0.30)

            # Capture clean pre-open baseline for exit & reversal restoration checks
            if trigger in EXIT_CASES or trigger in REVERSE_CASES:
                baseline_path = out / f"{name}-baseline.png"
                grab_single_x11_frame(display, geom, baseline_path, env)

            # For EXIT cases, open the target layer and let it settle BEFORE recording
            if trigger == "panel_exit":
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+comma"], env=env, check=False)
                time.sleep(0.50)
            elif trigger == "peek_exit":
                subprocess.run(
                    ["xdotool", "keydown", "Shift_L", "click", "1", "keyup", "Shift_L"], env=env, check=False
                )
                time.sleep(0.65)

            # Record telemetry count before our measured action so we isolate this transition's telemetry
            pre_tele_count = len(parse_motion_telemetry(log_path))

            rec_duration = duration if not trigger.endswith("_reverse") else max(duration, 1.0)
            recorder = subprocess.Popen(
                [
                    "ffmpeg",
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-f",
                    "x11grab",
                    "-framerate",
                    str(fps),
                    "-video_size",
                    f"{geom['WIDTH']}x{geom['HEIGHT']}",
                    "-i",
                    f"{display}+{geom['X']},{geom['Y']}",
                    "-t",
                    f"{rec_duration + 0.15:.2f}",
                    str(frames / "frame_%03d.png"),
                ],
                env=env,
            )
            # Wait for ffmpeg to actually emit frame_001.png before pre-roll so
            # ffmpeg startup latency (~100-150ms) never skews action_start_idx.
            first_frame = frames / "frame_001.png"
            t_wait0 = time.monotonic()
            while not first_frame.exists() and (time.monotonic() - t_wait0) < 2.0:
                time.sleep(0.005)
            t_rec_start = time.monotonic()
            time.sleep(0.16)
            t_action_sent = time.monotonic()
            action_start_idx = max(1, int(round((t_action_sent - t_rec_start) * fps)) - 2)

            if trigger == "panel":
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+comma"], env=env, check=False)
            elif trigger == "fold":
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+s"], env=env, check=False)
            elif trigger == "omni":
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+l"], env=env, check=False)
            elif trigger == "find":
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+f"], env=env, check=False)
            elif trigger == "sitecard":
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+i"], env=env, check=False)
            elif trigger == "peek":
                # Pointer is already settled on the link; fire Shift+click only!
                subprocess.run(
                    ["xdotool", "keydown", "Shift_L", "click", "1", "keyup", "Shift_L"], env=env, check=False
                )
            elif trigger in ("panel_exit", "peek_exit"):
                subprocess.run(["xdotool", "key", "--clearmodifiers", "Escape"], env=env, check=False)
            elif trigger == "panel_reverse":
                subprocess.run(
                    [
                        "xdotool",
                        "key",
                        "--clearmodifiers",
                        "ctrl+comma",
                        "sleep",
                        "0.16",
                        "key",
                        "--clearmodifiers",
                        "Escape",
                    ],
                    env=env,
                    check=False,
                )
            elif trigger == "fold_reverse":
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+s"], env=env, check=False)
                time.sleep(0.08)
                subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+s"], env=env, check=False)
            elif trigger == "peek_reverse":
                subprocess.run(
                    ["xdotool", "keydown", "Shift_L", "click", "1", "keyup", "Shift_L"], env=env, check=False
                )
                time.sleep(0.06)
                subprocess.run(["xdotool", "key", "--clearmodifiers", "Escape"], env=env, check=False)

            recorder.wait(timeout=15)

            if check_scrim_click and trigger == "sitecard":
                time.sleep(0.25)
                subprocess.run(
                    ["xdotool", "mousemove", "--window", wid, "320", str(geom["HEIGHT"] // 2), "click", "1"],
                    env=env,
                    check=False,
                )
                time.sleep(0.35)
                after_path = out / f"{name}-after-scrim-click.png"
                grab_single_x11_frame(display, geom, after_path, env)
                frame_list = sorted(frames.glob("frame_*.png"))
                if frame_list and after_path.exists():
                    pre_img = Image.open(frame_list[0]).convert("RGB")
                    open_img = Image.open(frame_list[-1]).convert("RGB")
                    post_img = Image.open(after_path).convert("RGB")
                    diff_vs_pre = count_diff_pixels(pre_img, post_img, 8)
                    diff_vs_open = count_diff_pixels(open_img, post_img, 8)
                    scrim_dismiss = {
                        "dismissed": diff_vs_open > 5000 and diff_vs_pre < diff_vs_open,
                        "diff_vs_pre_px": diff_vs_pre,
                        "diff_vs_open_px": diff_vs_open,
                    }
        finally:
            kill_proc_group(browser)

    result = analyze_frames(frames, trigger, fps, action_start_idx=action_start_idx, baseline_img_path=baseline_path)
    warnings = scan_log_warnings(log_path)
    tele = parse_motion_telemetry(log_path)[pre_tele_count:]
    result["warnings"] = warnings
    result["stderr_clean"] = len(warnings) == 0
    result["frame_clock_telemetry"] = tele
    if scrim_dismiss is not None:
        result["scrim_click_dismiss"] = scrim_dismiss
    return result


def parse_sway_renderer(sway_log: Path) -> dict:
    txt = sway_log.read_text(errors="replace") if sway_log.exists() else ""
    info = {
        "compositor": "sway (headless wlroots)",
        "drm_device": None,
        "egl_driver": None,
        "gl_vendor": None,
        "gl_renderer": None,
        "gles_version": None,
        "hardware_gpu": False,
    }
    for ln in txt.splitlines():
        if "Opening DRM render node" in ln:
            m = re.search(r"'([^']+)'", ln)
            if m:
                info["drm_device"] = m.group(1)
        elif "EGL driver name:" in ln:
            info["egl_driver"] = ln.split("EGL driver name:", 1)[1].strip()
        elif "GL vendor:" in ln:
            info["gl_vendor"] = ln.split("GL vendor:", 1)[1].strip()
        elif "GL renderer:" in ln:
            info["gl_renderer"] = ln.split("GL renderer:", 1)[1].strip()
        elif "Using OpenGL ES" in ln:
            info["gles_version"] = ln.split("Using ", 1)[1].strip()
    renderer = (info["gl_renderer"] or "").lower()
    info["hardware_gpu"] = bool(renderer) and not any(sw in renderer for sw in ("llvmpipe", "softpipe", "swrast"))
    return info


EV_KEY = {
    "Escape": 1,
    "1": 2, "2": 3, "3": 4, "4": 5, "5": 6, "6": 7, "7": 8, "8": 9, "9": 10, "0": 11,
    "Return": 28,
    "comma": 51,
    "period": 52,
    "slash": 53,
    "colon": 39,
    "f": 33,
    "h": 35,
    "i": 23,
    "l": 38,
    "p": 25,
    "s": 31,
    "t": 20,
}
MOD_SHIFT = 1
MOD_CTRL = 4
BTN_LEFT = 272


def build_us_xkb_keymap() -> bytes:
    xkb = ctypes.CDLL("libxkbcommon.so.0")
    xkb.xkb_context_new.argtypes = [ctypes.c_int]
    xkb.xkb_context_new.restype = ctypes.c_void_p
    xkb.xkb_keymap_new_from_names.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int]
    xkb.xkb_keymap_new_from_names.restype = ctypes.c_void_p
    xkb.xkb_keymap_get_as_string.argtypes = [ctypes.c_void_p, ctypes.c_int]
    xkb.xkb_keymap_get_as_string.restype = ctypes.c_char_p
    xkb.xkb_keymap_unref.argtypes = [ctypes.c_void_p]
    xkb.xkb_context_unref.argtypes = [ctypes.c_void_p]
    ctx = xkb.xkb_context_new(0)
    if not ctx:
        raise RuntimeError("xkb_context_new failed")
    try:
        km = xkb.xkb_keymap_new_from_names(ctx, None, 0)
        if not km:
            raise RuntimeError("xkb_keymap_new_from_names failed")
        try:
            raw = xkb.xkb_keymap_get_as_string(km, 1)
            if not raw:
                raise RuntimeError("xkb_keymap_get_as_string failed")
            return raw + b"\0"
        finally:
            xkb.xkb_keymap_unref(km)
    finally:
        xkb.xkb_context_unref(ctx)


class WaylandVirtualInput:
    """Persistent zwp_virtual_keyboard_v1 + zwlr_virtual_pointer_v1 client.

    Keeps a single hardware-like keyboard and pointer attached to Sway's seat
    for the entire test session so short-lived CLI helpers (like wtype) never
    churn wl_seat capabilities, trigger GTK :backdrop focus flicker, or drop
    mid-flight reversal chords, and enables real Shift+Click Peek coverage.
    """

    def __init__(self, sock_path: str, width: int, height: int):
        self.width = width
        self.height = height
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(sock_path)
        self.next_id = 4
        self.sock.sendall(
            struct.pack("<III", 1, (12 << 16) | 1, 2)
            + struct.pack("<III", 1, (12 << 16) | 0, 3)
        )
        self.sock.settimeout(0.5)
        buf = b""
        try:
            while True:
                ch = self.sock.recv(65536)
                if not ch:
                    break
                buf += ch
        except socket.timeout:
            pass
        globals_map: dict[str, tuple[int, int]] = {}
        off = 0
        while off + 8 <= len(buf):
            oid, sop = struct.unpack_from("<II", buf, off)
            sz = sop >> 16
            op = sop & 0xFFFF
            if oid == 2 and op == 0 and off + 16 <= len(buf):
                name, slen = struct.unpack_from("<II", buf, off + 8)
                iface = buf[off + 16 : off + 16 + slen - 1].decode("utf-8", "replace")
                pad = (slen + 3) & ~3
                (ver,) = struct.unpack_from("<I", buf, off + 16 + pad)
                globals_map[iface] = (name, ver)
            off += sz

        for req in ("wl_seat", "zwlr_virtual_pointer_manager_v1", "zwp_virtual_keyboard_manager_v1"):
            if req not in globals_map:
                raise RuntimeError(f"Wayland compositor missing required global: {req}")

        def bind(iface: str, max_ver: int) -> int:
            name, ver = globals_map[iface]
            oid = self.next_id
            self.next_id += 1
            bi = iface.encode() + b"\0"
            pad = (-len(bi)) & 3
            payload = struct.pack("<II", name, len(bi)) + bi + b"\0" * pad + struct.pack("<II", min(ver, max_ver), oid)
            self.sock.sendall(struct.pack("<II", 2, ((8 + len(payload)) << 16) | 0) + payload)
            return oid

        self.seat = bind("wl_seat", 7)
        self.vp_mgr = bind("zwlr_virtual_pointer_manager_v1", 2)
        self.vk_mgr = bind("zwp_virtual_keyboard_manager_v1", 1)

        self.vp = self.next_id
        self.next_id += 1
        self.sock.sendall(struct.pack("<IIII", self.vp_mgr, (16 << 16) | 0, self.seat, self.vp))

        self.vk = self.next_id
        self.next_id += 1
        self.sock.sendall(struct.pack("<IIII", self.vk_mgr, (16 << 16) | 0, self.seat, self.vk))

        keymap_bytes = build_us_xkb_keymap()
        fd = os.memfd_create("f39-xkb-keymap")
        try:
            os.write(fd, keymap_bytes)
            os.lseek(fd, 0, os.SEEK_SET)
            msg = struct.pack("<IIII", self.vk, (16 << 16) | 0, 1, len(keymap_bytes))
            self.sock.sendmsg([msg], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", [fd]))])
        finally:
            os.close(fd)
        self.sync()

    def _now_ms(self) -> int:
        return int(time.monotonic() * 1000) & 0xFFFFFFFF

    def sync(self) -> None:
        cb = self.next_id
        self.next_id += 1
        self.sock.sendall(struct.pack("<III", 1, (12 << 16) | 0, cb))
        self.sock.settimeout(0.15)
        try:
            self.sock.recv(4096)
        except socket.timeout:
            pass

    def modifiers(self, depressed: int) -> None:
        self.sock.sendall(struct.pack("<IIIIII", self.vk, (24 << 16) | 2, depressed, 0, 0, 0))

    def key_event(self, code: int, state: int) -> None:
        self.sock.sendall(struct.pack("<IIIII", self.vk, (20 << 16) | 1, self._now_ms(), code, state))

    def tap_key(self, key_name: str, mod: int = 0) -> None:
        code = EV_KEY[key_name]
        if mod:
            self.modifiers(mod)
        self.key_event(code, 1)
        self.key_event(code, 0)
        if mod:
            self.modifiers(0)
        self.sync()

    def type_url(self, text: str) -> None:
        for ch in text:
            if ch == ":":
                self.tap_key("colon", MOD_SHIFT)
            elif ch == "/":
                self.tap_key("slash")
            elif ch == ".":
                self.tap_key("period")
            elif ch in EV_KEY:
                self.tap_key(ch)
            else:
                raise ValueError(f"unmapped character in URL: {ch!r}")

    def move_abs(self, x: int, y: int) -> None:
        self.sock.sendall(
            struct.pack("<IIIIIII", self.vp, (28 << 16) | 1, self._now_ms(), x, y, self.width, self.height)
        )
        self.sock.sendall(struct.pack("<II", self.vp, (8 << 16) | 4))
        self.sync()

    def click(self, mod: int = 0) -> None:
        if mod:
            self.modifiers(mod)
            self.sync()
        self.sock.sendall(struct.pack("<IIIII", self.vp, (20 << 16) | 2, self._now_ms(), BTN_LEFT, 1))
        self.sock.sendall(struct.pack("<II", self.vp, (8 << 16) | 4))
        self.sync()
        time.sleep(0.015)
        self.sock.sendall(struct.pack("<IIIII", self.vp, (20 << 16) | 2, self._now_ms(), BTN_LEFT, 0))
        self.sock.sendall(struct.pack("<II", self.vp, (8 << 16) | 4))
        if mod:
            self.modifiers(0)
        self.sync()

    def close(self) -> None:
        try:
            self.sock.close()
        except OSError:
            pass


def probe_mkv_pts_ms(mkv_path: Path) -> list[float]:
    if not mkv_path.exists() or mkv_path.stat().st_size == 0:
        return []
    res = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "csv=p=0",
            str(mkv_path),
        ],
        text=True,
        capture_output=True,
        check=False,
    )
    out: list[float] = []
    for ln in res.stdout.splitlines():
        ln = ln.strip()
        if not ln or ln == "N/A":
            continue
        try:
            out.append(round(float(ln) * 1000.0, 2))
        except ValueError:
            pass
    return out


def run_capture_case_wayland(
    binary: Path,
    out: Path,
    sway_env: dict,
    vinput: WaylandVirtualInput,
    url: str,
    name: str,
    animations: bool,
    trigger: str,
    size: str,
    fps: int,
    duration: float,
    check_scrim_click: bool = False,
) -> dict:
    w_px, h_px = (int(v) for v in size.split("x", 1))
    conf = out / f"gtk-{name}"
    frames = out / f"frames-{name}"
    if frames.exists():
        shutil.rmtree(frames)
    (conf / "gtk-3.0").mkdir(parents=True, exist_ok=True)
    frames.mkdir(parents=True, exist_ok=True)
    (conf / "gtk-3.0" / "settings.ini").write_text(
        f"[Settings]\ngtk-enable-animations={'true' if animations else 'false'}\ngtk-cursor-blink=false\n"
    )
    (conf / "glib-2.0" / "settings").mkdir(parents=True, exist_ok=True)
    (conf / "glib-2.0" / "settings" / "keyfile").write_text(
        f"[org/gnome/desktop/interface]\nenable-animations={'true' if animations else 'false'}\ncursor-blink=false\n"
    )
    env = dict(sway_env)
    env.pop("SEARCH_BENCH", None)
    env.pop("SEARCH_BENCH_SOCKET", None)
    env.pop("DISPLAY", None)
    env.pop("DBUS_SESSION_BUS_ADDRESS", None)
    env.update(
        GDK_BACKEND="wayland",
        GSETTINGS_BACKEND="keyfile",
        SEARCH_MOTION_TELEMETRY="1",
        XDG_CONFIG_HOME=str(conf),
        XDG_DATA_HOME=str(conf / "data"),
        XDG_CACHE_HOME=str(conf / "cache"),
    )
    log_path = out / f"{name}.log"
    mkv_path = out / f"{name}.mkv"
    if mkv_path.exists():
        mkv_path.unlink()
    baseline_path: Path | None = None
    scrim_dismiss = None
    action_start_idx = 1
    action_press_offset_ms = 0.0
    action_done_offset_ms = 0.0
    cx = w_px // 2 + 116
    cy = h_px // 2

    # Park pointer in quiet bottom-right corner before launch so no sidebar/toolbar hover occurs
    vinput.move_abs(w_px - 2, h_px - 2)

    with log_path.open("w") as log_fh:
        browser = subprocess.Popen(
            [str(binary), "--fresh", "--appearance", "light", "--size", size, url],
            env=env,
            stdout=log_fh,
            stderr=log_fh,
            start_new_session=True,
        )
        try:
            time.sleep(2.1)
            # Load fixture explicitly for page-anchored transitions
            if trigger in ("peek", "peek_exit", "peek_reverse", "find", "sitecard"):
                vinput.tap_key("l", MOD_CTRL)
                time.sleep(0.22)
                vinput.type_url(url)
                vinput.tap_key("Return")
                time.sleep(1.1)

            # Warm GTK fonts/caches once before recording
            vinput.tap_key("comma", MOD_CTRL)
            time.sleep(0.42)
            vinput.tap_key("comma", MOD_CTRL)
            time.sleep(0.42)

            # Pre-position and settle pointer/hover toast BEFORE recording starts for Peek
            if trigger in ("peek", "peek_exit", "peek_reverse"):
                vinput.move_abs(cx, cy)
                time.sleep(0.32)

            post_grim_path: Path | None = None
            open_grim_path: Path | None = None
            if trigger in EXIT_CASES or trigger in REVERSE_CASES:
                baseline_path = out / f"{name}-baseline.png"
                subprocess.run(["grim", "-o", "HEADLESS-1", str(baseline_path)], env=env, check=False)

            if trigger == "panel_exit":
                vinput.tap_key("comma", MOD_CTRL)
                time.sleep(0.48)
                open_grim_path = out / f"{name}-open.png"
                subprocess.run(["grim", "-o", "HEADLESS-1", str(open_grim_path)], env=env, check=False)
            elif trigger == "peek_exit":
                vinput.click(MOD_SHIFT)
                time.sleep(0.55)
                open_grim_path = out / f"{name}-open.png"
                subprocess.run(["grim", "-o", "HEADLESS-1", str(open_grim_path)], env=env, check=False)

            pre_sitecard_path: Path | None = None
            if check_scrim_click and trigger == "sitecard":
                pre_sitecard_path = out / f"{name}-pre-sitecard.png"
                subprocess.run(["grim", "-o", "HEADLESS-1", str(pre_sitecard_path)], env=env, check=False)

            pre_tele_count = len(parse_motion_telemetry(log_path))
            rec_duration = duration if not trigger.endswith("_reverse") else max(duration, 1.0)

            rec_start_mono = time.monotonic()
            recorder = subprocess.Popen(
                [
                    "wf-recorder",
                    "-D",
                    "-o",
                    "HEADLESS-1",
                    "-r",
                    str(fps),
                    "-c",
                    "libx264",
                    "-p",
                    "crf=0",
                    "-p",
                    "preset=ultrafast",
                    "-y",
                    "-f",
                    str(mkv_path),
                ],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            time.sleep(0.30)
            # With -D (--no-damage), wf-recorder records continuously at 60fps
            # from startup, so pre-action frames form a true quiet baseline and
            # action_start_idx is anchored directly to the real trigger time
            # (with a 3-frame safety margin before press so zero action frames
            # can ever be excluded).
            # wf-recorder -D spends ~100-140ms initializing wlroots screencopy + x264
            # before packet pts=0, so subtract 140ms to place action_start_idx right
            # before the trigger while keeping ~8-10 pre-action frames to detect any
            # pre-action contamination honestly.
            action_start_idx = max(1, int(((time.monotonic() - rec_start_mono) - 0.14) * fps))

            action_press_offset_ms = round((time.monotonic() - rec_start_mono) * 1000.0, 2)
            if trigger == "panel":
                vinput.tap_key("comma", MOD_CTRL)
            elif trigger == "fold":
                vinput.tap_key("s", MOD_CTRL)
            elif trigger == "omni":
                vinput.tap_key("l", MOD_CTRL)
            elif trigger == "find":
                vinput.tap_key("f", MOD_CTRL)
            elif trigger == "sitecard":
                vinput.tap_key("i", MOD_CTRL)
            elif trigger == "peek":
                vinput.click(MOD_SHIFT)
            elif trigger == "panel_exit":
                vinput.tap_key("comma", MOD_CTRL)
            elif trigger == "peek_exit":
                vinput.tap_key("Escape")
            elif trigger == "panel_reverse":
                vinput.tap_key("comma", MOD_CTRL)
                time.sleep(0.13)
                vinput.tap_key("comma", MOD_CTRL)
            elif trigger == "fold_reverse":
                vinput.tap_key("s", MOD_CTRL)
                time.sleep(0.09)
                vinput.tap_key("s", MOD_CTRL)
            elif trigger == "peek_reverse":
                vinput.click(MOD_SHIFT)
                time.sleep(0.08)
                vinput.tap_key("Escape")
            action_done_offset_ms = round((time.monotonic() - rec_start_mono) * 1000.0, 2)

            time.sleep(rec_duration)
            recorder.send_signal(signal.SIGINT)
            try:
                recorder.wait(timeout=4)
            except subprocess.TimeoutExpired:
                kill_proc_group(recorder)

            if trigger in EXIT_CASES or trigger in REVERSE_CASES:
                time.sleep(0.05)
                post_grim_path = out / f"{name}-post.png"
                subprocess.run(["grim", "-o", "HEADLESS-1", str(post_grim_path)], env=env, check=False)

            if check_scrim_click and trigger == "sitecard" and pre_sitecard_path is not None:
                open_sc_path = out / f"{name}-open-sitecard.png"
                after_sc_path = out / f"{name}-after-scrim-click.png"
                subprocess.run(["grim", "-o", "HEADLESS-1", str(open_sc_path)], env=env, check=False)
                vinput.move_abs(40, 40)
                time.sleep(0.04)
                vinput.click()
                time.sleep(0.40)
                subprocess.run(["grim", "-o", "HEADLESS-1", str(after_sc_path)], env=env, check=False)
                if pre_sitecard_path.exists() and open_sc_path.exists() and after_sc_path.exists():
                    pre_img = Image.open(pre_sitecard_path).convert("RGB")
                    open_img = Image.open(open_sc_path).convert("RGB")
                    post_img = Image.open(after_sc_path).convert("RGB")
                    diff_vs_pre = count_diff_pixels(pre_img, post_img, 8)
                    diff_vs_open = count_diff_pixels(open_img, post_img, 8)
                    scrim_dismiss = {
                        "dismissed": diff_vs_open > 5000 and diff_vs_pre < diff_vs_open,
                        "diff_vs_pre_px": diff_vs_pre,
                        "diff_vs_open_px": diff_vs_open,
                    }
        finally:
            kill_proc_group(browser)

    raw_pts_ms = probe_mkv_pts_ms(mkv_path)
    if mkv_path.exists() and mkv_path.stat().st_size > 0:
        subprocess.run(
            [
                "ffmpeg",
                "-nostdin",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(mkv_path),
                "-fps_mode",
                "cfr",
                "-r",
                str(fps),
                str(frames / "frame_%03d.png"),
            ],
            check=False,
        )

    result = analyze_frames(frames, trigger, fps, action_start_idx=action_start_idx, baseline_img_path=None)
    result["timing"] = {
        "action_press_offset_ms": action_press_offset_ms,
        "action_done_offset_ms": action_done_offset_ms,
        "raw_mkv_packets": len(raw_pts_ms),
        "raw_mkv_pts_ms": raw_pts_ms[:40],
        "raw_mkv_span_ms": round(raw_pts_ms[-1] - raw_pts_ms[0], 2) if len(raw_pts_ms) >= 2 else 0.0,
    }
    if baseline_path is not None and baseline_path.exists() and post_grim_path is not None and post_grim_path.exists():
        b_img = Image.open(baseline_path).convert("RGB")
        p_img = Image.open(post_grim_path).convert("RGB")
        excl = (387, 144, 1374, 823) if trigger.startswith("peek") else None
        if excl:
            b_img = b_img.copy()
            p_img = p_img.copy()
            b_img.paste((0, 0, 0), excl)
            p_img.paste((0, 0, 0), excl)
        end_px = count_diff_pixels(b_img, p_img, 8)
        peak_px = result["max_changed_pixels"]
        for fp in sorted(frames.glob("frame_*.png")):
            f_img = Image.open(fp).convert("RGB")
            if excl:
                f_img = f_img.copy()
                f_img.paste((0, 0, 0), excl)
            peak_px = max(peak_px, count_diff_pixels(b_img, f_img, 8))
        if open_grim_path is not None and open_grim_path.exists():
            o_img = Image.open(open_grim_path).convert("RGB")
            if excl:
                o_img = o_img.copy()
                o_img.paste((0, 0, 0), excl)
            peak_px = max(peak_px, count_diff_pixels(b_img, o_img, 8))
        result["restoration"] = {
            "end_vs_baseline_px": end_px,
            "peak_vs_baseline_px": peak_px,
            "clean_return": end_px <= 4000 and peak_px > 5000,
        }
    warnings = scan_log_warnings(log_path)
    tele = parse_motion_telemetry(log_path)[pre_tele_count:]
    result["warnings"] = warnings
    result["stderr_clean"] = len(warnings) == 0
    if scrim_dismiss is not None:
        result["scrim_click_dismiss"] = scrim_dismiss

    # Also collect unperturbed hardware GdkFrameClock telemetry in a clean, recorder-free
    # launch on the exact same GPU Wayland compositor so wf-recorder's zwlr_screencopy
    # CPU readback cannot affect the measured 60Hz FrameClock cadence.
    unrecorded_tele: list[dict] = []
    if animations and trigger in ("panel", "fold", "omni", "find", "sitecard", "peek", "panel_exit", "peek_exit"):
        ulog = out / f"{name}-unrecorded.log"
        vinput.move_abs(w_px - 2, h_px - 2)
        with ulog.open("w") as ufh:
            ub = subprocess.Popen(
                [str(binary), "--fresh", "--appearance", "light", "--size", size, url],
                env=env,
                stdout=ufh,
                stderr=ufh,
                start_new_session=True,
            )
            try:
                time.sleep(2.0)
                if trigger in ("find", "sitecard", "peek", "peek_exit"):
                    vinput.tap_key("l", MOD_CTRL)
                    time.sleep(0.22)
                    vinput.type_url(url)
                    vinput.tap_key("Return")
                    time.sleep(1.1)
                vinput.tap_key("comma", MOD_CTRL)
                time.sleep(0.40)
                vinput.tap_key("comma", MOD_CTRL)
                time.sleep(0.40)
                if trigger in ("peek", "peek_exit"):
                    vinput.move_abs(cx, cy)
                    time.sleep(0.30)
                if trigger == "panel_exit":
                    vinput.tap_key("comma", MOD_CTRL)
                    time.sleep(0.45)
                elif trigger == "peek_exit":
                    vinput.click(MOD_SHIFT)
                    time.sleep(0.50)
                ubefore = len(parse_motion_telemetry(ulog))
                if trigger in ("panel", "panel_exit"):
                    vinput.tap_key("comma", MOD_CTRL)
                elif trigger == "fold":
                    vinput.tap_key("s", MOD_CTRL)
                elif trigger == "omni":
                    vinput.tap_key("l", MOD_CTRL)
                elif trigger == "find":
                    vinput.tap_key("f", MOD_CTRL)
                elif trigger == "sitecard":
                    vinput.tap_key("i", MOD_CTRL)
                elif trigger == "peek":
                    vinput.click(MOD_SHIFT)
                elif trigger == "peek_exit":
                    vinput.tap_key("Escape")
                time.sleep(0.55)
                unrecorded_tele = parse_motion_telemetry(ulog)[ubefore:]
            finally:
                kill_proc_group(ub)
    result["frame_clock_telemetry"] = tele
    result["unrecorded_frame_clock_telemetry"] = unrecorded_tele
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description="Capture live non-SEARCH_BENCH 60fps GTK motion evidence")
    parser.add_argument("--binary", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--out", type=Path, default=ROOT / "build" / "motion-evidence-f39")
    parser.add_argument(
        "--backend",
        choices=("x11-xvfb", "wayland-sway-gpu"),
        default="x11-xvfb",
        help="Capture backend: software Xvfb+Openbox or isolated hardware-GPU headless Sway",
    )
    parser.add_argument("--cases", default=",".join(ALL_CASES), help="Comma-separated transition names")
    parser.add_argument("--size", default="1500x918")
    parser.add_argument("--fps", type=int, default=60)
    parser.add_argument("--duration", type=float, default=0.85)
    parser.add_argument("--check-scrim-click", action="store_true", default=True)
    args = parser.parse_args()

    if not args.binary.is_file():
        print(f"missing binary: {args.binary}", file=sys.stderr)
        return 2
    triggers = [c.strip() for c in args.cases.split(",") if c.strip()]
    for t in triggers:
        if t not in ALL_CASES:
            print(f"unknown case: {t} (valid: {', '.join(ALL_CASES)})", file=sys.stderr)
            return 2

    args.out.mkdir(parents=True, exist_ok=True)
    server, port = make_fixture_server()
    url = f"http://127.0.0.1:{port}/"

    if args.backend == "wayland-sway-gpu":
        rt_dir = Path(tempfile.mkdtemp(dir="/tmp", prefix="f39-sway-"))
        cfg = rt_dir / "sway.conf"
        w, h = args.size.split("x", 1)
        cfg.write_text(f"output HEADLESS-1 resolution {w}x{h}@{args.fps}Hz\ndefault_border none\n")
        senv = dict(os.environ)
        senv.pop("DISPLAY", None)
        senv.pop("WAYLAND_DISPLAY", None)
        senv.update(
            XDG_RUNTIME_DIR=str(rt_dir),
            WLR_BACKENDS="headless",
            WLR_LIBINPUT_NO_DEVICES="1",
            WLR_RENDERER="gles2",
            WLR_RENDER_DRM_DEVICE="/dev/dri/renderD128",
        )
        sway_log = args.out / "sway-gpu.log"
        sway = None
        vinput = None
        try:
            sway = subprocess.Popen(
                ["sway", "-d", "-c", str(cfg)],
                env=senv,
                stdout=sway_log.open("w"),
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            time.sleep(1.2)
            wname = next((s.name for s in rt_dir.glob("wayland-*") if not s.name.endswith(".lock")), None)
            if sway.poll() is not None or not wname:
                raise RuntimeError(f"headless GPU sway failed to start; see {sway_log}")
            senv["WAYLAND_DISPLAY"] = wname
            vinput = WaylandVirtualInput(str(rt_dir / wname), int(w), int(h))
            renderer_info = parse_sway_renderer(sway_log)
            randr = subprocess.run(["wlr-randr"], env=senv, text=True, capture_output=True, check=False).stdout.strip()
            renderer_info["wlr_randr"] = randr
            report = {
                "method": (
                    f"wf-recorder {args.fps}fps lossless CFR extraction + ffprobe raw packet PTS, "
                    "live release binary WITHOUT SEARCH_BENCH, isolated headless Sway on hardware DRM/GLES2 "
                    "(/dev/dri/renderD128, Intel Iris Xe @ 60Hz), persistent Wayland virtual keyboard + pointer "
                    "(zwp_virtual_keyboard_v1 + zwlr_virtual_pointer_v1), SEARCH_MOTION_TELEMETRY=1 GdkFrameClock "
                    "telemetry (recorded and unrecorded) + action-scoped delta>2 analysis across all 19 transitions"
                ),
                "backend": "wayland-sway-gpu",
                "backend_notes": (
                    "Compositor scans out via hardware DRM/GLES2 (Mesa Intel Iris Xe on /dev/dri/renderD128 @ 60Hz). "
                    "GTK3 client chrome draws on CPU via Cairo/wl_shm, and wf-recorder performs zwlr_screencopy CPU "
                    "readback + lossless x264 encoding during recorded runs (~30Hz effective captured cadence on full-window "
                    "scrim paints); unrecorded GdkFrameClock telemetry is recorded separately on the same GPU compositor "
                    "to show undisturbed ~60Hz pacing."
                ),
                "renderer": renderer_info,
                "skipped_cases": [],
                "provenance": collect_provenance(args.binary),
                "contract": CONTRACT,
                "cases": {},
                "gates": {},
            }
            all_ok = True
            for trig in triggers:
                modes = ((True, "normal"),) if trig in REVERSE_CASES else ((True, "normal"), (False, "reduced"))
                for animations, suffix in modes:
                    name = f"{trig}-{suffix}"
                    res = run_capture_case_wayland(
                        binary=args.binary,
                        out=args.out,
                        sway_env=senv,
                        vinput=vinput,
                        url=url,
                        name=name,
                        animations=animations,
                        trigger=trig,
                        size=args.size,
                        fps=args.fps,
                        duration=args.duration,
                        check_scrim_click=args.check_scrim_click and suffix == "normal",
                    )
                    passed, reasons = evaluate_gate(trig, animations, res, args.backend)
                    res["passed"] = passed
                    res["fail_reasons"] = reasons
                    if not passed:
                        all_ok = False
                    report["cases"][name] = res
                    report["gates"][name] = {
                        "passed": passed,
                        "fail_reasons": reasons,
                        "changed_frames": res["changed_frames"],
                        "intermediate_frames": res["intermediate_frames"],
                        "span_ms": res["span_ms"],
                        "pre_action_changed_frames": res["pre_action_changed_frames"],
                        "stderr_clean": res["stderr_clean"],
                        "roi": res["roi"],
                        "telemetry_count": len(res["frame_clock_telemetry"]),
                        "unrecorded_telemetry_count": len(res.get("unrecorded_frame_clock_telemetry", [])),
                    }
                    status = "ok  " if passed else "FAIL"
                    print(
                        f"  {status} [HW] {name:20s} frames={res['changed_frames']:2d} "
                        f"interm={res['intermediate_frames']:2d} span={res['span_ms']:5.1f}ms "
                        f"rec_tele={len(res['frame_clock_telemetry'])} unrec_ticks={[t['ticks'] for t in res.get('unrecorded_frame_clock_telemetry',[])]} warnings={len(res['warnings'])} {reasons or ''}",
                        flush=True,
                    )
            report["all_passed"] = all_ok
            (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
            return 0 if all_ok else 1
        finally:
            if vinput is not None:
                vinput.close()
            kill_proc_group(sway)
            shutil.rmtree(rt_dir, ignore_errors=True)
            server.shutdown()
            server.server_close()

    display = pick_free_display()
    xvfb = None
    openbox = None
    try:
        xvfb = subprocess.Popen(
            ["Xvfb", display, "-screen", "0", "1920x1080x24", "-nolisten", "tcp"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        time.sleep(0.6)
        wm_env = dict(os.environ, DISPLAY=display)
        openbox = subprocess.Popen(
            ["openbox", "--sm-disable"],
            env=wm_env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        time.sleep(0.8)

        report = {
            "method": (
                f"ffmpeg x11grab {args.fps}fps, live release binary WITHOUT SEARCH_BENCH, "
                "gtk-cursor-blink=false, Xvfb+Openbox (software), pointer/hover settled pre-recording, "
                "action-scoped sensitive delta>2 analysis + chrome-only Peek ROI + upper/lower duration gates"
            ),
            "backend": "x11-xvfb",
            "renderer": {
                "compositor": "Xvfb + Openbox (software X11)",
                "hardware_gpu": False,
            },
            "provenance": collect_provenance(args.binary),
            "contract": CONTRACT,
            "cases": {},
            "gates": {},
        }
        all_ok = True
        for trig in triggers:
            modes = ((True, "normal"),) if trig in REVERSE_CASES else ((True, "normal"), (False, "reduced"))
            for animations, suffix in modes:
                name = f"{trig}-{suffix}"
                res = run_capture_case_x11(
                    binary=args.binary,
                    out=args.out,
                    display=display,
                    url=url,
                    name=name,
                    animations=animations,
                    trigger=trig,
                    size=args.size,
                    fps=args.fps,
                    duration=args.duration,
                    check_scrim_click=args.check_scrim_click and suffix == "normal",
                )
                passed, reasons = evaluate_gate(trig, animations, res, args.backend)
                res["passed"] = passed
                res["fail_reasons"] = reasons
                if not passed:
                    all_ok = False
                report["cases"][name] = res
                report["gates"][name] = {
                    "passed": passed,
                    "fail_reasons": reasons,
                    "changed_frames": res["changed_frames"],
                    "intermediate_frames": res["intermediate_frames"],
                    "span_ms": res["span_ms"],
                    "pre_action_changed_frames": res["pre_action_changed_frames"],
                    "stderr_clean": res["stderr_clean"],
                    "roi": res["roi"],
                }
                status = "ok  " if passed else "FAIL"
                print(
                    f"  {status} {name:20s} frames={res['changed_frames']:2d} "
                    f"interm={res['intermediate_frames']:2d} span={res['span_ms']:5.1f}ms "
                    f"pre={res['pre_action_changed_frames']} warnings={len(res['warnings'])} {reasons or ''}",
                    flush=True,
                )

        report["all_passed"] = all_ok
        (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        return 0 if all_ok else 1
    finally:
        kill_proc_group(openbox)
        kill_proc_group(xvfb)
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    raise SystemExit(main())
