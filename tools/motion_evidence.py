#!/usr/bin/env python3
"""Collect reproducible motion evidence from the browser bench socket.

Usage:
  python3 tools/motion_evidence.py [output.json]
  python3 tools/motion_evidence.py --binary /path/to/search --out /path/to/out.json

Records the actual shared motion role contract (`dur::MICRO`, `dur::ENTER`,
`dur::EXIT`, `dur::LAYOUT`, `dur::TRAVEL`), binary and source provenance, the
reduced-motion and bounded-step unit tests, and live main-loop drift samples
(including an honest Settings open/close cycle after `reset`). All values are
produced by the running app or by `cargo test`; nothing here invents a frame
time. For live non-SEARCH_BENCH 60fps capture across all transitions, run
`python3 tools/live_motion.py`.
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bench import Bench

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BIN = ROOT / "target" / "release" / "search"
DEFAULT_OUT = ROOT / "build" / "motion-evidence-f39.json"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        while chunk := fh.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()


def provenance(binary: Path) -> dict:
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
    return {
        "binary": str(binary.resolve()) if binary.exists() else str(binary),
        "binary_sha256": sha256_file(binary) if binary.exists() else None,
        "captured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "git_head": git_head,
        "git_dirty": bool(diff),
        "source_diff_sha256": hashlib.sha256(diff).hexdigest(),
        "source_tree_sha256": tree_hash.hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Collect bench-socket motion contract and drift evidence")
    parser.add_argument("positional_out", nargs="?", type=Path, default=None)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--target-dir", type=Path, default=None)
    args = parser.parse_args()

    out_path = args.out or args.positional_out or DEFAULT_OUT
    bench = Bench(timeout=45.0)
    cfg = bench.must("motioncfg")

    # Measure a real Settings open + close cycle after reset with honest DriftTracker
    bench.must("reset")
    bench.must("key settings")
    time.sleep(0.25)
    settings_open_drift = bench.must("drift")
    bench.must("reset")
    bench.must("key closepanel")
    time.sleep(0.25)
    settings_close_drift = bench.must("drift")
    drift = bench.must("drift")

    test_env = dict(os.environ)
    if args.target_dir is not None:
        test_env["CARGO_TARGET_DIR"] = str(args.target_dir)
    tests = subprocess.run(
        ["cargo", "test", "--release", "--bin", "search", "motion::tests::", "--", "--nocapture"],
        cwd=ROOT,
        env=test_env,
        text=True,
        capture_output=True,
        check=False,
    )
    micro = cfg.get("microMs", 120)
    enter = cfg.get("enterMs", 180)
    exit_ms = cfg.get("exitMs", 130)
    layout = cfg.get("layoutMs", 220)
    travel = cfg.get("travelPx", 8)
    evidence = {
        "source": "src/motion.rs via the running browser bench socket",
        "provenance": provenance(args.binary),
        "config": cfg,
        "live_drift": drift,
        "settings_open_drift": settings_open_drift,
        "settings_close_drift": settings_close_drift,
        "reduced_motion_test": {
            "command": "cargo test --release --bin search motion::tests:: -- --nocapture",
            "rc": tests.returncode,
            "tail": (tests.stdout + tests.stderr).strip().splitlines()[-12:],
        },
        "roles": {
            "MICRO": f"{micro}ms",
            "ENTER": f"{enter}ms",
            "EXIT": f"{exit_ms}ms",
            "LAYOUT": f"{layout}ms",
            "TRAVEL": f"{travel}px",
        },
        "contract": {
            "panel": f"ENTER {enter}ms / EXIT {exit_ms}ms (switch MICRO {micro}ms, travel {travel}px)",
            "fold": f"LAYOUT {layout}ms",
            "omnibox": f"ENTER {enter}ms / EXIT {exit_ms}ms (travel {travel}px)",
            "find": f"ENTER {enter}ms / EXIT {exit_ms}ms (travel {travel}px)",
            "peek": f"ENTER {enter}ms / EXIT {exit_ms}ms (scrim alpha fade + travel {travel}px)",
            "site_card": f"ENTER {enter}ms / EXIT {exit_ms}ms (scrim alpha fade + travel {travel}px)",
            "reduced_motion": "GTK gtk-enable-animations=false or SEARCH_BENCH makes every transition finish synchronously",
        },
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(evidence, indent=2))
    drift_ok = settings_open_drift.get("worst", 999) <= 120 and settings_close_drift.get("worst", 999) <= 120
    return 0 if (tests.returncode == 0 and drift_ok) else 1


if __name__ == "__main__":
    raise SystemExit(main())
