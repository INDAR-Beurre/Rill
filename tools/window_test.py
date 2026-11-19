#!/usr/bin/env python3
"""Tests the window itself: is it draggable, do the controls work.

`tools/stress.py` drives the browser's own actions, which cannot tell whether a
real mouse press reaches the window and moves it. This drives the X server with
xdotool instead: the desktop is kwin_wayland, so the browser has to be launched
with GDK_BACKEND=x11 to be visible at all.

    python3 tools/window_test.py

It launches its own browser on a scratch config and leaves nothing behind.
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
BINARY = ROOT / "target" / "release" / "search"
SOCKET = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp")) / "search-bench.sock"

results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("  ok   " if ok else "  FAIL ") + name + (" - " + detail if detail else ""))


class Window:
    """One browser window, spoken to over X and over the bench socket."""

    def __init__(self, home):
        env = dict(os.environ)
        env.update(
            GDK_BACKEND="x11",
            XDG_CONFIG_HOME=str(home / "config"),
            XDG_DATA_HOME=str(home / "data"),
        )
        (home / "config").mkdir(parents=True)
        (home / "data").mkdir(parents=True)
        self.x = {k: env[k] for k in ("DISPLAY", "XAUTHORITY") if k in env}
        SOCKET.unlink(missing_ok=True)
        self.proc = subprocess.Popen(
            [str(BINARY), "--bench", "--fresh"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
        self.id = self.wait_for_window()

    def wait_for_window(self, timeout=25):
        deadline = time.time() + timeout
        while time.time() < deadline:
            found = self.xdotool("search", "--onlyvisible", "--pid", str(self.proc.pid))
            ids = found.split()
            if ids:
                geometry = self.xdotool("getwindowgeometry", "-shell", ids[-1])
                if "WIDTH=10" not in geometry:
                    return ids[-1]
            time.sleep(0.2)
        raise SystemExit("the window never appeared on the X server")

    def xdotool(self, *args):
        out = subprocess.run(
            ["xdotool", *args], env={**os.environ, **self.x}, capture_output=True, text=True
        )
        return out.stdout

    def geometry(self, wid=None):
        shell = self.xdotool("getwindowgeometry", "-shell", wid or self.id)
        return {
            key.lower(): int(value)
            for key, _, value in (line.partition("=") for line in shell.split())
            if value.isdigit()
        }

    def settled(self, timeout=6.0, width=None):
        """Wait until the window has stopped moving and resizing.

        Clicking a control while the compositor is still animating a maximise
        uses coordinates the window no longer has, which reads as a dead
        control.
        """
        deadline = time.time() + timeout
        last = None
        stable = 0
        while time.time() < deadline:
            now = self.geometry()
            if width is not None and now.get("width") != width:
                stable = 0
            elif last == now:
                stable += 1
                if stable >= 3:
                    return now
            else:
                stable = 0
            last = now
            time.sleep(0.25)
        return self.geometry()

    def narrow(self, width=1180):
        """Put the window back to a normal size, whatever state it is in.

        `is_maximized` is the window manager's flag and arranges a beat after
        the resize, so a toggle driven by it can land on the wrong side. Ask
        for the geometry, and try again if it is still wide.
        """
        for _ in range(4):
            now = self.settled(width=width)
            if now["width"] <= width + 60:
                return now
            self.xdotool("mousemove", str(now["x"] + 71), str(now["y"] + 20))
            time.sleep(0.3)
            self.xdotool("click", "1")
            time.sleep(1.0)
        return self.settled(width=width)

    def click(self, x, y):
        self.xdotool("mousemove", str(x), str(y))
        time.sleep(0.25)
        self.xdotool("click", "1")
        time.sleep(0.8)

    def drag(self, x, y, dx, dy, steps=6):
        self.xdotool("mousemove", str(x), str(y))
        time.sleep(0.3)
        self.xdotool("mousedown", "1")
        time.sleep(0.3)
        for step in range(1, steps + 1):
            self.xdotool("mousemove", str(x + dx * step // steps), str(y + dy * step // steps))
            time.sleep(0.1)
        self.xdotool("mouseup", "1")
        time.sleep(0.8)

    def bench(self, command):
        import socket as pysocket

        with pysocket.socket(pysocket.AF_UNIX, pysocket.SOCK_STREAM) as sock:
            sock.settimeout(20)
            sock.connect(str(SOCKET))
            sock.sendall((command + "\n").encode())
            data = b""
            while not data.endswith(b"\n"):
                chunk = sock.recv(65536)
                if not chunk:
                    break
                data += chunk
        return json.loads(data.decode() or "{}")

    def close(self):
        try:
            self.bench("quit")
        except OSError:
            pass
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def main():
    if not BINARY.exists():
        raise SystemExit(f"build it first: cargo build --release ({BINARY} is missing)")
    if not shutil.which("xdotool"):
        raise SystemExit("xdotool is not installed: pacman -S xdotool")

    home = pathlib.Path(tempfile.mkdtemp(prefix="search-window-"))
    window = None
    try:
        window = Window(home)
        start = window.geometry()
        check(
            "the window is on screen",
            start.get("width", 0) > 600 and start.get("height", 0) > 400,
            f"{start.get('width')}x{start.get('height')} at {start.get('x')},{start.get('y')}",
        )

        # The band at the top of the column is the handle. A press in it and a
        # drag has to move the window, otherwise there is nothing to grab.
        band = (start["x"] + 150, start["y"] + 20)
        window.drag(*band, dx=120, dy=70)
        dragged = window.geometry()
        moved_by = (dragged["x"] - start["x"], dragged["y"] - start["y"])
        check(
            "the window drags by the top band",
            moved_by[0] >= 100 and moved_by[1] >= 50,
            f"moved by {moved_by[0]},{moved_by[1]}",
        )

        # A double-click on the handle maximises, as it does in every other
        # window on this desktop.
        start = window.geometry()
        band = (start["x"] + 150, start["y"] + 20)
        window.xdotool("mousemove", str(band[0]), str(band[1]))
        time.sleep(0.2)
        window.xdotool("click", "--repeat", "2", "--delay", "80", "1")
        time.sleep(1.2)
        # Judged on geometry, not on `is_maximized`: that flag is the window
        # manager's to set and arrives a beat after the resize, so asserting it
        # makes a working control look dead.
        wide = window.settled(width=1180)
        check(
            "a double click on the handle maximises",
            wide["width"] >= 1900,
            f"{wide['width']}x{wide['height']}",
        )
        window.narrow()

        # The controls work from the mouse, which they did not: every handler
        # was wired before the App was installed, so all of them had an empty
        # handle and silently did nothing. The three sit at x 12, 36 and 60,
        # 22 wide and 22 tall, centred vertically in the 40px band.
        start = window.narrow()
        window.click(start["x"] + 71, start["y"] + 20)
        wide = window.settled(width=1180)
        check(
            "the maximise control maximises",
            wide["width"] >= 1900,
            f"{wide['width']}x{wide['height']}",
        )

        window.click(wide["x"] + 71, wide["y"] + 20)
        back = window.settled(width=1180)
        check(
            "and the same control puts it back",
            1000 <= back["width"] <= 1300,
            f"{back['width']}x{back['height']}",
        )

        # A tab row is the other thing that has to answer a mouse.
        before = window.bench("probe")
        window.bench("open https://example.com/first")
        time.sleep(1.0)
        window.bench("open https://example.com/second")
        time.sleep(1.2)
        start = window.settled(width=1180)
        window.click(start["x"] + 60, start["y"] + 100)
        time.sleep(0.6)
        after = window.bench("probe")
        check(
            "a click on a tab row selects that tab",
            after.get("active") != before.get("active") and after.get("active") in
            {tab["id"] for tab in after.get("tabs", [])},
            f"active {before.get('active')} -> {after.get('active')}",
        )
    finally:
        if window is not None:
            window.close()
        shutil.rmtree(home, ignore_errors=True)

    passed = sum(1 for _, ok, _ in results if ok)
    print()
    print(f"{passed} passed, {len(results) - passed} failed, {len(results)} checks")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    sys.exit(main())
