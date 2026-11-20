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
    pss = anon = rss = shmem = dirty = 0
    processes = 0
    for pid in trees:
        try:
            with open(f"/proc/{pid}/smaps_rollup", "r") as handle:
                for line in handle:
                    if line.startswith("Pss:"):
                        pss += int(line.split()[1])
                    elif line.startswith("Pss_Anon:"):
                        anon += int(line.split()[1])
                    elif line.startswith("Pss_Shmem:"):
                        shmem += int(line.split()[1])
                    elif line.startswith("Private_Dirty:"):
                        dirty += int(line.split()[1])
                    elif line.startswith("Rss:"):
                        rss += int(line.split()[1])
            processes += 1
        except OSError:
            continue
    # Pss_Anon is "the browser's own memory" and it is the figure the promise is
    # made about, but it is blind to shared-memory pages — which is exactly
    # where a page's buffers can land once the DMA-BUF renderer is off. Naming
    # shmem and private-dirty beside it means the report shows what anon leaves
    # out instead of letting a reader assume it is the whole story.
    return {
        "pss_mb": round(pss / 1024, 1),
        "anon_mb": round(anon / 1024, 1),
        "shmem_mb": round(shmem / 1024, 1),
        "private_dirty_mb": round(dirty / 1024, 1),
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
        # One name for the folder the suite's downloads land in, so a check can
        # say whether a file landed there or in the person's real Downloads.
        self.downloads = self.tmp / "downloads"
        self.rounds = 6 if args.quick else 20
        self.pressure = {}

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

    def start_browser(self, fresh=True):
        env = dict(os.environ)
        env["SEARCH_BENCH"] = "1"
        # Assigned, not defaulted: an XDG_DATA_HOME inherited from the shell
        # would hand this run the profile it is meant to be independent of, and
        # its history would leak into checks that ask whether something was
        # written down or offered.
        env["XDG_CONFIG_HOME"] = str(self.tmp / "config")
        env["XDG_DATA_HOME"] = str(self.tmp / "data")
        # A download that lands in the real ~/Downloads can pass on a file an
        # earlier run left there. Give the suite its own folder.
        self.downloads.mkdir(exist_ok=True)
        env["XDG_DOWNLOAD_DIR"] = str(self.downloads)
        env.pop("DISPLAY", None)
        env["GDK_BACKEND"] = "wayland"
        started = time.time()
        argv = [str(BINARY), "--appearance", "light"]
        if fresh:
            # --fresh means "do not restore", and it is the right start for a
            # test that wants a known state. A restart that is meant to bring
            # the session back must not pass it, or it tests nothing.
            argv.append("--fresh")
        self.browser = subprocess.Popen(
            argv,
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

    def restart(self):
        """Close the browser and start it again on the same config and data."""
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
        self.bench = None
        self.start_browser(fresh=False)
        return self.bench

    def restart_with_prefs(self, path, prefs):
        """Quit, write prefs while nothing is running to overwrite them, start."""
        if self.bench:
            try:
                self.bench.command("quit")
            except Exception:
                pass
        if self.browser and self.browser.poll() is None:
            try:
                self.browser.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.browser.terminate()
                try:
                    self.browser.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.browser.kill()
        self.bench = None
        path.write_text(json.dumps(prefs))
        self.start_browser(fresh=False)
        return self.bench

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
            # A check that asserts True asserts nothing: these two now assert
            # what the browser reported, so a launch that answers nothing fails.
            # A deadline of 30s makes "< 30" a check that cannot fail: the
            # browser is only reachable if it answered inside it. So assert a
            # number that is a claim, and assert the browser drew something.
            self.check("bench is up", self.startup < 8, f"ready in {self.startup:.1f}s")
            ram = self.ram()
            self.check("first paint", ram.get("anon_mb", 0) > 5 and ram.get("pss_mb", 0) > 40,
                       json.dumps(ram)[:110])
            probe = bench.probe()
            self.check("probe", probe.get("active", 0) > 0 and probe.get("width", 0) > 200,
                       f"{probe.get('width')}x{probe.get('height')}, {len(probe.get('tabs', []))} tabs")

        @self.scenario("what the last critique found")
        def _critique():
            # This one runs before anything is loaded, because two of its
            # checks need a browser that has been nowhere: a field with no
            # offers under it and no bookmarks in the list. It says so if that
            # is not the state it got, rather than quietly measuring something
            # else.
            bench.must("forget")
            bench.must("closeall")
            time.sleep(0.8)

            def at(omni):
                # "x,y" in window coordinates, or -1 for a window that has not
                # been laid out, which is not a geometry to compare.
                x, _, y = (omni.get("fieldAt") or "-1,-1").partition(",")
                try:
                    return int(x), int(y)
                except ValueError:
                    return -1, -1

            def settled():
                # GTK allocates an overlay child in the pass after the one that
                # shows it, so a field raised by a keystroke has no position
                # until the window has been round the loop again. Ask the
                # browser to finish that pass rather than sleeping a guess.
                omni = {}
                for _ in range(30):
                    bench.settle()
                    omni = bench.must("debug")["omni"]
                    if omni.get("mapped") and at(omni)[1] > 0:
                        break
                    time.sleep(0.1)
                return omni

            # The field is anchored from the window height alone, so an empty
            # list and a full one leave it standing on the same line. A field
            # that re-centres on what is under it moves under the caret while
            # the list arrives.
            bench.must("key omni")
            empty = settled()
            bench.must("escape")
            time.sleep(0.3)

            plain = bench.open(self.url("/plain?critique=1"))
            bench.wait_loaded(plain, timeout=20)
            time.sleep(0.4)
            bench.must("key omni")
            filled = settled()
            probe = bench.probe()
            empty_x, empty_y = at(empty)
            filled_x, filled_y = at(filled)
            self.check(
                "the field stands still when the list fills",
                empty_y > 0
                and empty["offers"] == 0
                and filled["offers"] > 0
                and filled["listVis"] is True
                and filled_y == empty_y
                and filled_x == empty_x,
                f"{empty['offers']} offers at ({empty_x},{empty_y}) mapped {empty['mapped']}, "
                f"{filled['offers']} offers at ({filled_x},{filled_y}) mapped {filled['mapped']}",
                {"empty_at": list(at(empty)), "filled_at": list(at(filled)),
                 "empty_offers": empty["offers"], "filled_offers": filled["offers"]},
            )

            # The chip beside the field already names the site, so the field
            # holds the part the chip cannot: the path. "127.0.0.1" twice on
            # one line is the browser reading its own address back to you.
            self.check(
                "the field does not say the host twice",
                "http://" not in probe["omniText"]
                and probe["omniText"] != self.url("/plain?critique=1")
                and probe["omniText"].endswith("/plain?critique=1"),
                f"field holds {probe['omniText']!r}",
            )
            bench.must("escape")
            time.sleep(0.3)

            # A page that failed is the browser's own page, and the keyboard
            # has to get out of it: Ctrl L is the way out of every page.
            missing = bench.open(self.url("/missing"))
            bench.wait_loaded(missing, timeout=15)
            time.sleep(0.5)
            failed = bench.probe()
            bench.must("key omni")
            time.sleep(0.4)
            escaped = bench.probe()
            self.check(
                "a 404 can be left with the keyboard",
                failed["visibleChild"] == "error" and escaped["omni"] is True,
                f"showing {failed['visibleChild']}, then Ctrl L gave omni={escaped['omni']}",
            )
            bench.must("escape")
            time.sleep(0.3)

            # The failed tab remembers its error; it must not paint it over the
            # neighbour you are looking at. Selecting the good tab has to be
            # both the tab bar's answer and the page on screen.
            bench.select(plain)
            time.sleep(0.8)
            probe = bench.probe()
            text = bench.text()
            body = text.get("text") or ""
            self.check(
                "a failed tab does not paint over another",
                probe["active"] == plain
                and probe["visibleChild"] == f"tab-{probe['active']}"
                and "quietest page" in body,
                f"active {probe['active']} showing {probe['visibleChild']}, "
                f"page text {body[:40]!r}",
            )

            bench.open(self.url("/office"))
            bench.wait_loaded()
            bench.must("key history")
            bench.settle()
            widgets = bench.must("paneltree").get("widgets", [])
            filters = [w for w in widgets if w["kind"] == "GtkEntry"]
            self.check("the history filter has visible geometry",
                       len(filters) == 1 and filters[0]["size"][0] > 400 and filters[0]["size"][1] > 20,
                       filters)
            bench.must("key closepanel")

            # The sheet the browser draws is keys::SHEET, and paneltext walks
            # the labels it drew: a panel that opened empty would pass a check
            # on `probe` alone, so the rows are read rather than assumed.
            bench.must("key shortcuts")
            time.sleep(0.5)
            panel = bench.probe()
            sheet = bench.must("paneltext")
            rows = [line for line in (sheet.get("text") or "").split("\n") if line.strip()]
            self.check("the Shortcuts panel lists keys that work",
                panel["panel"] == "Shortcuts"
                and sheet.get("panel") == "Shortcuts"
                and "Ctrl L" in rows
                and "The field" in rows
                and "F3" in rows
                and "Shift F3" in rows
                and len(rows) >= 40,
                f"panel {panel['panel']}, {len(rows)} labels, "
                f"Ctrl L listed: {'Ctrl L' in rows}, "
                f"F3 listed: {'F3' in rows}, Shift F3 listed: {'Shift F3' in rows}",
                {"panel_rows": len(rows)},
            )
            geometry = bench.must("panelgeometry")
            self.check("the Shortcuts panel has a real scroll range",
                       geometry["body"][0] > 500 and geometry["upper"] > geometry["page"], geometry)
            reached = bench.must("panelscroll 10000")
            bench.settle()
            bottom = bench.must("panelscroll 0")
            self.check("the Shortcuts panel reaches its last row",
                       bottom["value"] > 0 and abs(bottom["value"] - (bottom["upper"] - bottom["page"])) <= 1,
                       bottom)
            bench.must("panelscroll -10000")
            bench.must("key closepanel")
            # Nothing this scenario opened is left for the next one to measure
            # an idle browser against, and the engine needs a moment to retire
            # the two processes those tabs owned before the memory check runs.
            bench.must("closeall")
            time.sleep(2)

        @self.scenario("a single idle tab")
        def _idle():
            bench.open(self.url("/plain"))
            bench.wait_loaded(timeout=20)
            time.sleep(3)
            usage = self.ram()
            # 120MB of the browser's own memory is the promise, and a run of
            # one idle tab measures 49-119MB, so the threshold sits at the top
            # of the measured range rather than above it. Resident memory is
            # the engine's shared pages as well, and 260MB is what an idle tab
            # honestly costs with them included.
            self.check(
                "idle one tab",
                usage["anon_mb"] < 120 and usage["pss_mb"] < 260 and usage["processes"] <= 5,
                f"{json.dumps(usage)} (one idle tab measures 49-119MB of its own memory)",
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
            # A per-tab figure that is printed but never asserted is a figure
            # with nothing behind it: a tab costing 200MB each would pass. The
            # measured cost is about 21MB, so 40 is a bound that a real
            # regression crosses and a noisy host does not.
            self.check(
                f"{len(ids)} tabs open",
                len(probe["tabs"]) >= len(ids) and per_tab < 40,
                f"{len(probe['tabs'])} tabs, +{per_tab:.1f}MB per tab, total {after['pss_mb']}MB",
                {"per_tab_mb": round(per_tab, 1), "memory": after},
            )
            for tab_id in ids:
                bench.close(tab_id)
            time.sleep(1)
            after_close = self.ram()
            # Giving memory back means the number came down, not that it stayed
            # inside a tolerance of its own value.
            # Back to where it started, not merely lower than its own peak:
            # "came down by 100MB" lets a 300MB leak pass. The baseline is the
            # reading taken before any of these tabs existed.
            self.check(
                "tabs give their memory back",
                after_close["pss_mb"] < before["pss_mb"] + 60,
                f"{after_close['pss_mb']}MB after closing (was {after['pss_mb']}MB, "
                f"started at {before['pss_mb']}MB)",
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
            ordered = sorted(timings)
            # p95 from the standard library, not a dependency: 300 switches
            # hide three slow ones in the tail, and the mean alone would call
            # that quick. 60ms is a switch you cannot feel, 500ms is one you
            # can.
            p95 = statistics.quantiles(ordered, n=20)[18]
            self.check(
                "tab switches stay quick",
                average < 60 and worst < 500,
                f"mean {average:.0f}ms, p95 {p95:.0f}ms, worst {worst:.0f}ms over {len(timings)}",
                {"mean_ms": round(average, 1), "p95_ms": round(p95, 1),
                 "worst_ms": round(worst, 1)},
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
            opened = []
            for path, timeout in cases:
                started = time.time()
                opened.append(bench.open(self.url(path)))
                bench.wait_loaded(timeout=timeout)
                took = time.time() - started
                # wait_loaded raises on timeout, so "took < timeout" is always
                # true. Assert the page really rendered: words for a text page,
                # elements for a page of pictures or frames, which have neither.
                # It is asked in JS rather than through `text`, because `text`
                # waits on the page and a page of 40 images can outlast it.
                # The fixture serves 40 tiles; assert what it serves, not a
                # number that was written down before the page existed.
                if path == "/images":
                    probe_js = "js document.images.length"
                    wanted = 40
                elif path == "/frames":
                    probe_js = "js document.querySelectorAll('iframe').length"
                    wanted = 12
                else:
                    probe_js = "js (document.body.textContent||'').length"
                    wanted = 40
                # The DOM is empty for a moment after `load_changed` says
                # Finished, so the first answer is a legitimate-looking 0.
                # Keep asking until one answer passes, and treat 0 as "ask
                # again" rather than as a result.
                seen = 0
                for _ in range(12):
                    answer = bench.must(probe_js).get("value") or "0"
                    seen = int(answer) if answer.isdigit() else 0
                    if seen >= wanted:
                        break
                    time.sleep(0.25)
                self.check(
                    f"loads {path}",
                    seen >= wanted,
                    f"{took:.1f}s, {seen} on the page",
                    {"seconds": round(took, 2)},
                )
            probe = bench.probe()
            self.check("alive after hard pages",
                       len(probe["tabs"]) >= len(cases)
                       and all(tab["hasView"] for tab in probe["tabs"] if tab["url"]),
                       f"{len(probe['tabs'])} tabs, "
                       f"{sum(1 for tab in probe['tabs'] if tab['hasView'])} with a view")
            # Five pages of a few megabytes each are not something to leave
            # standing for the next scenario to measure the browser against.
            for tab_id in opened:
                bench.close(tab_id)

        @self.scenario("pages that misbehave")
        def _bad():
            tab = bench.open(self.url("/missing"))
            bench.wait_loaded(tab, timeout=15)
            time.sleep(0.4)
            probe = bench.probe()
            self.check("a 404 shows the browser's own page",
                       probe["visibleChild"] == "error",
                       f"showing {probe['visibleChild']}, active {probe['active']}")
            self.check("a 404 shows, the browser lives",
                       len(probe["tabs"]) >= 1
                       and all(tab["hasView"] for tab in probe["tabs"] if tab["url"]),
                       f"{len(probe['tabs'])} tabs, "
                       f"{sum(1 for tab in probe['tabs'] if tab['hasView'])} with a view")

            redirect = bench.open(self.url("/redirect?n=6"))
            landed = bench.wait_for(lambda probe: probe["url"].endswith("/plain"), timeout=25)
            self.check("a redirect chain lands", landed["url"].endswith("/plain"), landed["url"])

            # Surviving a loop means: the tab exists, the browser answers fast,
            # and the engine has stopped asking. The address of a loop that
            # never resolves is not something the browser can report.
            loop_tab = bench.open(self.url("/loop"))
            time.sleep(3)
            started = time.time()
            probe = bench.probe()
            answered = time.time() - started
            alive = [tab for tab in probe["tabs"] if tab["id"] == loop_tab]
            self.check(
                "a redirect loop is survived",
                bool(alive) and answered < 3,
                f"{len(alive)} tab for the loop, answered in {answered:.1f}s",
            )

            stall = bench.open(self.url("/stall"))
            time.sleep(3)
            started = time.time()
            bench.probe()
            self.check("a page that never ends does not hang the browser", time.time() - started < 3,
                       f"probe answered in {time.time() - started:.1f}s")

            busy = bench.open(self.url("/js-busy"))
            time.sleep(2)
            started = time.time()
            other = bench.open(self.url("/plain?after-busy=1"))
            bench.wait_loaded(other, timeout=20)
            took = time.time() - started
            titles = {tab["id"]: tab for tab in bench.probe()["tabs"]}
            self.check(
                "a busy script does not stop other tabs",
                took < 20 and titles.get(busy, {}).get("hasView") is True,
                f"other tab loaded in {took:.1f}s, busy tab still here: {busy in titles}",
            )

            # The reading to come back to is this one, taken with the same six
            # tabs open that are still open after the spray closes. Twice one
            # idle tab was the wrong bar: it compared a browser carrying six
            # live tabs, two of them pages that never finish, with a browser
            # carrying one, and sat within 1% of the measured 420-426 MB on
            # every run, so it failed on noise and not on a leak.
            before_spray = self.ram()
            spray = bench.open(self.url("/js-leak"))
            time.sleep(3)
            usage = self.ram()
            self.check("a memory spray is survived", usage["pss_mb"] < 2600, json.dumps(usage))
            bench.close(spray)
            time.sleep(4)
            after = self.ram()
            # "Lower than the peak plus 50" lets a browser still sitting at the
            # peak pass. The bound that means something is the idle reading: a
            # browser that gave the sprayed tab back should be within a factor
            # of two of one idle tab, not within a rounding error of its own
            # worst moment.
            self.check("the engine survives a spray and keeps working",
                       bench.probe().get("ok") and after["pss_mb"] < before_spray["pss_mb"] + 60,
                       f"{usage['pss_mb']}MB -> {after['pss_mb']}MB, still answering "
                       f"(was {before_spray['pss_mb']}MB before the spray)",
                       {"memory": after, "before": before_spray})
            # This scenario opens seven tabs, several of them pages that never
            # finish. A later check about an idle browser must not measure
            # those, so they go now.
            for tab_id in (tab, redirect, loop_tab, stall, busy, other):
                bench.close(tab_id)
            time.sleep(1)

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
            # The mean says the usual panel is quick. The worst is recorded
            # beside it because a mean hides the one open that took a second and
            # a half, and that is the one a person remembers.
            panel_mean, panel_worst = statistics.mean(timings), max(timings)
            self.check("panels open and close", panel_mean < 200,
                       f"mean {panel_mean:.0f}ms, worst {panel_worst:.0f}ms over {len(timings)}",
                       {"mean_ms": round(panel_mean, 1), "worst_ms": round(panel_worst, 1)})

            # `omni` is "go to this address"; the field is opened by `key omni`.
            bench.must("key omni")
            time.sleep(0.4)
            probe = bench.probe()
            self.check("the field opens on the address you are on",
                       probe["omniText"].endswith("/long"),
                       f"field holds {probe['omniText'][-24:]!r}")
            bench.must("key down")
            time.sleep(0.3)
            once = bench.probe()
            bench.must("key down")
            time.sleep(0.3)
            twice = bench.probe()
            # "Picked something" is true of a list that has never been stepped,
            # so the assertion is that the pick *moved*, twice, from nothing.
            self.check("down moves the pick through the list",
                       once["omniCount"] > 1
                       and once["omniPicked"] == 0
                       and twice["omniPicked"] == 1,
                       f"picked {once['omniPicked']} then {twice['omniPicked']} "
                       f"of {once['omniCount']}")
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
            # Find is a question about the page on screen, and the panel checks
            # above left this tab on /plain. Go back to the long read first, or
            # the search is fair and finds nothing.
            bench.must(f"go {self.url('/long')}")
            bench.wait_loaded(timeout=20)
            time.sleep(0.4)
            answer = bench.command("find Paragraph 5")
            self.check("find searches", answer.get("ok"), str(answer)[:80])
            time.sleep(1.5)
            probe = bench.probe()
            self.check("find bar state", probe["find"] is True, "open")
            # A find that matches nothing also "succeeds". The count is the
            # evidence that the engine was asked and answered.
            self.check("find counts its matches",
                       probe.get("findCount", "").endswith("matches")
                       and probe["findCount"] != "no matches",
                       f"count {probe.get('findCount')!r}")
            # F3 and Shift F3 are rows in the sheet, and a bench verb goes
            # through `keys::intent_of`, so these press the panel's own
            # shortcut rather than find_step. Stepping leaves the count alone,
            # so the page moving is the evidence a match was taken.
            start = float(bench.must("js window.scrollY").get("value") or 0)
            bench.must("key findnext")
            time.sleep(0.2)
            next_y = float(bench.must("js window.scrollY").get("value") or 0)
            self.check("F3 steps to the next match",
                       next_y > start and bench.probe()["find"] is True,
                       f"scrollY {start:.0f} to {next_y:.0f}")
            bench.must("key findprevious")
            time.sleep(0.2)
            back_y = float(bench.must("js window.scrollY").get("value") or 0)
            self.check("Shift F3 steps back to the previous match",
                       back_y < next_y, f"scrollY {next_y:.0f} to {back_y:.0f}")
            bench.must("escape")

            bench.must("key reader")
            time.sleep(1.5)
            text = bench.text()
            # A container that exists is not a page that was reset: measure that
            # it is on screen and that the article's own headline has moved
            # inside it, at the size the reader chose.
            style = bench.must(
                "js (() => { const r = document.getElementById('__search_reader');"
                " if (!r || r.getBoundingClientRect().width < 300) return 'narrow';"
                " const h = r.querySelector('h1'); if (!h) return 'no headline';"
                " const size = parseFloat(getComputedStyle(h).fontSize);"
                " return size >= 28 ? 'set' : 'small:' + size; })()"
            )
            # Keeping the words is not the claim: the claim is that the browser
            # set the article again in its own type, in its own container.
            self.check("reading mode replaces the page",
                       (text.get("text") or "") != "" and style.get("value") == "set",
                       f"text {len(text.get('text') or '')} chars, container {style.get('value')}")
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
            # After a reload the old document is gone for a moment; ask until
            # the element is there to be measured, or the answer is a
            # TypeError about null rather than a verdict about the strip.
            still = {"value": "absent"}
            for _ in range(40):
                still = bench.must(
                    "js (() => { const e = document.querySelector('#hide-me');"
                    " return e ? getComputedStyle(e).display : 'absent'; })()"
                )
                if still.get("value") != "absent":
                    break
                time.sleep(0.25)
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
            # "at least two tabs" is true of a browser that had two already.
            # The claim is that the pop-up made exactly one more.
            self.check("a pop-up becomes a tab",
                       len(probe["tabs"]) == len(before["tabs"]) + 1,
                       f"{len(before['tabs'])} -> {len(probe['tabs'])} tabs")
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
            for _ in range(30):
                downloads = bench.must("downloads").get("downloads", [])
                if downloads and all(item.get("done") for item in downloads):
                    break
                time.sleep(0.3)
            downloads = bench.must("downloads").get("downloads", [])
            self.check("a download is recorded and finishes",
                       bool(downloads) and all(item.get("done") for item in downloads),
                       str(downloads)[:140])
            path = Path(downloads[0]["path"]) if downloads else Path("/nonexistent")
            for _ in range(40):
                if path.exists() and path.stat().st_size > 0:
                    break
                time.sleep(0.2)
            # Landing somewhere is not landing in the right place: a run that
            # writes into the person's own ~/Downloads leaves a file behind and
            # can pass on a file an earlier run put there.
            inside = self.args.attach
            if not inside:
                try:
                    inside = self.downloads.resolve() in Path(path).resolve().parents
                except OSError:
                    inside = False
            self.check(
                "the file landed",
                path.exists() and path.stat().st_size > 0 and inside,
                f"{path} {path.stat().st_size if path.exists() else 0} bytes, "
                f"inside the suite's folder: {inside}",
            )

        @self.scenario("the blocker runs before the page")
        def _blocker():
            # The rule list is a promise; this is the page that measures it.
            tab = bench.open(self.url("/banner"))
            bench.wait_loaded(tab, timeout=20)
            time.sleep(0.6)
            seen = bench.must("js window.__probe()")
            try:
                state = json.loads(seen.get("value") or "{}")
            except json.JSONDecodeError:
                state = {}
            self.check("the blocker hides what it says it hides",
                       state.get("keep") is True
                       and state.get("banner") is False
                       and state.get("ad") is False
                       and state.get("news") is False,
                       json.dumps(state))
            # turning it off for a host has to change the page, not just a file
            bench.must("blocker")
            time.sleep(1.0)
            bench.must(f"go {self.url('/banner?off=1')}")
            bench.wait_loaded(timeout=20)
            time.sleep(0.8)
            off = json.loads(bench.must("js window.__probe()").get("value") or "{}")
            self.check("turning it off for a site brings the rules back",
                       off.get("banner") is True and off.get("ad") is True,
                       json.dumps(off))
            bench.must("blocker")
            time.sleep(1.0)
            bench.must(f"go {self.url('/banner?on=1')}")
            bench.wait_loaded(timeout=20)
            time.sleep(0.8)
            on = json.loads(bench.must("js window.__probe()").get("value") or "{}")
            self.check("and turning it on again hides them",
                       on.get("banner") is False,
                       json.dumps(on))

        @self.scenario("a private tab")
        def _private():
            # Its own cookie jar, and nothing of it left afterwards.
            ordinary = bench.open(self.url("/cookie?set=1"))
            bench.wait_loaded(ordinary, timeout=20)
            time.sleep(0.4)
            bench.must(f"go {self.url('/cookie')}")
            bench.wait_loaded(ordinary, timeout=20)
            time.sleep(0.4)
            shared = bench.must("js document.cookie.includes('flavour')")
            self.check("an ordinary tab keeps a cookie",
                       shared.get("value") == "true", str(shared)[:60])

            bench.must("key privatetab")
            time.sleep(0.6)
            probe = bench.probe()
            private = probe["active"]
            self.check("a private tab is marked private",
                       any(tab["id"] == private and tab["private"] for tab in probe["tabs"]),
                       f"tab {private}")
            bench.must(f"go {self.url('/cookie')}")
            bench.wait_loaded(private, timeout=20)
            time.sleep(0.4)
            jar = bench.must("js document.cookie.includes('flavour')")
            self.check("and starts with an empty jar", jar.get("value") == "false", str(jar)[:60])
            bench.must("key privatetab")
            time.sleep(0.4)
            # A private tab's address is the one thing it promised not to keep.
            bench.must(f"go {self.url('/titled?t=SecretVisit')}")
            bench.wait_loaded(private, timeout=20)
            bench.must("key privatetab")
            time.sleep(0.5)
            history = self.tmp / "data" / "search" / "history.json"
            written = history.read_text() if history.exists() else ""
            self.check("a private visit is not written down",
                       "SecretVisit" not in written,
                       f"{len(written)} bytes of history")

            # A window a private tab opens is a private tab.
            bench.must("key privatetab")
            time.sleep(0.5)
            probe = bench.probe()
            owner = probe["active"]
            bench.must(f"go {self.url('/popup')}")
            bench.wait_loaded(owner, timeout=20)
            time.sleep(0.5)
            bench.must("js document.getElementById('go').click()")
            time.sleep(1.5)
            after = bench.probe()
            born = [tab for tab in after["tabs"] if tab["id"] not in {t["id"] for t in probe["tabs"]}]
            self.check("a window a private tab opens is private too",
                       bool(born) and all(tab["private"] for tab in born),
                       f"{len(born)} new tabs, private: {[t['private'] for t in born]}")
            fork = bench.must("js document.cookie.includes('flavour')")
            self.check("two private tabs share nothing with the ordinary one",
                       fork.get("value") == "false", str(fork)[:60])
            # Five tabs, two of them private, none of them the next scenario's
            # business: a private tab left open is also a promise this suite
            # has no way to check.
            bench.must("closeall")
            time.sleep(0.6)

        @self.scenario("what a fresh start remembers")
        def _session():
            # The restart replaces the socket; every later scenario reads the
            # same name, so it has to point at the new browser.
            nonlocal bench
            # Tabs come back, and the ones in the background come back asleep:
            # a restored tab should cost nothing until it is clicked. The tab
            # in front is woken at start, because the window has to show a
            # page and not the blank pane, so the check needs a second tab.
            front = bench.open(self.url("/titled?t=Front"))
            bench.wait_loaded(front, timeout=20)
            tab = bench.open(self.url("/titled?t=Remembered"))
            bench.wait_loaded(tab, timeout=20)
            bench.must("key bookmark")     # something to notice afterwards
            bench.must(f"select {front}")
            bench.must("session")          # the session write is debounced; ask now
            bench = self.restart()
            time.sleep(1)
            probe = bench.probe()
            urls = [t["url"] for t in probe["tabs"]]
            self.check("a fresh start brings the tabs back",
                       any("/titled?t=Remembered" in url for url in urls),
                       f"{len(urls)} tabs")
            asleep = [t["asleep"] for t in probe["tabs"]
                      if "/titled?t=Remembered" in t["url"] and t["id"] != probe["active"]]
            self.check("and they come back asleep",
                       bool(asleep) and all(asleep), f"asleep {asleep}")
            shown = [t for t in probe["tabs"] if t["id"] == probe["active"]]
            self.check("except the one in front, which shows its page",
                       bool(shown) and not shown[0]["asleep"]
                       and probe.get("visibleChild") == f"tab-{probe['active']}",
                       f"active {probe['active']} -> {probe.get('visibleChild')}")

        @self.scenario("reading mode")
        def _reading():
            # The page's own words, set again by the browser: it must enter, and
            # it must carry a way out that does not need a keyboard.
            tab = bench.open(self.url("/long"))
            bench.wait_loaded(tab, timeout=20)
            bench.must("escape")
            time.sleep(0.2)
            # Read from partway down the page, not from the top. A restore that
            # always lands on 0 looks correct at 0 and is only wrong here.
            start_y = 200
            bench.must(f"js window.scrollTo(0, {start_y})")
            time.sleep(0.1)
            bench.must("js (() => { const p = document.createElement('p'); p.className = 'byline'; p.textContent = 'By Ada Lovelace'; document.querySelector('article').appendChild(p); return true; })()")
            bench.must("key reader")
            time.sleep(1.2)
            inside = bench.must("js !!window.__searchReaderOn")
            self.check("reading mode enters", inside.get("value") == "true", str(inside)[:60])
            self.check("the browser knows which tab is being read", bench.probe().get("reading") is True, str(bench.probe()))
            way_out = bench.must("js !!document.getElementById('__search_reader_exit')")
            self.check("and carries its own way out", way_out.get("value") == "true", str(way_out)[:60])
            contrast = bench.must("js (() => { const b = document.querySelector('#__search_reader > .byline'); if (!b) return '0'; const ch = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }; const lum = (s) => { const m = (s.match(/[0-9]+/g) || []).map(Number); return 0.2126 * ch(m[0]) + 0.7152 * ch(m[1]) + 0.0722 * ch(m[2]); }; const x = lum(getComputedStyle(document.body).backgroundColor), y = lum(getComputedStyle(b).color); return ((Math.max(x,y) + 0.05) / (Math.min(x,y) + 0.05)).toFixed(2); })()")
            self.check("reader byline has measured contrast", float(contrast.get("value") or 0) >= 6.0, str(contrast))
            # The page's own button, clicked as a page clicks it. Toggling from
            # the host would prove only that the host can leave; this is the
            # way out a keyboardless user has, and it has to reach the host too.
            bench.must("js document.getElementById('__search_reader_exit').click()")
            time.sleep(0.3)
            clicked = bench.must("js !!window.__searchReaderOn")
            self.check("the page's exit button leaves reading mode",
                       clicked.get("value") == "false", str(clicked)[:60])
            self.check("and the browser is told the page left it",
                       bench.probe().get("reading") is False, str(bench.probe()))
            scrolled = bench.must("js window.scrollY")
            self.check("the exit button puts the page back where it was read from",
                       abs(float(scrolled.get("value") or 0) - start_y) <= 2,
                       f"scrollY {scrolled.get('value')!r}, was {start_y}")
            # Back in by the keyboard, and out by it: the toggle the host owns
            # still works after the page has left reading mode on its own.
            bench.must("key reader")
            time.sleep(1.2)
            again = bench.must("js !!window.__searchReaderOn")
            self.check("reading mode enters again after the page left it",
                       again.get("value") == "true", str(again)[:60])
            bench.must("key reader")
            time.sleep(1.2)
            back = bench.must("js !!window.__searchReaderOn")
            self.check("and leaves again", back.get("value") == "false", str(back)[:60])
            self.check("the browser knows reading ended", bench.probe().get("reading") is False, str(bench.probe()))
            restored = bench.must("js [!!document.querySelector('style[data-search-reader]'), window.scrollY]")
            self.check("leaving restores the page",
                       str(restored.get("value")).startswith("false,"), str(restored)[:80])

        @self.scenario("the window itself")
        def _window():
            sizes = [(1000, 700), (1400, 900), (900, 620), (1600, 1000), (1200, 780)]
            for width, height in sizes * (2 if self.args.quick else 4):
                bench.must(f"resize {width}x{height}")
                time.sleep(0.05)
            time.sleep(0.5)
            probe = bench.probe()
            # The desktop adds its own frame, so the inner size is a few pixels
            # shy of the request. What matters is that the size was taken.
            width, height = probe.get("width", 0), probe.get("height", 0)
            self.check("the window survives resizing",
                       abs(width - 1200) <= 40 and abs(height - 780) <= 40,
                       f"{width}x{height}, asked for 1200x780")
            # The window taking the size is not the page taking it. A resize
            # that reaches the frame but not the view leaves the page painted
            # at the old size inside a bigger window, which the window's own
            # numbers cannot show.
            bench.settle()
            probe = bench.probe()
            vw, vh = probe.get("viewWidth", 0), probe.get("viewHeight", 0)
            self.check("the page fills the resized window",
                       vw > width - 300 and abs(vh - (height - 23)) <= 40,
                       f"view {vw}x{vh} inside {width}x{height}")
            before = bench.probe()["maximised"]
            bench.must("key maximise")
            time.sleep(0.4)
            toggled = bench.probe()["maximised"]
            bench.must("key maximise")
            time.sleep(0.4)
            back = bench.probe()["maximised"]
            self.check("maximise toggles", toggled != before and back == before,
                       f"{before} -> {toggled} -> {back}")

        @self.scenario("closing everything")
        def _close_all():
            bench.must("closeall")
            time.sleep(1)
            probe = bench.probe()
            self.check("closing every tab leaves one", len(probe["tabs"]) == 1, f"{len(probe['tabs'])} tabs")
            time.sleep(2)
            usage = self.ram()
            # One tab left, so the bound is the idle promise, not a number
            # chosen to be comfortably above whatever came out.
            self.check("memory comes back down", usage["pss_mb"] < 260,
                       json.dumps(usage), {"memory": usage})

        @self.scenario("an idle minute")
        def _drift():
            bench.open(self.url("/plain?drift=1"))
            bench.wait_loaded(timeout=20)
            time.sleep(6)  # let the engine finish waking up before we judge it
            before = self.ram()
            time.sleep(5 if self.args.quick else 60)
            after = self.ram()
            drift = after["pss_mb"] - before["pss_mb"]
            own_drift = after["anon_mb"] - before["anon_mb"]
            # 35MB a minute is not "nothing grows", it is a leak with a
            # generous allowance. Ten is above the noise of a settled engine
            # and far below anything a reader would call growth. The browser's
            # own share is measured too, because that is the figure the promise
            # is about.
            self.check("nothing grows while nothing happens", drift < 10 and own_drift < 10,
                       f"{before['pss_mb']}MB -> {after['pss_mb']}MB ({drift:+.1f}MB), "
                       f"own {before['anon_mb']}MB -> {after['anon_mb']}MB ({own_drift:+.1f}MB)",
                       {"drift_mb": round(drift, 1), "own_drift_mb": round(own_drift, 1)})

        @self.scenario("extensions")
        def _extensions():
            # The loader is proved in tools/extensions.py; what matters here is
            # that a page loaded in this browser carries no extension at all
            # unless one was installed, so the browser without extensions is
            # still the browser.
            listed = bench.command("extensions").get("extensions", [])
            self.check("a fresh browser has no extensions", listed == [], str(listed))
            tab = bench.open(self.url("/plain?ext=0"))
            bench.wait_loaded(tab_id=tab)
            marked = bench.command(
                "js (() => String(document.documentElement.getAttribute('data-search-ext')))()"
            ).get("value")
            self.check("and no extension touch in the page", marked == "null", str(marked))

        @self.scenario("the shield really compiled")
        def _shield():
            state = {"lists": 0, "finished": 0, "rules": 0}
            for _ in range(30):
                state = bench.command("shield")
                if state.get("finished") == state.get("lists") and state.get("lists"):
                    break
                time.sleep(1)
            self.check(
                "every filter list compiled",
                state.get("lists", 0) > 0 and state.get("finished") == state.get("lists"),
                str(state),
            )
            self.check(
                "and carries the rules the list has",
                state.get("rules", 0) > 2000,
                str(state),
            )

        @self.scenario("the bookmarks bar")
        def _bmarks():
            bench.must("closeall")
            # An earlier scenario kept a page and restarted, so the suite's own
            # profile already holds bookmarks. Start this one from nothing.
            bench.must("forget")
            bench.must("closeall")
            time.sleep(0.5)
            start = bench.probe()
            bar_before = start.get("bookmarkBar", None)
            self.check("a browser with nothing kept hides the bar", bar_before is False, str(bar_before))
            tab = bench.open(self.url("/titled?t=Kept"))
            bench.wait_loaded(tab_id=tab)
            time.sleep(1)
            bench.must("key bookmark")
            time.sleep(1)
            probe = bench.probe()
            self.check("keeping a page shows the bar", probe.get("bookmarkBar") is True, str(probe.get("bookmarkBar")))
            self.check("with one chip in it", probe.get("bookmarkCount") == 1, str(probe.get("bookmarkCount")))

        @self.scenario("a tab that is not looked at lets go of its page")
        def _sleep():
            # The sleep timer had never been exercised by any run: the docs
            # described it and every check measured tabs that were all awake.
            # A minute is the shortest setting, and the tick is a minute, so
            # this waits long enough for one tick to find an old tab.
            # The restart below replaces the socket, and every later scenario
            # reads the same name, so it has to point at the new browser.
            nonlocal bench
            prefs_dir = self.tmp / "config" / "search"
            prefs_dir.mkdir(parents=True, exist_ok=True)
            prefs_path = prefs_dir / "prefs.json"
            try:
                prefs = json.loads(prefs_path.read_text()) if prefs_path.exists() else {}
            except (OSError, ValueError):
                prefs = {}
            prefs["memory_saver"] = True
            prefs["sleep_minutes"] = 0
            # Written while the browser is down, not before quitting it: the
            # window's delete handler saves its own prefs on close, which put
            # the old values back over this file and the tab never slept.
            self.restart_with_prefs(prefs_path, prefs)
            bench = self.bench

            bench.must("closeall")
            keep = bench.open(self.url("/plain"))
            away = bench.open(self.url("/long"))
            bench.must(f"select {keep}")
            time.sleep(3)
            before = self.ram()
            awake = [t for t in bench.probe()["tabs"] if t["id"] == away]
            self.check("the tab starts awake", bool(awake) and not awake[0]["asleep"],
                       f"tab {away} asleep={awake[0]['asleep'] if awake else 'gone'}")
            # The tick is two minutes. Zero minutes means "anything not looked
            # at", so the first tick after the tabs are opened is the one that
            # matters; 130s covers it wherever in the cycle the restart landed.
            time.sleep(130)
            probe = bench.probe()
            slept = [t for t in probe["tabs"] if t["id"] == away]
            after = self.ram()
            self.check("a tab left alone falls asleep",
                       bool(slept) and slept[0]["asleep"] and not slept[0]["hasView"],
                       f"asleep={slept[0]['asleep'] if slept else 'gone'}, "
                       f"hasView={slept[0]['hasView'] if slept else 'gone'}")
            self.check("sleeping gives memory back",
                       after["anon_mb"] < before["anon_mb"],
                       f"{before['anon_mb']}MB -> {after['anon_mb']}MB own")
            # And it wakes up when it is asked for, which is the other half of
            # the promise: a slept tab is still a tab you can click.
            bench.must(f"select {away}")
            time.sleep(2.5)
            woke = [t for t in bench.probe()["tabs"] if t["id"] == away]
            self.check("a slept tab wakes when it is selected",
                       bool(woke) and not woke[0]["asleep"] and woke[0]["hasView"],
                       f"asleep={woke[0]['asleep'] if woke else 'gone'}")
            # Leave the setting as it was found, so later scenarios measure the
            # browser the rest of the suite describes.
            prefs["memory_saver"] = False
            prefs["sleep_minutes"] = 30
            self.restart_with_prefs(prefs_path, prefs)
            bench = self.bench

        @self.scenario("stopped talking about the page that is gone")
        def _no_stale():
            probe = bench.probe()
            self.check("the active tab is the one on screen", probe.get("visibleChild") == f"tab-{probe.get('active')}",
                       f"{probe.get('active')} -> {probe.get('visibleChild')}")

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
        # The numbers the run measured, beside the verdicts it reached: a
        # report of pass/fail alone cannot show a startup that crept or an
        # idle tab that grew. Null when the scenario that measures them never
        # ran, so a missing figure reads as missing and not as zero.
        idle = getattr(self, "one_tab", None) or {}
        REPORT.parent.mkdir(parents=True, exist_ok=True)
        REPORT.write_text(json.dumps({"results": self.results, "passed": passed,
                                      "failed": len(failed),
                                      "startup_seconds": getattr(self, "startup", None),
                                      "idle_rss_mb": idle.get("rss_mb"),
                                      "idle_own_mb": idle.get("anon_mb")}, indent=2))
        print("\n" + "=" * 60)
        print(f"{passed} passed, {len(failed)} failed, {len(self.results)} checks")
        if hasattr(self, "one_tab"):
            print(f"one idle tab: {self.one_tab['pss_mb']}MB resident, "
                  f"{self.one_tab['anon_mb']}MB of it the browser's own")
        if failed:
            print("\nfailures:")
            for result in failed:
                print(f"  - {result['name']}: {str(result['detail'])[:300]}")
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
