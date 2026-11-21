#!/usr/bin/env python3
"""Prove the extension loader with extensions that say whether they ran.

Three are built here rather than downloaded: one whose content script marks the
page, one that only asks for a background page, and one locked to another site.
The point is that the browser tells the difference and can be seen to.

    python3 tools/extensions.py
"""
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from bench import Bench, BenchError, DEFAULT_SOCKET  # noqa: E402

MARKER_JS = "document.documentElement.setAttribute('data-search-ext', 'marker');"
MARKER_CSS = (
    'html[data-search-ext="marker"] body::after { content: "extension ran"; '
    "position: fixed; bottom: 0; left: 8px; z-index: 2147483647; "
    'background: #22c55e; color: #06240f; font: 12px sans-serif; padding: 2px 6px; }'
)


def manifest(name, **extra):
    base = {"manifest_version": 3, "name": name, "version": "1.0"}
    base.update(extra)
    return base


def build(folder, data, files):
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "manifest.json").write_text(json.dumps(data, indent=2))
    for name, text in files.items():
        (folder / name).write_text(text)


class Run:
    def __init__(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="search-ext-"))
        self.browser = None
        self.bench = None
        self.server = None
        self.port = 0
        self.failures = []

    def check(self, name, condition, detail=""):
        print(f"  {'ok  ' if condition else 'FAIL'} {name}" + ("" if condition else f"  {detail}"))
        if not condition:
            self.failures.append(name)

    def start_server(self):
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            self.port = probe.getsockname()[1]
        self.server = subprocess.Popen(
            [sys.executable, str(ROOT / "tools" / "fixtures.py"), str(self.port)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(80):
            try:
                with socket.create_connection(("127.0.0.1", self.port), timeout=0.2):
                    return
            except OSError:
                time.sleep(0.1)
        raise RuntimeError("the fixture server did not start")

    def start_browser(self):
        env = dict(os.environ)
        env["SEARCH_BENCH"] = "1"
        env["XDG_CONFIG_HOME"] = str(self.home / "config")
        env["XDG_DATA_HOME"] = str(self.home / "data")
        env.pop("DISPLAY", None)
        env["GDK_BACKEND"] = "wayland"
        self.browser = subprocess.Popen(
            [str(ROOT / "target/release/search"), "--appearance", "light", "--fresh"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        deadline = time.time() + 30
        while time.time() < deadline:
            if self.browser.poll() is not None:
                out = self.browser.stderr.read().decode(errors="replace")[-1500:]
                raise RuntimeError(f"the browser exited at launch:\n{out}")
            try:
                self.bench = Bench(timeout=15)
                self.bench.must("ping")
                return
            except (BenchError, OSError):
                time.sleep(0.1)
        raise RuntimeError("the bench socket never answered")

    def stop(self):
        if self.bench:
            try:
                self.bench.command("quit")
            except Exception:
                pass
        if self.browser and self.browser.poll() is None:
            self.browser.terminate()
            try:
                self.browser.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.browser.kill()
                self.browser.wait()
        if self.server:
            self.server.terminate()
        socket_path = pathlib.Path(
            self.bench.path if self.bench else os.environ.get("SEARCH_BENCH_SOCKET", DEFAULT_SOCKET)
        )
        socket_path.unlink(missing_ok=True)
        shutil.rmtree(self.home, ignore_errors=True)

    def page(self, path):
        return f"http://127.0.0.1:{self.port}{path}"


def main():
    work = pathlib.Path(tempfile.mkdtemp(prefix="search-ext-build-"))
    run = None
    try:
        marker = work / "marker"
        bg_only = work / "background"
        other_site = work / "other"

        build(
            marker,
            manifest(
                "Marker",
                description="Marks the page so the loader can be seen working.",
                content_scripts=[
                    {
                        "matches": ["*://127.0.0.1/*"],
                        "js": ["marker.js"],
                        "css": ["marker.css"],
                        "run_at": "document_end",
                    }
                ],
            ),
            {"marker.js": MARKER_JS, "marker.css": MARKER_CSS},
        )
        build(
            bg_only,
            manifest(
                "Background Only",
                background={"service_worker": "bg.js"},
                permissions=["tabs", "cookies"],
            ),
            {"bg.js": "chrome.tabs.query({}, () => {});"},
        )
        build(
            other_site,
            manifest(
                "Other Site",
                content_scripts=[{"matches": ["*://example.com/*"], "js": ["x.js"]}],
            ),
            {"x.js": "document.documentElement.setAttribute('data-search-ext', 'other');"},
        )

        run = Run()
        run.start_server()
        run.start_browser()
        self_bench = run.bench

        result = self_bench.command(f"installext {marker}")
        run.check("an unpacked extension installs", result.get("ok") is True, result)
        run.check("and reports that it runs", result.get("runs") is True, result)

        blocked = self_bench.command(f"installext {bg_only}")
        run.check("one with only a background page installs too", blocked.get("ok") is True, blocked)
        run.check("and is reported as not running", blocked.get("runs") is False, blocked)
        why = blocked.get("why_not", "")
        run.check("for a stated reason", "background" in why, why)
        run.check("naming a permission it cannot have", "cookies" in why, why)

        run.check("a third one installs", self_bench.command(f"installext {other_site}").get("ok") is True)

        listed = self_bench.command("extensions").get("extensions", [])
        names = {entry["name"] for entry in listed}
        run.check(
            "all three are on the installed list",
            {"Marker", "Background Only", "Other Site"} <= names,
            names,
        )

        tab = self_bench.open(run.page("/plain"))
        self_bench.wait_loaded(tab_id=tab)
        seen = json.loads(
            self_bench.command(
                "js (() => JSON.stringify({"
                " marked: document.documentElement.getAttribute('data-search-ext'),"
                " pill: getComputedStyle(document.body, '::after').content"
                "}))()"
            ).get("value", "{}") or "{}"
        )
        run.check("its script ran on a page it asks for", seen.get("marked") == "marker", seen)
        run.check("its style sheet applied too", "extension ran" in str(seen.get("pill", "")), seen)

        # The extension locked to example.com must leave this page alone, or the
        # allow list is decoration.
        run.check(
            "an extension locked to another site did not run here",
            seen.get("marked") != "other",
            seen,
        )
    finally:
        if run is not None:
            run.stop()
        shutil.rmtree(work, ignore_errors=True)

    print("all extension checks passed" if not run.failures else f"{len(run.failures)} failed")
    return 1 if run.failures else 0


if __name__ == "__main__":
    sys.exit(main())
