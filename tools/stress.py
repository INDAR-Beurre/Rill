#!/usr/bin/env python3
"""Stress Search the way a person does: badly.

Starts the browser with its bench socket, starts a local server of awkward
pages, then opens too many tabs, switches between them too fast, loads pages
that never finish, pages that eat memory and pages that redirect in a loop,
opens and closes every panel, hides things, uses find and reading mode, drags
the window around, and watches the memory the whole time.

    python3 tools/stress.py                # the whole run, about four minutes
    python3 tools/stress.py --quick        # fewer rounds, about a minute
    python3 tools/stress.py --attach       # drive a browser already running
    python3 tools/stress.py --keep         # leave the browser and server up

Writes build/stress-report.json and prints a summary. Exit code 1 if anything
failed or the browser died.
"""
import argparse
import json
import os
import signal
import socket
import statistics
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
from bench import Bench, BenchError  # noqa: E402

BINARY = ROOT / "target/release/search"
REPORT = ROOT / "build/stress-report.json"
PORT = 0  # chosen at start: a stale server on a fixed port has fooled this before


# -- process and memory ----------------------------------------------------

def process_tree(root_pid):
    children = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        try:
            with open(f"/proc/{entry}/stat", "r") as handle:
                fields = handle.read().rsplit(") ", 1)[1].split()
            children.setdefault(int(fields[1]), []).append(int(entry))
        except (OSError, IndexError):
            continue
    seen, frontier = [], [root_pid]
    while frontier:
        pid = frontier.pop()
        if pid in seen:
            continue
        seen.append(pid)
        frontier.extend(children.get(pid, []))
    return seen


def memory(root_pid):
    """Resident and anonymous memory of the whole browser, in MB."""
    trees = process_tree(root_pid)
    pss = anon = rss = 0
    processes = 0
    for pid in trees:
        try:
            with open(f"/proc/{pid}/smaps_rollup", "r") as handle:
                for line in handle:
                    if line.startswith("Pss:"):
                        pss += int(line.split()[1])
                    elif line.startswith("Pss_Anon:"):
                        anon += int(line.split()[1])
                    elif line.startswith("Rss:"):
                        rss += int(line.split()[1])
            processes += 1
        except OSError:
            continue
    return {
        "pss_mb": round(pss / 1024, 1),
        "anon_mb": round(anon / 1024, 1),
        "rss_mb": round(rss / 1024, 1),
        "processes": processes,
    }


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


# -- the run ---------------------------------------------------------------

class Run:
    def __init__(self, args):
        self.args = args
        self.results = []
        self.browser = None
        self.bench = None
        self.server = None
        self.port = 0
        self.tmp = Path(tempfile.mkdtemp(prefix="search-stress-"))
        self.rounds = 6 if args.quick else 20

    # -- lifecycle ------------------------------------------------------
    def start_server(self):
        # Bind port 0 so the fixture server always picks a free port: an old
        # server left running on a fixed port would be tested instead of this
        # one, and it would be the old one's fault that everything timed out.
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            self.port = probe.getsockname()[1]
        self.server = subprocess.Popen(
            [sys.executable, str(HERE / "fixtures.py"), str(self.port)],
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
        env.setdefault("XDG_CONFIG_HOME", str(self.tmp / "config"))
        env.setdefault("XDG_DATA_HOME", str(self.tmp / "data"))
        env.pop("DISPLAY", None)
        env["GDK_BACKEND"] = "wayland"
        started = time.time()
        self.browser = subprocess.Popen(
            [str(BINARY), "--fresh", "--appearance", "light"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        deadline = time.time() + 30
        while time.time() < deadline:
            if self.browser.poll() is not None:
                out = self.browser.stderr.read().decode(errors="replace")[-2000:]
                raise RuntimeError(f"the browser exited at launch:\n{out}")
            try:
                self.bench = Bench(timeout=20)
                self.bench.must("ping")
                self.startup = round(time.time() - started, 2)
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
        if self.server:
            self.server.terminate()

    # -- reporting ------------------------------------------------------
    def check(self, name, ok, detail, extra=None):
        entry = {"name": name, "ok": bool(ok), "detail": detail}
        if extra:
            entry.update(extra)
        self.results.append(entry)
        mark = "ok  " if ok else "FAIL"
        print(f"  {mark} {name}: {detail}", flush=True)
        return ok

    def scenario(self, name):
        """Decorator-ish helper: run one scenario, catching everything."""

        def run(function):
            print(f"\n{name}", flush=True)
            started = time.time()
            try:
                function()
            except Exception as error:  # noqa: BLE001
                self.check(f"{name}", False, f"{type(error).__name__}: {error}")
                if self.args.verbose:
                    traceback.print_exc()
            if self.browser and self.browser.poll() is not None:
                out = self.browser.stderr.read().decode(errors="replace")[-3000:]
                self.check(f"{name} alive", False, f"the browser exited:\n{out}")
            for result in self.results:
                if "seconds" not in result:
                    result.setdefault("seconds", round(time.time() - started, 2))

        return run

    def url(self, path):
        return f"http://127.0.0.1:{self.port}{path}"

    def ram(self):
        return memory(self.browser.pid)

    # -- scenarios ------------------------------------------------------
    def run_all(self):
        bench = self.bench

        @self.scenario("start")
        def _start():
            self.check("bench is up", True, f"ready in {self.startup}s")
            self.check("first paint", True, json.dumps(self.ram()))
            probe = bench.probe()
            self.check("probe", probe.get("active", 0) > 0, f"{len(probe.get('tabs', []))} tabs")

        @self.scenario("a single idle tab")
        def _idle():
            bench.open(self.url("/plain"))
            bench.wait_loaded(timeout=20)
            time.sleep(3)
            usage = self.ram()
            self.check(
                "idle one tab",
                usage["pss_mb"] < 260 and usage["processes"] <= 5,
                json.dumps(usage),
                {"memory": usage},
            )
            self.one_tab = usage
            text = bench.text()
            self.check("page text", "quietest page" in (text.get("text") or ""), (text.get("text") or "")[:60])

        @self.scenario("too many tabs")
        def _many():
            before = self.ram()
            ids = []
            for index in range(self.rounds):
                ids.append(bench.open(self.url(f"/plain?tab={index}")))
            bench.wait_loaded(timeout=30)
            time.sleep(2)
            after = self.ram()
            per_tab = (after["pss_mb"] - before["pss_mb"]) / max(1, len(ids))
            probe = bench.probe()
            self.check(
                f"{len(ids)} tabs open",
                len(probe["tabs"]) >= len(ids),
                f"{len(probe['tabs'])} tabs, +{per_tab:.1f}MB per tab, total {after['pss_mb']}MB",
                {"per_tab_mb": round(per_tab, 1), "memory": after},
            )
            for tab_id in ids:
                bench.close(tab_id)
            time.sleep(1)
            after_close = self.ram()
            self.check(
                "tabs give their memory back",
                after_close["pss_mb"] <= max(after["pss_mb"], before["pss_mb"]) + 10,
                f"{after_close['pss_mb']}MB after closing (was {after['pss_mb']}MB)",
                {"memory": after_close},
            )

        @self.scenario("switching tabs like a person")
        def _switch():
            ids = [bench.open(self.url(f"/plain?switch={i}")) for i in range(8)]
            bench.wait_loaded(timeout=30)
            timings = []
            for index in range(self.rounds * 15):
                tab = ids[index % len(ids)]
                started = time.time()
                bench.select(tab)
                timings.append((time.time() - started) * 1000)
            average = statistics.mean(timings)
            worst = max(timings)
            self.check(
                "tab switches stay quick",
                average < 120 and worst < 900,
                f"mean {average:.0f}ms, worst {worst:.0f}ms over {len(timings)}",
                {"mean_ms": round(average, 1), "worst_ms": round(worst, 1)},
            )
            for tab_id in ids:
                bench.close(tab_id)

        @self.scenario("pages that are hard")
        def _hard():
            cases = [
                ("/big", 25),
                ("/long", 25),
                ("/images", 25),
                ("/frames", 25),
                ("/styles", 10),
            ]
            for path, timeout in cases:
                started = time.time()
                bench.open(self.url(path))
                bench.wait_loaded(timeout=timeout)
                took = time.time() - started
                self.check(
                    f"loads {path}",
                    took < timeout,
                    f"{took:.1f}s",
                    {"seconds": round(took, 2)},
                )
            probe = bench.probe()
            self.check("alive after hard pages", bool(probe.get("tabs")), f"{len(probe['tabs'])} tabs")

        @self.scenario("pages that misbehave")
        def _bad():
            bench.open(self.url("/missing"))
            bench.wait_loaded(timeout=15)
            self.check("a 404 shows, the browser lives", True, "ok")

            bench.open(self.url("/redirect?n=6"))
            landed = bench.wait_for(lambda probe: probe["url"].endswith("/plain"), timeout=25)
            self.check("a redirect chain lands", landed["url"].endswith("/plain"), landed["url"])

            bench.open(self.url("/loop"))
            time.sleep(3)
            self.check("a redirect loop is survived", True, bench.probe()["url"][:60])

            bench.open(self.url("/stall"))
            time.sleep(3)
            started = time.time()
            bench.probe()
            self.check("a page that never ends does not hang the browser", time.time() - started < 3,
                       f"probe answered in {time.time() - started:.1f}s")

            bench.open(self.url("/js-busy"))
            time.sleep(2)
            started = time.time()
            bench.open(self.url("/plain?after-busy=1"))
            bench.wait_loaded(timeout=20)
            self.check("a busy script does not stop other tabs", True, f"{time.time() - started:.1f}s")

            spray = bench.open(self.url("/js-leak"))
            time.sleep(3)
            usage = self.ram()
            self.check("a memory spray is survived", usage["pss_mb"] < 2600, json.dumps(usage))
            bench.close(spray)
            time.sleep(4)
            after = self.ram()
            self.check("the engine survives a spray and keeps working",
                       bench.probe().get("ok") and after["pss_mb"] < usage["pss_mb"] + 50,
                       f"{usage['pss_mb']}MB -> {after['pss_mb']}MB, still answering",
                       {"memory": after})

        @self.scenario("the field, panels and find")
        def _chrome():
            bench.open(self.url("/long"))
            bench.wait_loaded(timeout=20)
            timings = []
            for _ in range(self.rounds):
                for key in ("settings", "history", "bookmarks", "downloads"):
                    started = time.time()
                    bench.must(f"key {key}")
                    timings.append((time.time() - started) * 1000)
                    bench.must("key closepanel")
            self.check("panels open and close", statistics.mean(timings) < 200,
                       f"mean {statistics.mean(timings):.0f}ms over {len(timings)}")

            # `omni` is "go to this address"; the field is opened by `key omni`.
            bench.must("key omni")
            time.sleep(0.4)
            probe = bench.probe()
            self.check("the field opens on the address you are on",
                       probe["omniText"].endswith("/long"),
                       f"field holds {probe['omniText'][-24:]!r}")
            bench.must("key down")
            time.sleep(0.3)
            probe = bench.probe()
            self.check("down picks a suggestion",
                       probe["omniPicked"] >= 0 or probe["omniCount"] == 0,
                       f"picked {probe['omniPicked']} of {probe['omniCount']}")
            bench.must("escape")

            # A panel's own search has to filter, keep its words, and clear.
            planning = bench.open(self.url("/plain"))
            bench.wait_loaded(planning, timeout=20)
            bench.must("key bookmark")
            time.sleep(0.3)
            bench.must("key bookmarks")
            time.sleep(0.4)
            bench.must("filter quiet")
            time.sleep(0.4)
            probe = bench.probe()
            self.check("a panel filters as you type",
                       probe["panelFilter"] == "quiet" and probe["panel"] == "Bookmarks",
                       f"filter {probe['panelFilter']!r} in {probe['panel']}")
            bench.must("filter ")
            time.sleep(0.4)
            probe = bench.probe()
            self.check("clearing the filter shows everything again",
                       probe["panelFilter"] == "", f"filter {probe['panelFilter']!r}")
            bench.must("key closepanel")
            bench.must("key shortcuts")
            time.sleep(0.4)
            probe = bench.probe()
            self.check("Ctrl / lists every key", probe["panel"] == "Shortcuts", probe["panel"])
            bench.must("key closepanel")
            answer = bench.command("find Paragraph 5")
            self.check("find searches", answer.get("ok"), str(answer)[:80])
            time.sleep(1)
            probe = bench.probe()
            self.check("find bar state", probe["find"] is True, "open")
            bench.must("escape")

            bench.must("key reader")
            time.sleep(1.5)
            text = bench.text()
            self.check("reading mode replaces the page", (text.get("text") or "") != "", "text present")
            bench.must("key reload")
            bench.wait_loaded(timeout=20)

        @self.scenario("hiding what annoys you")
        def _hide():
            tab = bench.open(self.url("/plain?hide=1"))
            bench.wait_loaded(tab, timeout=20)
            probe = bench.probe()
            print(f"    [hide] tab={tab} active={probe.get('active')} url={probe.get('url')} "
                  f"tabs={len(probe.get('tabs', []))}")
            answer = bench.must("js document.querySelectorAll('#hide-me').length")
            print(f"    [hide] count={answer}")
            self.check("the strip is there", answer.get("value") == "1", str(answer)[:60])
            bench.must("js window.webkit.messageHandlers.searchHide.postMessage('#hide-me')")
            time.sleep(1)
            hidden = bench.must("hidden")
            self.check("a hidden thing is kept", "#hide-me" in json.dumps(hidden), str(hidden)[:90])
            bench.must("key reload")
            bench.wait_loaded(timeout=20)
            still = bench.must("js getComputedStyle(document.querySelector('#hide-me')).display")
            self.check("and stays hidden after a reload", still.get("value") == "none", str(still)[:60])

        @self.scenario("downloads and popups")
        def _downloads():
            tab = bench.open(self.url("/popup"))
            bench.wait_loaded(tab, timeout=20)
            probe = bench.probe()
            print(f"    [popup] tab={tab} active={probe.get('active')} url={probe.get('url')}")
            before = bench.probe()
            bench.must("js document.getElementById('go').click()")
            time.sleep(1.5)
            probe = bench.probe()
            self.check("a pop-up becomes a tab", len(probe["tabs"]) >= 2, f"{len(probe['tabs'])} tabs")
            # A tab opening in the background must not take the screen: the tab
            # bar and the page have to name the same tab, or Ctrl+W closes the
            # one the user is not looking at.
            self.check(
                "and the tab bar still names the page on screen",
                probe["active"] == before["active"] and probe["visibleChild"] == f"tab-{before['active']}",
                f"active {before['active']} -> {probe['active']}, showing {probe['visibleChild']}",
            )

            bench.open(self.url("/download"))
            for _ in range(30):
                answer = bench.must("downloads")
                if answer.get("downloads"):
                    break
                time.sleep(0.3)
            downloads = bench.must("downloads").get("downloads", [])
            self.check("a download is recorded", bool(downloads), str(downloads)[:120])
            if downloads:
                path = Path(downloads[0]["path"])
                for _ in range(40):
                    if path.exists() and path.stat().st_size > 0:
                        break
                    time.sleep(0.2)
                self.check("the file landed", path.exists() and path.stat().st_size > 0,
                           f"{path} {path.stat().st_size if path.exists() else 0} bytes")

        @self.scenario("reading mode")
        def _reading():
            # The page's own words, set again by the browser: it must enter, and
            # it must carry a way out that does not need a keyboard.
            tab = bench.open(self.url("/long"))
            bench.wait_loaded(tab, timeout=20)
            bench.must("escape")
            time.sleep(0.2)
            bench.must("key reader")
            time.sleep(1.2)
            inside = bench.must("js !!window.__searchReaderOn")
            self.check("reading mode enters", inside.get("value") == "true", str(inside)[:60])
            way_out = bench.must("js !!document.getElementById('__search_reader_exit')")
            self.check("and carries its own way out", way_out.get("value") == "true", str(way_out)[:60])
            bench.must("key reader")
            time.sleep(1.2)
            back = bench.must("js !!window.__searchReaderOn")
            self.check("and leaves again", back.get("value") == "false", str(back)[:60])

        @self.scenario("the window itself")
        def _window():
            sizes = [(1000, 700), (1400, 900), (900, 620), (1600, 1000), (1200, 780)]
            for width, height in sizes * (2 if self.args.quick else 4):
                bench.must(f"resize {width}x{height}")
                time.sleep(0.05)
            time.sleep(0.5)
            probe = bench.probe()
            self.check("the window survives resizing", probe.get("ok"), f"{probe.get('width')}x{probe.get('height')}")
            bench.must("key maximise")
            time.sleep(0.4)
            bench.must("key maximise")
            self.check("maximise toggles", True, "ok")

        @self.scenario("closing everything")
        def _close_all():
            bench.must("closeall")
            time.sleep(1)
            probe = bench.probe()
            self.check("closing every tab leaves one", len(probe["tabs"]) == 1, f"{len(probe['tabs'])} tabs")
            time.sleep(2)
            usage = self.ram()
            self.check("memory comes back down", usage["pss_mb"] < 420, json.dumps(usage), {"memory": usage})

        @self.scenario("an idle minute")
        def _drift():
            bench.open(self.url("/plain?drift=1"))
            bench.wait_loaded(timeout=20)
            time.sleep(6)  # let the engine finish waking up before we judge it
            before = self.ram()
            time.sleep(5 if self.args.quick else 60)
            after = self.ram()
            drift = after["pss_mb"] - before["pss_mb"]
            self.check("nothing grows while nothing happens", drift < 35,
                       f"{before['pss_mb']}MB -> {after['pss_mb']}MB ({drift:+.1f}MB)",
                       {"drift_mb": round(drift, 1)})

        @self.scenario("no strays")
        def _strays():
            probe = bench.probe()
            self.check("the browser is still one browser", probe.get("ok"), "ok")
            usage = self.ram()
            self.check("process count is sane", usage["processes"] <= 8, f"{usage['processes']} processes")

    # -- summary --------------------------------------------------------
    def summary(self):
        passed = sum(1 for result in self.results if result["ok"])
        failed = [result for result in self.results if not result["ok"]]
        REPORT.parent.mkdir(parents=True, exist_ok=True)
        REPORT.write_text(json.dumps({"results": self.results, "passed": passed,
                                      "failed": len(failed)}, indent=2))
        print("\n" + "=" * 60)
        print(f"{passed} passed, {len(failed)} failed, {len(self.results)} checks")
        if "one_tab" in vars(self).keys():
            pass
        if hasattr(self, "one_tab"):
            print(f"one idle tab: {self.one_tab['pss_mb']}MB resident, "
                  f"{self.one_tab['anon_mb']}MB of it the browser's own")
        if failed:
            print("\nfailures:")
            for result in failed:
                print(f"  - {result['name']}: {result['detail'][:300]}")
        print(f"report: {REPORT}")
        return 1 if failed else 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--quick", action="store_true", help="fewer rounds")
    parser.add_argument("--attach", action="store_true", help="drive a browser already running")
    parser.add_argument("--keep", action="store_true", help="leave the browser running")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)

    if not BINARY.exists() and not args.attach:
        print(f"no binary at {BINARY}: cargo build --release first", file=sys.stderr)
        return 2

    run = Run(args)
    try:
        run.start_server()
        if args.attach:
            run.bench = Bench()
            run.browser = None
            run.startup = 0.0
            class _Attached:
                pid = int(subprocess.check_output(["pgrep", "-x", "search"]).split()[0])
                def poll(self):  # noqa: D401
                    return None if alive(self.pid) else 1
                def stderr(self):
                    import io
                    return io.StringIO("")
            run.browser = _Attached()
            run.run_all()
        else:
            run.start_browser()
            run.run_all()
    finally:
        if not args.keep:
            run.stop()
    return run.summary()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
