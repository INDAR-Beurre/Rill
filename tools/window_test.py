#!/usr/bin/env python3
"""Tests the window itself: is it draggable, do the controls work.

`tools/stress.py` drives the browser's own actions, which cannot tell whether a
real mouse press reaches the window and moves it. This drives the X server with
xdotool instead: the desktop is kwin_wayland, so the browser has to be launched
with GDK_BACKEND=x11 to be visible at all.

If KWin rejects XTest input, select SEARCH_INPUT_DRIVER=ydotool with an existing
YDOTOOL_SOCKET. Window inspection still uses X11; input goes through /dev/uinput.

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
BINARY = pathlib.Path(os.environ.get("SEARCH_BINARY", ROOT / "target/release/search"))
INPUT_DRIVER = os.environ.get("SEARCH_INPUT_DRIVER", "xdotool")

results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("  ok   " if ok else "  FAIL ") + name + (" - " + detail if detail else ""))


class Window:
    """One browser window, spoken to over X and over the bench socket."""

    def __init__(self, home):
        self.socket = home / "bench.sock"
        env = dict(os.environ)
        env.update(
            GDK_BACKEND="x11",
            SEARCH_BENCH_SOCKET=str(self.socket),
            XDG_CONFIG_HOME=str(home / "config"),
            XDG_DATA_HOME=str(home / "data"),
            XDG_CACHE_HOME=str(home / "cache"),
        )
        (home / "config").mkdir(parents=True)
        (home / "data").mkdir(parents=True)
        self.x = {k: env[k] for k in ("DISPLAY", "XAUTHORITY") if k in env}
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

    def ydotool(self, *args):
        subprocess.run(
            ["ydotool", *args], env={**os.environ, **self.x},
            check=True, capture_output=True, text=True,
        )

    def move_pointer(self, horizontal, vertical):
        if INPUT_DRIVER == "ydotool":
            self.ydotool("mousemove", "--absolute", str(horizontal), str(vertical))
        else:
            self.xdotool("mousemove", str(horizontal), str(vertical))

    def mouse_button(self, action="click", repeat=1):
        if INPUT_DRIVER == "ydotool":
            flags = {"click": "0xC0", "down": "0x40", "up": "0x80"}
            self.ydotool("click", "--repeat", str(repeat), "--next-delay", "40", flags[action])
        elif action == "click":
            self.xdotool("click", "--repeat", str(repeat), "--delay", "80", "1")
        else:
            self.xdotool("mousedown" if action == "down" else "mouseup", "1")

    def key(self, shortcut):
        if INPUT_DRIVER == "ydotool":
            codes = {"ctrl+l": ("29:1", "38:1", "38:0", "29:0"), "Escape": ("1:1", "1:0")}
            self.ydotool("key", "--key-delay", "20", *codes[shortcut])
        else:
            self.xdotool("key", "--clearmodifiers", shortcut)

    def geometry(self, wid=None):
        shell = self.xdotool("getwindowgeometry", "-shell", wid or self.id)
        return {
            key.lower(): int(value)
            for key, _, value in (line.partition("=") for line in shell.split())
            if value.isdigit()
        }

    def state(self):
        return subprocess.run(
            ["xprop", "-id", self.id, "_NET_WM_STATE"],
            env={**os.environ, **self.x}, capture_output=True, text=True,
        ).stdout

    def maximised(self):
        state = self.state()
        return "MAXIMIZED_HORZ" in state and "MAXIMIZED_VERT" in state

    def click_control(self, name):
        control = next(control for control in self.bench("chrome")["controls"] if control["name"] == name)
        if not control["visible"]:
            raise RuntimeError(f"{name} is not visible")
        geometry = self.geometry()
        self.click(
            geometry["x"] + control["at"][0] + control["size"][0] // 2,
            geometry["y"] + control["at"][1] + control["size"][1] // 2 + 1,
        )

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
            now = self.settled()
            if not self.maximised():
                return now
            self.click_control("maximise")
            time.sleep(1.0)
        return self.settled(width=width)

    def click(self, x, y):
        self.move_pointer(x, y)
        time.sleep(0.25)
        self.mouse_button()
        time.sleep(0.8)

    def drag(self, x, y, dx, dy, steps=6):
        self.move_pointer(x, y)
        time.sleep(0.3)
        self.mouse_button("down")
        time.sleep(0.3)
        for step in range(1, steps + 1):
            self.move_pointer(x + dx * step // steps, y + dy * step // steps)
            time.sleep(0.1)
        self.mouse_button("up")
        time.sleep(0.8)

    def bench(self, command):
        import socket as pysocket

        with pysocket.socket(pysocket.AF_UNIX, pysocket.SOCK_STREAM) as sock:
            sock.settimeout(20)
            sock.connect(str(self.socket))
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
            self.proc.wait()
        self.socket.unlink(missing_ok=True)


def main():
    if not BINARY.exists():
        raise SystemExit(f"build it first: cargo build --release ({BINARY} is missing)")
    if not shutil.which("xdotool") or not shutil.which("xprop"):
        raise SystemExit("xdotool and xprop are required for real window input checks")
    if INPUT_DRIVER not in ("xdotool", "ydotool"):
        raise SystemExit("SEARCH_INPUT_DRIVER must be xdotool or ydotool")
    if INPUT_DRIVER == "ydotool" and (not shutil.which("ydotool") or not os.environ.get("YDOTOOL_SOCKET")):
        raise SystemExit("The ydotool input driver needs ydotool and a running YDOTOOL_SOCKET")
    print(f"Input driver: {INPUT_DRIVER}", flush=True)

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
        window.move_pointer(*band)
        time.sleep(0.2)
        window.mouse_button(repeat=2)
        time.sleep(1.2)
        # Judged on geometry, not on `is_maximized`: that flag is the window
        # manager's to set and arrives a beat after the resize, so asserting it
        # makes a working control look dead.
        wide = window.settled()
        check(
            "a double click on the handle maximises",
            window.maximised(),
            f"{wide['width']}x{wide['height']}",
        )
        window.narrow()

        start = window.narrow()
        window.click_control("maximise")
        wide = window.settled()
        check(
            "the maximise control maximises",
            window.maximised(),
            f"{wide['width']}x{wide['height']}",
        )

        window.click_control("maximise")
        back = window.settled(width=1180)
        check(
            "and the same control puts it back",
            not window.maximised() and 1000 <= back["width"] <= 1300,
            f"{back['width']}x{back['height']}",
        )

        # A tab row is the other thing that has to answer a mouse.
        first = window.bench("open about:blank")["id"]
        time.sleep(1.0)
        second = window.bench("open about:blank")["id"]
        time.sleep(1.2)
        start = window.settled(width=1180)
        probe = window.bench("probe")
        target_index = next(index for index, tab in enumerate(probe["tabs"]) if tab["id"] == first)
        rows = [node for node in window.bench("uitree")["nodes"] if node["type"] == "GtkEventBox" and "row" in node["classes"]]
        row = rows[target_index]
        window.click(start["x"] + row["at"][0] + 70, start["y"] + row["at"][1] + row["size"][1] // 2 + 1)
        time.sleep(0.6)
        after = window.bench("probe")
        check(
            "a click on a tab row selects that tab",
            after.get("active") == first,
            f"active {second} -> {after.get('active')}",
        )
        window.xdotool("windowactivate", "--sync", window.id)
        window.key("ctrl+l")
        time.sleep(0.4)
        check("Ctrl L opens the command field from real keyboard input", window.bench("probe")["omni"])
        window.key("Escape")
        time.sleep(0.4)
        check("Escape dismisses the command field", not window.bench("probe")["omni"])
        before = len(window.bench("probe")["tabs"])
        window.click_control("newtab")
        probe = window.bench("probe")
        check("New Tab answers a real mouse press without a modal", len(probe["tabs"]) == before + 1 and not probe["omni"])
        window.click_control("address")
        check("The persistent address opens from a real mouse press", window.bench("probe")["omni"])
        window.key("Escape")
        time.sleep(0.3)
        window.click_control("minimise")
        check("The minimise control minimises the window", "HIDDEN" in window.state())
        window.xdotool("windowactivate", "--sync", window.id)
        window.settled()
        window.click_control("close")
        try:
            window.proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            pass
        check("The close control exits cleanly", window.proc.poll() == 0)
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
