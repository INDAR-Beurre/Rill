#!/usr/bin/env python3
"""Collect reproducible motion evidence from the browser bench socket.

Usage: python3 tools/motion_evidence.py [output.json]

Records the pure timing contract, the reduced-motion decision table, and a
live main-loop drift sample. All values are produced by the running app or by
`cargo test`; nothing here invents a frame time.
"""
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bench import Bench

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "build" / "motion-evidence.json"


def main() -> int:
    bench = Bench(timeout=45.0)
    cfg = bench.must("motioncfg")
    drift = bench.must("drift")
    tests = subprocess.run(
        ["cargo", "test", "--release", "motion::tests::", "--", "--nocapture"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    evidence = {
        "source": "src/motion.rs and tools/stress.py via the running browser bench socket",
        "config": cfg,
        "live_drift": drift,
        "reduced_motion_test": {
            "command": "cargo test --release motion::tests:: -- --nocapture",
            "rc": tests.returncode,
            "tail": (tests.stdout + tests.stderr).strip().splitlines()[-8:],
        },
        "contract": {
            "panel": "PANEL 180ms",
            "fold": "FOLD 130ms",
            "omnibox": "OMNI 120ms",
            "find": "MICRO 120ms",
            "peek": "PEEK 220ms",
            "site_card": "CARD 180ms",
            "reduced_motion": "GTK gtk-enable-animations=false or SEARCH_BENCH makes every transition finish synchronously",
        },
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(evidence, indent=2))
    return 0 if tests.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
