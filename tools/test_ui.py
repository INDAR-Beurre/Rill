"""Exercise the real redesigned GTK interface with an isolated browser profile."""
import hashlib
import json
import os
import pathlib
import re
import signal
import struct
import subprocess
import tempfile
import threading
import time
import zlib
from http.server import ThreadingHTTPServer

from bench import Bench, BenchError
from fixtures import Handler


ROOT = pathlib.Path(__file__).resolve().parent.parent
BINARY = pathlib.Path(os.environ.get("SEARCH_BINARY", ROOT / "target/release/search"))
OUTPUT = ROOT / "build/redesign"
SCREENSHOTS = []


def ready(socket_path, process):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Search exited during startup: {process.returncode}")
        try:
            client = Bench(path=str(socket_path), timeout=20)
            client.must("ping")
            return client
        except BenchError:
            time.sleep(0.1)
    raise RuntimeError("Search did not open its test socket")


def require(condition, message):
    if not condition:
        raise AssertionError(message)
    print(f"  ok   {message}", flush=True)


def settled(client, width=None):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        client.must("settle")
        probe = client.probe()
        if width is None or abs(probe["width"] - width) <= 14:
            time.sleep(0.08)
            return probe
        time.sleep(0.08)
    raise AssertionError(f"Window did not resize to {width}: {client.probe()['width']}")


def shot(client, name):
    settled(client)
    path = OUTPUT / (name + ".png")
    client.must(f"shot {path}")
    SCREENSHOTS.append(path.name)
    return path


def png_pixels(path, points):
    data = path.read_bytes()
    require(data[:8] == b"\x89PNG\r\n\x1a\n", f"{path.name} is an actual PNG")
    position, compressed = 8, bytearray()
    width = height = channels = 0
    while position < len(data):
        length = struct.unpack_from(">I", data, position)[0]
        kind = data[position + 4:position + 8]
        payload = data[position + 8:position + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour = struct.unpack_from(">IIBB", payload)
            require(depth == 8 and colour in (2, 6), "Capture uses a supported true-colour PNG")
            channels = 4 if colour == 6 else 3
        elif kind == b"IDAT":
            compressed.extend(payload)
        position += length + 12
    require(all(0 <= horizontal < width and 0 <= vertical < height for horizontal, vertical in points), "Pixel probes lie inside the capture")
    raw = zlib.decompress(compressed)
    stride, previous, found = width * channels, bytearray(width * channels), {}
    for vertical in range(max(point[1] for point in points) + 1):
        offset = vertical * (stride + 1)
        filter_kind = raw[offset]
        current = bytearray(raw[offset + 1:offset + 1 + stride])
        if filter_kind:
            for index in range(stride):
                left = current[index - channels] if index >= channels else 0
                above = previous[index]
                diagonal = previous[index - channels] if index >= channels else 0
                if filter_kind == 1:
                    predictor = left
                elif filter_kind == 2:
                    predictor = above
                elif filter_kind == 3:
                    predictor = (left + above) // 2
                elif filter_kind == 4:
                    estimate = left + above - diagonal
                    distances = (abs(estimate - left), abs(estimate - above), abs(estimate - diagonal))
                    predictor = (left, above, diagonal)[distances.index(min(distances))]
                else:
                    raise AssertionError(f"Unexpected PNG filter {filter_kind}")
                current[index] = (current[index] + predictor) & 255
        for horizontal, requested_vertical in points:
            if vertical == requested_vertical:
                found[(horizontal, vertical)] = tuple(current[horizontal * channels:horizontal * channels + 3])
        previous = current
    return found


def theme_matches_capture(client, path):
    nodes = client.must("uitree")["nodes"]
    page = next(node for node in nodes if "start-page" in node["classes"])
    probe = client.probe()
    # GTK reports a transparent resolved background for GtkScrolledWindow even
    # when its CSS paints a solid card. The palette itself is the source of
    # truth for the start page, and the captured pixel must match it.
    probes = {(page["at"][0] + 8, page["at"][1] + 110): probe["card"]}
    artwork = next((node for node in nodes if "start-orbit" in node["classes"]), None)
    if artwork:
        point = (artwork["at"][0] + int(artwork["size"][0] * 0.58), artwork["at"][1] + int(artwork["size"][1] * 0.30))
        probes[point] = artwork["foreground"]
    pixels = png_pixels(path, probes)
    for point, css_colour in probes.items():
        expected = tuple(map(int, re.findall(r"\d+", css_colour)[:3]))
        require(all(abs(actual - desired) <= 2 for actual, desired in zip(pixels[point], expected)), f"{path.name} renders the live GTK palette at {point}: {pixels[point]}")


def settings_fit(client):
    geometry = client.must("panelgeometry")
    require(geometry["scroll"][1] >= 160, f"Settings has a useful scrolling viewport: {geometry['card']} card, {geometry['body']} body, {geometry['scroll']} scroll")
    nodes = client.must("uitree")["nodes"]
    card = next(node for node in nodes if "panel-workspace" in node["classes"])
    if "panel-compact" not in card["classes"]:
        rail = next(node for node in nodes if "panel-rail" in node["classes"])
        require(rail["size"][0] <= 200, "Panel rail leaves the content room to breathe")
        first_row = next((node for node in nodes if "panel-setting-row" in node["classes"]), None)
        require(first_row is not None, "Reopened Settings still has its rows")
        # Settings is a scrollable document: rows below the fold must remain
        # reachable, not be clipped by a card that shrank around them.
        require(first_row["size"][1] >= 40, "The first browsing setting has a usable row height")


def controls_fit(client):
    probe = settled(client)
    chrome = client.must("chrome")
    require(chrome["rowCount"] == int(chrome["tabCount"]), "Native tab rows match the live tab count")
    controls = chrome["controls"]
    for control in controls:
        if not control["visible"]:
            continue
        horizontal, vertical = control["at"]
        width, height = control["size"]
        require(
            horizontal >= 0 and vertical >= 0
            and horizontal + width <= probe["width"] + 1
            and vertical + height <= probe["height"] + 1,
            f"{control['name']} fits at {probe['width']}×{probe['height']}: {control['at']}, {control['size']}",
        )
    nodes = client.must("uitree")["nodes"]
    for node in nodes:
        if "start-section-label" in node["classes"]:
            require(node["size"][0] >= node["preferredWidth"][0], f"Section heading is not truncated: {node['text']}")
        if any(style in node["classes"] for style in ("start-content", "panel-workspace", "error-content")):
            require(
                node["at"][0] >= 0 and node["at"][0] + node["size"][0] <= probe["width"] + 1,
                f"{node['classes']} stays inside the window",
            )


def error_fit(client):
    nodes = client.must("uitree")["nodes"]
    glyph = next(node for node in nodes if "error-glyph" in node["classes"])
    button = next(node for node in nodes if node["type"] == "GtkButton" and "act" in node["classes"] and "primary" in node["classes"])
    action = client.probe()["errorAction"]
    caption = next(node for node in nodes if node["type"] == "GtkLabel" and node["text"] == action)
    require(glyph["size"] == [48, 48], "Error-page illustration retains its full allocation")
    require(button["size"][1] >= 32 and caption["size"][1] >= 16, "Error-page action and caption remain readable")
    return {"glyph": (glyph["at"], glyph["size"]), "button": (button["at"], button["size"])}


def exercise(client, base):
    require(not client.probe()["omni"], "New tab reveals the start page, not a modal")
    require(client.probe()["visibleChild"] == "blank", "The native start page is the visible stack child at launch")
    start = client.must("starttext")["text"]
    require("Go somewhere" in start and "No recent pages" in start, "Fresh start page is honest about empty history")
    require("Selected by Search" in start, "Curated links do not impersonate personal bookmarks")
    controls_fit(client)
    theme_matches_capture(client, shot(client, "start-light"))
    client.must("resize 900x1000")
    settled(client, 900)
    controls_fit(client)
    shot(client, "start-light-direct-900")
    client.must("resize 1180x1000")
    settled(client, 1180)
    chrome = client.must("chrome")
    require(all(not control["focusOnClick"] for control in chrome["controls"] if control["name"] in ("back", "forward", "reload")), "Mouse navigation does not steal page keyboard focus")
    client.must("start   ")
    require(client.probe()["visibleChild"] == "blank", "Empty start search does not navigate")
    client.must(f"start {base}/plain")
    client.wait_loaded()
    require(client.probe()["url"] == f"{base}/plain", "Start entry submits through real URL navigation")
    require(client.must("chrome")["address"].startswith("http://127.0.0.1"), "Toolbar does not hide an insecure HTTP scheme")
    client.must("uiclick bookmark")
    require(client.probe()["bookmarkCount"] == 1, "Toolbar bookmark control saves the active page")
    shot(client, "page-light")
    for route in ["long", "form"]:
        client.open(f"{base}/{route}")
        client.wait_loaded()
    client.must("uiclick newtab")
    settled(client)
    require(not client.probe()["omni"], "Mouse new-tab action focuses the new start experience")
    require(not client.must("chrome")["bookmarksShown"], "Start page does not duplicate bookmarks in a second bar")
    start = client.must("starttext")["text"]
    require("Selected by Search" not in start and "Plain page" in start, "Start page shows actual bookmarks and recent visits")
    shot(client, "start-populated")
    client.must("uiclick address")
    require(client.probe()["omni"], "Persistent address control opens the command field")
    shot(client, "omnibox-light")
    client.must("key escape")
    for theme in ["light", "dark"]:
        client.must(f"theme {theme}")
        require(client.must("chrome")["dark"] == (theme == "dark"), f"{theme} appearance switches while running")
        theme_matches_capture(client, shot(client, f"start-{theme}-populated"))
        for control, panel in [("library", "Bookmarks"), ("history", "History"), ("downloads", "Downloads"), ("settings", "Settings")]:
            client.must(f"uiclick {control}")
            require(client.probe()["panel"] == panel, f"{control} opens {panel} in {theme}")
            controls_fit(client)
            if panel == "Settings":
                settings_fit(client)
            shot(client, f"{control}-{theme}")
            if panel == "Settings":
                client.must("panelscroll 10000")
                geometry = client.must("panelgeometry")
                require(geometry["value"] > 0, "Long settings remain scrollable")
                shot(client, f"settings-bottom-{theme}")
            client.must("key closepanel")
        for panel in ["extensions", "shortcuts"]:
            client.must(f"key {panel}")
            shot(client, f"{panel}-{theme}")
            client.must("key closepanel")
        client.must("uiclick address")
        shot(client, f"omnibox-{theme}")
        client.must("key escape")
    client.must("uiclick private")
    settled(client)
    start = client.must("starttext")["text"]
    require("Private" in start and "anonymous" in start, "Private start page states its actual limits")
    require("Plain page" not in start and "A long read" not in start, "Private start page hides regular browsing records")
    require(client.must("chrome")["address"] == "Private tab", "Toolbar immediately reflects private mode")
    shot(client, "private-dark")
    client.must(f"start {base}/plain")
    client.wait_page("The quietest page there is.", min_chars=20)
    require(client.probe()["visibleChild"] != "error", "Private start search opens the actual page")
    active = client.probe()["active"]
    require(any(tab["id"] == active and tab["private"] for tab in client.probe()["tabs"]), "Start search preserves private context")
    client.must("key closetab")
    for width, height in [(1180, 1000), (1000, 1000), (900, 1000)]:
        client.must(f"resize {width}x{height}")
        settled(client, width)
        shot(client, f"start-{width}")
        controls_fit(client)
        client.must("uiclick settings")
        settled(client, width)
        controls_fit(client)
        settings_fit(client)
        shot(client, f"settings-{width}")
        client.must("key closepanel")
    client.must("uiclick fold")
    require(client.must("fullscreen")["sideWidth"] <= 20, "Column fold leaves the promised thin strip")
    controls_fit(client)
    shot(client, "folded-dark")
    client.must("uiclick fold")
    require(client.must("fullscreen")["sideWidth"] >= 200, "Toolbar can restore the tab column")
    # Reference mode hides navigation by default. Leaving page fullscreen
    # restores the person's prior choice, which here is hidden; the explicit
    # toggle is the way back.
    client.must("fullscreen on")
    require(not client.must("fullscreen")["toolbarShown"], "Video fullscreen hides the redesigned toolbar")
    client.must("fullscreen off")
    require(not client.must("fullscreen")["toolbarShown"], "Leaving video fullscreen keeps the chosen quiet chrome")
    require(client.must("toolbar on")["toolbarShown"], "Ctrl Shift S can reveal navigation")
    client.must("toolbar off")
    client.must("resize 1180x1000")
    settled(client, 1180)
    for action in ["uiclick newtab", "key newtab"]:
        client.must("uiclick settings")
        client.must(action)
        settled(client)
        require(not client.probe()["panel"] and client.must("chrome")["startFocused"], f"{action} dismisses the panel before focusing search")
    client.go(f"{base}/plain")
    client.wait_loaded()
    client.go(f"{base}/form")
    client.wait_loaded()
    client.must("uiclick back")
    client.wait_for(lambda probe: probe["url"] == f"{base}/plain")
    client.wait_loaded()
    client.must("resize 900x1000")
    settled(client, 900)
    controls_fit(client)
    shot(client, "page-dark-900")
    client.must("uiclick more")
    items = {item["label"]: item["sensitive"] for item in client.must("menuentries")["items"]}
    require(items.get("Forward") and items.get("Reload"), "Compact chrome provides native navigation overflow actions")
    client.must("menupick Forward")
    client.wait_for(lambda probe: probe["url"] == f"{base}/form")
    client.wait_loaded()
    client.go(f"{base}/slow?ms=2000")
    client.wait_for(lambda probe: any(tab["id"] == probe["active"] and tab["loading"] for tab in probe["tabs"]))
    reload_control = next(control for control in client.must("chrome")["controls"] if control["name"] == "reload")
    require("Stop loading" in reload_control["accessibleName"], "The loading action is announced as Stop, not Reload")
    client.must("uiclick more")
    client.must("menupick Stop Loading")
    client.wait_for(lambda probe: all(not tab["loading"] for tab in probe["tabs"] if tab["id"] == probe["active"]))
    client.go(f"{base}/plain")
    client.wait_loaded()
    client.must("key blocker")
    client.wait_loaded()
    client.must("uiclick private")
    client.must(f"start {base}/plain")
    client.wait_loaded()
    require(not client.must("chrome")["protected"], "Private toolbar accurately reports saved blocking exceptions")
    client.must("key closetab")
    client.must("key blocker")
    client.wait_loaded()
    client.must("resize 1180x1000")
    settled(client, 1180)
    client.must("key pin")
    shot(client, "pins-dark")
    client.go(f"{base}/long")
    client.wait_loaded()
    client.must("uiclick reader")
    client.wait_for(lambda probe: probe["reading"])
    shot(client, "reading-dark")
    client.must("uiclick reader")
    client.open(f"{base}/missing")
    client.wait_for(lambda probe: probe["error"])
    geometry = error_fit(client)
    shot(client, "error-dark")
    require(error_fit(client) == geometry, "Capturing the error page does not alter native widget geometry")
    shot(client, "error-dark-repeat")
    require(error_fit(client) == geometry, "Repeated captures preserve native allocations")
    client.must("resize 900x1000")
    settled(client, 900)
    controls_fit(client)
    geometry = error_fit(client)
    shot(client, "error-dark-900")
    require(error_fit(client) == geometry, "Compact error capture preserves control geometry")


def exercise_welcome(client):
    for theme in ["light", "dark"]:
        client.must(f"theme {theme}")
        nodes = client.must("uitree")["nodes"]
        require(any("welcome-card" in node["classes"] for node in nodes), f"First run has a real {theme} welcome card")
        shot(client, f"welcome-{theme}")
    client.must("resize 900x1000")
    settled(client, 900)
    shot(client, "welcome-900")
    card = next(node for node in client.must("uitree")["nodes"] if "welcome-card" in node["classes"])
    require(card["at"][0] >= 0 and card["at"][0] + card["size"][0] <= 900, "Welcome card fits a compact window")
    client.must("escape")
    require(not any("welcome-card" in node["classes"] for node in client.must("uitree")["nodes"]), "Escape dismisses onboarding")
    require(client.must("chrome")["startFocused"], "Onboarding hands keyboard focus to the start entry")


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    SCREENSHOTS.clear()
    fixture = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    fixture.daemon_threads = True
    thread = threading.Thread(target=fixture.serve_forever, daemon=True)
    thread.start()
    client = None
    process = None
    try:
        with tempfile.TemporaryDirectory(prefix="search-redesign-") as temporary:
            profile = pathlib.Path(temporary)
            socket_path = profile / "bench.sock"
            environment = dict(os.environ)
            backend = os.environ.get("SEARCH_TEST_BACKEND", "wayland")
            environment.update(
                GDK_BACKEND=backend,
                SEARCH_BENCH="1",
                SEARCH_BENCH_SOCKET=str(socket_path),
                XDG_CONFIG_HOME=str(profile / "config"),
                XDG_DATA_HOME=str(profile / "data"),
                XDG_CACHE_HOME=str(profile / "cache"),
            )
            if backend == "wayland":
                environment["WAYLAND_DISPLAY"] = os.environ.get("WAYLAND_DISPLAY", "/run/user/1000/wayland-0")
            else:
                environment.pop("WAYLAND_DISPLAY", None)
                environment["DISPLAY"] = os.environ.get("DISPLAY", ":0")
            with open(OUTPUT / "test-ui.log", "w+") as log:
                process = subprocess.Popen(
                    [str(BINARY), "--fresh", "--appearance", "light", "--size", "1180x1000"],
                    env=environment, stdout=log, stderr=log, start_new_session=True,
                )
                try:
                    client = ready(socket_path, process)
                    settled(client, 1180)
                    exercise(client, f"http://127.0.0.1:{fixture.server_port}")
                    client.must("quit")
                    process.wait(timeout=15)
                    client.disconnect()
                    welcome_socket = profile / "welcome.sock"
                    environment.update(
                        SEARCH_BENCH_SOCKET=str(welcome_socket),
                        XDG_CONFIG_HOME=str(profile / "welcome/config"),
                        XDG_DATA_HOME=str(profile / "welcome/data"),
                        XDG_CACHE_HOME=str(profile / "welcome/cache"),
                    )
                    process = subprocess.Popen(
                        [str(BINARY), "--appearance", "light", "--size", "1180x1000"],
                        env=environment, stdout=log, stderr=log, start_new_session=True,
                    )
                    client = ready(welcome_socket, process)
                    settled(client, 1180)
                    exercise_welcome(client)
                    client.must("quit")
                    process.wait(timeout=15)
                    log.seek(0)
                    output = log.read()
                    require("stylesheet did not load" not in output and "Theme parsing error" not in output, "Both GTK stylesheets load without parser errors")
                    require("panicked at" not in output and process.returncode == 0, "Browser exits cleanly after the full UI walkthrough")
                    require("Gtk-CRITICAL" not in output, "Native layout and capture finish without GTK criticals")
                    require("attempt to allocate widget" not in output and "Negative content height" not in output, "Native widgets finish without invalid allocations")
                    binary_hash = hashlib.sha256()
                    with BINARY.open("rb") as binary:
                        while chunk := binary.read(1024 * 1024):
                            binary_hash.update(chunk)
                    (OUTPUT / "manifest.json").write_text(json.dumps({
                        "binary": str(BINARY),
                        "binarySha256": binary_hash.hexdigest(),
                        "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                        "screenshots": sorted(set(SCREENSHOTS)),
                    }, indent=2))
                finally:
                    if process.poll() is None:
                        try:
                            if client is not None:
                                client.command("quit")
                            process.wait(timeout=5)
                        except (BenchError, subprocess.TimeoutExpired):
                            os.killpg(process.pid, signal.SIGTERM)
                            try:
                                process.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                os.killpg(process.pid, signal.SIGKILL)
                                process.wait(timeout=5)
    finally:
        fixture.shutdown()
        fixture.server_close()


if __name__ == "__main__":
    main()
