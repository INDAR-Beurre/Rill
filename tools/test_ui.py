"""Exercise the real redesigned GTK interface with an isolated browser profile."""
import hashlib
import json
import os
import pathlib
import re
import signal
import struct
import subprocess
import sys
import tempfile
import threading
import time
import zlib
from http.server import ThreadingHTTPServer

from bench import Bench, BenchError
from fixtures import Handler


ROOT = pathlib.Path(__file__).resolve().parent.parent
BINARY = pathlib.Path(os.environ.get("SEARCH_BINARY", ROOT / "target/release/search"))
DEFAULT_OUTPUT = ROOT / "build/redesign"
SCREENSHOTS = []


def resolve_output(env=None):
    source = os.environ if env is None else env
    raw = (source.get("SEARCH_UI_OUTPUT") or "").strip()
    if raw:
        return pathlib.Path(raw).expanduser()
    return DEFAULT_OUTPUT


def select_backend(env=None):
    source = os.environ if env is None else env
    explicit = (source.get("SEARCH_TEST_BACKEND") or "").strip().lower()
    if explicit:
        return explicit
    if (source.get("WAYLAND_DISPLAY") or "").strip():
        return "wayland"
    if (source.get("DISPLAY") or "").strip():
        return "x11"
    return "wayland"


def build_browser_env(base_env, profile, socket_path):
    environment = dict(base_env)
    backend = select_backend(base_env)
    environment.update(
        GDK_BACKEND=backend,
        SEARCH_BENCH="1",
        SEARCH_BENCH_SOCKET=str(socket_path),
        XDG_CONFIG_HOME=str(profile / "config"),
        XDG_DATA_HOME=str(profile / "data"),
        XDG_CACHE_HOME=str(profile / "cache"),
    )
    if backend == "wayland":
        environment.pop("DISPLAY", None)
        environment["WAYLAND_DISPLAY"] = (
            (base_env.get("WAYLAND_DISPLAY") or "").strip()
            or "/run/user/1000/wayland-0"
        )
    else:
        environment.pop("WAYLAND_DISPLAY", None)
        environment["DISPLAY"] = (base_env.get("DISPLAY") or "").strip() or ":0"
    return backend, environment


OUTPUT = resolve_output()


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
    # truth for the clear upper start page, and the lower horizon must paint
    # genuine multi-tone Bayer ordered dither pixels rather than a flat fill.
    clear_point = (page["at"][0] + 8, page["at"][1] + 24)
    # Ordered Bayer dither horizon now covers ~62-70% of the homepage height,
    # well above the bottom band.
    dither_rows = []
    for y_frac in (0.45, 0.55, 0.70, 0.85):
        dither_y = page["at"][1] + max(40, int(page["size"][1] * y_frac))
        dither_xs = [
            page["at"][0] + max(8, int(page["size"][0] * frac))
            for frac in (0.18, 0.34, 0.50, 0.66, 0.82)
        ]
        for x in dither_xs:
            for dx in (0, 2):
                for dy in (0, 2):
                    dither_rows.append((x + dx, dither_y + dy))
    pixels = png_pixels(path, [clear_point] + dither_rows)
    expected = tuple(map(int, re.findall(r"\d+", probe["card"])[:3]))
    require(
        all(abs(actual - desired) <= 2 for actual, desired in zip(pixels[clear_point], expected)),
        f"{path.name} renders the live GTK palette at {clear_point}: {pixels[clear_point]}",
    )
    distinct_horizon = {pixels[pt] for pt in dither_rows}
    non_card = [
        rgb for rgb in distinct_horizon
        if any(abs(a - b) > 4 for a, b in zip(rgb, expected))
    ]
    require(
        len(distinct_horizon) >= 2 and len(non_card) >= 1,
        f"{path.name} renders ordered dither across >50% start-page horizon: {sorted(distinct_horizon)[:4]}",
    )


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
    # F39 native Cairo controls: pill toggles & menu-button engine picker (zero GtkSwitch/ComboBoxText).
    switches = [n for n in nodes if "pill-switch" in n["classes"]]
    pickers = [n for n in nodes if "engine-picker" in n["classes"]]
    require(len(switches) >= 5, f"Settings renders native pill-switch toggles: {len(switches)}")
    require(len(pickers) == 1 and pickers[0]["type"] == "GtkMenuButton", f"Settings renders native engine-picker button: {pickers}")
    require(not any(n["type"] in ("GtkSwitch", "GtkComboBoxText") for n in nodes), "Settings uses zero GtkSwitch/GtkComboBoxText widgets")


def mapped_tab_surfaces(client):
    """Inspect mapped-ancestry tab surfaces via uitree (never raw visible flag alone)."""
    nodes = client.must("uitree")["nodes"]
    top_tabs = [n for n in nodes if "toptab" in n["classes"] and n["size"][0] > 0 and n["size"][1] > 0]
    top_pins = [n for n in nodes if "toppin" in n["classes"] and n["size"][0] > 0 and n["size"][1] > 0]
    side_rows = [
        n for n in nodes
        if "row" in n["classes"] and "toptab" not in n["classes"]
        and n["type"] == "GtkEventBox" and n["size"][0] > 0 and n["size"][1] > 0
    ]
    side_pins = [
        n for n in nodes
        if "pin" in n["classes"] and "toppin" not in n["classes"]
        and n["type"] == "GtkEventBox" and n["size"][0] > 0 and n["size"][1] > 0
    ]
    top_has = len(top_tabs) > 0 or len(top_pins) > 0
    side_has = len(side_rows) > 0 or len(side_pins) > 0
    return {
        "nodes": nodes,
        "top_tabs": top_tabs,
        "top_pins": top_pins,
        "side_rows": side_rows,
        "side_pins": side_pins,
        "top_has_tabs": top_has,
        "side_has_tabs": side_has,
        "duplicate": top_has and side_has,
    }


def controls_fit(client):
    probe = settled(client)
    chrome = client.must("chrome")
    surfaces = mapped_tab_surfaces(client)
    nodes = surfaces["nodes"]
    active_space = chrome.get("activeSpace", "default")
    space_tabs = [t for t in probe["tabs"] if t.get("spaceId", "default") == active_space]
    unpinned_space_tabs = [t for t in space_tabs if not t.get("pinned")]

    require(not surfaces["duplicate"], f"Tab lists are strictly mutually exclusive by mapped ancestry: top=({len(surfaces['top_tabs'])}t,{len(surfaces['top_pins'])}p) side=({len(surfaces['side_rows'])}t,{len(surfaces['side_pins'])}p)")
    sidebar_shown = chrome.get("sidebarShown", False)
    topbar_shown = chrome["topbarShown"]
    require(topbar_shown != sidebar_shown, f"Exactly one of topbar ({topbar_shown}) or sidebar ({sidebar_shown}) is shown outside fullscreen")

    if topbar_shown:
        require(surfaces["top_has_tabs"] and not surfaces["side_has_tabs"], "Top-tab layout maps only top tabs/pins")
        require(chrome["rowCount"] == len(unpinned_space_tabs), f"Native top tabs match live unpinned tab count in active space ({chrome['rowCount']} vs {len(unpinned_space_tabs)})")
        require(chrome["topTabCount"] == len(space_tabs), f"Top tab strip lists every open tab in active space ({chrome['topTabCount']} vs {len(space_tabs)})")
        for tab in chrome["topTabs"]:
            if tab["visible"]:
                require(tab["at"][1] < 48, f"Top tab {tab['id']} sits in the top strip: y={tab['at'][1]}")
    else:
        require(surfaces["side_has_tabs"] and not surfaces["top_has_tabs"], "Vertical-sidebar layout maps only sidebar rows/pins")
        side_tabs = chrome.get("sideTabs", [])
        require(len(side_tabs) == len(space_tabs), f"Sidebar lists every open tab in active space ({len(side_tabs)} vs {len(space_tabs)})")
        more_ctrl = next((c for c in chrome["controls"] if c["name"] == "more"), None)
        require(more_ctrl is not None and more_ctrl["visible"], "Browser menu button remains mapped in vertical-sidebar layout")

    if not chrome["toolbarShown"]:
        require(
            chrome["visibleBrowserIcons"] <= 2,
            f"Hidden navigation bar keeps at most 2 top-bar browser icons (got {chrome['visibleBrowserIcons']})",
        )
    else:
        require(
            chrome["visibleBrowserIcons"] <= 9,
            f"Compact mouse-first toolbar keeps at most 9 browser icons (got {chrome['visibleBrowserIcons']})",
        )
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
    for node in nodes:
        if "start-search-icon" in node["classes"]:
            require(
                node["size"][0] <= 16 and node["size"][1] <= 16,
                f"Start-page search icon stays small and restrained: {node['size']}",
            )
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
    require(
        all(bloat not in start for bloat in ("Go somewhere", "Selected by Search", "Selected by Rill", "Explore", "Suggested sites", "Recently opened", "No recent pages")),
        "Fresh start page stays super minimal with zero section bloat",
    )
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
    # Minimal fresh toolbar defaults: Home, Reader, and Bookmark are hidden by default and enabled on demand.
    tp = chrome["toolbarPrefs"]
    require(
        tp["toolbar"] and tp["nav"] and tp["reload"]
        and (not tp.get("home", True)) and (not tp.get("reader", True)) and (not tp.get("bookmark", True)),
        f"Fresh minimal toolbar defaults keep Home/Reader/Bookmark hidden: {tp}",
    )
    client.must("start   ")
    require(client.probe()["visibleChild"] == "blank", "Empty start search does not navigate")
    client.must(f"start {base}/plain")
    client.wait_loaded()
    require(client.probe()["url"] == f"{base}/plain", "Start entry submits through real URL navigation")
    require(client.must("chrome")["address"].startswith("http://127.0.0.1"), "Toolbar does not hide an insecure HTTP scheme")
    client.must("toolbaritem bookmark on")
    client.must("uiclick bookmark")
    require(client.probe()["bookmarkCount"] == 1, "Toolbar bookmark control saves the active page")
    client.must("toolbaritem bookmark off")
    shot(client, "page-light")
    for route in ["long", "form"]:
        client.open(f"{base}/{route}")
        client.wait_loaded()
    client.must("uiclick newtab")
    settled(client)
    require(not client.probe()["omni"], "Mouse new-tab action focuses the new start experience")
    require(not client.must("chrome")["bookmarksShown"], "Start page does not duplicate bookmarks in a second bar")
    start = client.must("starttext")["text"]
    require(
        "Plain page" in start
        and all(bloat not in start for bloat in ("Suggested sites", "Explore", "Your bookmarks", "Recently opened")),
        "Populated start page surfaces only minimal bookmark pills without section bloat",
    )
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
                rail_brands = [(n.get("text") or "") for n in client.must("uitree")["nodes"] if "panel-rail-brand" in n.get("classes", [])]
                require(rail_brands == ["Rill"], f"Panel rail wordmark shows Rill in {theme}: {rail_brands}")
                ptext = client.must("paneltext")["text"]
                require("Rill " in ptext and "Rill never saves passwords" in ptext, f"Settings copy reflects Rill branding in {theme}")
            shot(client, f"{control}-{theme}")
            if panel == "Settings":
                client.must("filter dark")
                settled(client)
                client.must("filter ")
                settled(client)
                scrolled = client.must("panelscroll 10000")
                geometry = client.must("panelgeometry")
                require(
                    scrolled["upper"] >= scrolled["page"] + 300
                    and scrolled["value"] >= scrolled["upper"] - scrolled["page"] - 2
                    and geometry["value"] >= 300,
                    f"Settings filter clear restores full scroll range to bottom in {theme}: {scrolled}",
                )
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
        client.must("filter dark")
        settled(client, width)
        client.must("filter ")
        settled(client, width)
        scrolled = client.must("panelscroll 10000")
        require(
            scrolled["upper"] >= scrolled["page"] + 300
            and scrolled["value"] >= scrolled["upper"] - scrolled["page"] - 2
            and scrolled["value"] >= 300,
            f"Settings reaches bottom after filter clear at {width}px: {scrolled}",
        )
        shot(client, f"settings-{width}")
        client.must("key closepanel")
    # Verify top tabs can be reordered via the programmatic bench mutation and keyboard move.
    # (Genuine pointer drag-and-drop is verified separately via real X11 pointer events.)
    before_tabs = [t["id"] for t in client.must("chrome")["topTabs"] if not t["pinned"]]
    require(len(before_tabs) >= 3, f"Multiple top tabs exist for reorder verification: {before_tabs}")
    client.must(f"reordertab {before_tabs[-1]} {before_tabs[0]}")
    settled(client)
    after_reorder = [t["id"] for t in client.must("chrome")["topTabs"] if not t["pinned"]]
    require(after_reorder[0] == before_tabs[-1], f"Bench reordertab mutation moved tab to front: {before_tabs} -> {after_reorder}")
    xs = [t["at"][0] for t in client.must("chrome")["topTabs"] if not t["pinned"] and t["visible"]]
    require(xs == sorted(xs), f"Top tabs lay out left-to-right after reorder: {xs}")
    client.must("key movetabright")
    settled(client)
    after_key = [t["id"] for t in client.must("chrome")["topTabs"] if not t["pinned"]]
    require(after_key != after_reorder, f"Keyboard shortcut Ctrl+Shift+PageDown moves active top tab: {after_reorder} -> {after_key}")

    # Exclusive vertical sidebar vs top tabs: Ctrl S switches cleanly between them (never both).
    require(not client.must("chrome")["sidebarShown"], "Default layout hides the vertical sidebar in favour of top tabs")
    client.must("uiclick fold")
    require(client.must("chrome")["sidebarShown"] and client.must("fullscreen")["sideWidth"] >= 190, "Ctrl S switches to the vertical tab sidebar")
    controls_fit(client)
    # Regression guard: window.show_all() must never resurrect the hidden topbar alongside sidebar.
    after_showall = client.must("showall")
    require((not after_showall["topbarShown"]) and after_showall["sidebarShown"], f"show_all preserves exclusive sidebar layout: {after_showall}")
    # Sidebar + hidden toolbar combo: browser menu remains mapped & reachable without resurrecting topbar.
    require(not client.must("toolbar off")["toolbarShown"], "Navigation bar hides cleanly in sidebar mode")
    controls_fit(client)
    require(client.must("toolbar on")["toolbarShown"], "Navigation bar restores cleanly in sidebar mode")
    shot(client, "sidebar-optional-dark")
    client.must("uiclick fold")
    chrome_after_fold = client.must("chrome")
    require((not chrome_after_fold["sidebarShown"]) and chrome_after_fold["topbarShown"], "Ctrl S switches back to the top tab strip cleanly")
    controls_fit(client)
    shot(client, "folded-dark")
    # Mouse-first defaults show navigation controls cleanly for newcomers.
    client.must("fullscreen on")
    require(not client.must("fullscreen")["toolbarShown"], "Video fullscreen hides the navigation toolbar")
    client.must("fullscreen off")
    require(client.must("fullscreen")["toolbarShown"], "Leaving video fullscreen restores visible navigation toolbar")
    require(not client.must("toolbar off")["toolbarShown"], "Ctrl Shift S can hide navigation bar")
    require(client.must("toolbar on")["toolbarShown"], "Ctrl Shift S can restore navigation bar")
    client.must("resize 1180x1000")
    settled(client, 1180)
    for action in ["uiclick newtab", "key newtab"]:
        client.must("uiclick settings")
        client.must(action)
        settled(client)
        require(not client.probe()["panel"] and client.must("chrome")["startFocused"], f"{action} dismisses the panel before focusing search")
    client.go(f"{base}/plain")
    client.wait_loaded()
    # Site Card: hidden-toolbar anchor must preserve PAGE_INSET on wide and
    # narrow windows, emit zero Gtk-WARNING, and dismiss on real scrim click.
    client.must("toolbar off")
    for sc_width in (1180, 900):
        client.must(f"resize {sc_width}x1000")
        settled(client, sc_width)
        client.must("key sitecard")
        settled(client, sc_width)
        cards = client.must("cards")
        overlay_w = cards["overlay"][0]
        sc_card = next((c for c in cards["cards"] if c["name"] == "site-card"), None)
        sc_scrim = next((c for c in cards["cards"] if c["name"] == "site-card-scrim"), None)
        require(sc_card is not None and sc_scrim is not None, f"Site card and scrim open at {sc_width}px")
        right_gap = overlay_w - (sc_card["at"][0] + sc_card["size"][0])
        require(
            14 <= right_gap <= 24,
            f"Site card preserves PAGE_INSET right gap at {sc_width}px (hidden toolbar): gap={right_gap}px",
        )
        client.must("scrimclick site-card-scrim")
        settled(client, sc_width)
        after = client.must("cards")["cards"]
        require(
            not any(c["name"] in ("site-card", "site-card-scrim") for c in after),
            f"Clicking site-card-scrim dismisses Site Card at {sc_width}px",
        )
    client.must("toolbar on")
    client.must("resize 1180x1000")
    settled(client, 1180)
    # Omnibox security chip truth: plain HTTP -> insecure; plain search -> none;
    # unvisited host -> neutral destination (no lock); failed HTTPS -> insecure.
    client.must("key omni")
    settled(client)
    http_omni = client.must("debug")["omni"]
    require(
        http_omni["chipVisible"] and http_omni["chipHost"] == "127.0.0.1" and http_omni["chipKind"] == "insecure",
        f"Plain HTTP omnibox chip reports insecure: {http_omni}",
    )
    client.must("omnitext office")
    settled(client)
    search_omni = client.must("debug")["omni"]
    require(
        (not search_omni["chipVisible"]) and search_omni["chipKind"] == "none",
        f"Plain search query shows no search-engine lock chip: {search_omni}",
    )
    client.must("omnitext example.com")
    settled(client)
    dest_omni = client.must("debug")["omni"]
    require(
        dest_omni["chipVisible"] and dest_omni["chipHost"] == "example.com" and dest_omni["chipKind"] == "destination",
        f"Unvisited typed host shows neutral destination chip, not verified lock: {dest_omni}",
    )
    for bad_scheme in ("ftp://example.com/file", "javascript:alert(document.domain)", "data:text/html,example.com"):
        client.must(f"omnitext {bad_scheme}")
        settled(client)
        bad_omni = client.must("debug")["omni"]
        require(
            (not bad_omni["chipVisible"]) and bad_omni["chipKind"] == "none",
            f"Unsupported scheme {bad_scheme!r} shows no destination chip: {bad_omni}",
        )
    client.must("escape")
    client.must("showerror tls")
    settled(client)
    ef_init = client.must("errorfocus")
    require(
        ef_init.get("error_visible") and ef_init.get("back_focused"),
        f"TLS error initially focuses safe Go back action: {ef_init}",
    )
    tls_tab = client.probe()["active"]
    other_tab = client.open(f"{base}/plain")
    client.wait_loaded()
    client.select(tls_tab)
    settled(client)
    ef_revisit = client.must("errorfocus")
    require(
        ef_revisit.get("error_visible") and ef_revisit.get("back_focused"),
        f"Revisiting TLS error tab refocuses safe Go back action: {ef_revisit}",
    )
    # Switching to a TLS error tab while an overlay (Settings) is open must not steal overlay focus.
    client.must("key settings")
    settled(client)
    client.select(other_tab)
    client.select(tls_tab)
    settled(client)
    ef_overlay = client.must("errorfocus")
    require(
        ef_overlay.get("error_visible") and (not ef_overlay.get("back_focused")),
        f"TLS error tab select does not steal focus from open Settings overlay: {ef_overlay}",
    )
    client.must("key closepanel")
    settled(client)
    client.select(other_tab)
    client.must("key closetab")
    client.select(tls_tab)
    settled(client)
    client.must("key omni")
    settled(client)
    tls_omni = client.must("debug")["omni"]
    require(
        tls_omni["chipVisible"] and tls_omni["chipHost"] == "expired.badssl.com" and tls_omni["chipKind"] == "insecure",
        f"Failed HTTPS page omnibox chip reports insecure, never lock: {tls_omni}",
    )
    client.must("escape")
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
    require(
        items.get("Zoom Out") and items.get("Reset Zoom") and items.get("Zoom In")
        and items.get("Bookmarks") and items.get("History") and items.get("Downloads")
        and items.get("Add-ons & Extensions") and items.get("Settings"),
        f"Firefox-familiar menu exposes zoom and core library actions: {sorted(items.keys())}",
    )
    client.must("menupick Zoom In")
    require(client.must("chrome")["zoomPercent"] > 100, "Menu Zoom In increases page zoom")
    client.must("menupick Reset Zoom")
    require(client.must("chrome")["zoomPercent"] == 100, "Menu Reset Zoom restores 100%")
    client.must("escape")
    client.must("uiclick forward")
    client.wait_for(lambda probe: probe["url"] == f"{base}/form")
    client.wait_loaded()
    client.must("toolbar off")
    client.go(f"{base}/slow?ms=2000")
    client.wait_for(lambda probe: any(tab["id"] == probe["active"] and tab["loading"] for tab in probe["tabs"]))
    reload_control = next(control for control in client.must("chrome")["controls"] if control["name"] == "reload")
    require("Stop loading" in reload_control["accessibleName"], "The loading action is announced as Stop, not Reload")
    client.must("uiclick more")
    client.must("menupick Stop Loading")
    client.wait_for(lambda probe: all(not tab["loading"] for tab in probe["tabs"] if tab["id"] == probe["active"]))
    client.must("toolbar on")
    client.must("toolbaritem home on")
    client.must("uiclick home")
    client.wait_for(lambda probe: probe["visibleChild"] == "blank")
    require(client.probe()["visibleChild"] == "blank", "Home button navigates active tab to Start Page")
    client.must("toolbaritem home off")
    client.must("uiclick back")
    client.wait_for(lambda probe: probe["visibleChild"] != "blank")
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
    client.must("toolbaritem reader on")
    client.must("uiclick reader")
    client.wait_for(lambda probe: probe["reading"])
    shot(client, "reading-dark")
    client.must("uiclick reader")
    client.must("toolbaritem reader off")
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
        probe = settled(client)
        nodes = client.must("uitree")["nodes"]
        require(probe["visibleChild"] == "welcome", f"First run renders full-tab welcome stack child in {theme}: {probe['visibleChild']}")
        require(not any("welcome-scrim" in node["classes"] for node in nodes), f"Full-tab onboarding uses zero modal scrim overlay in {theme}")
        require(any("welcome-page" in node["classes"] or "welcome-card" in node["classes"] for node in nodes), f"First run has a real {theme} full-tab welcome surface")
        welcome_texts = [(n.get("text") or "") for n in nodes if "welcome-title" in n.get("classes", []) or "welcome-description" in n.get("classes", [])]
        require(any("Welcome to Rill" in t for t in welcome_texts) and not any("Search looks" in t or "Welcome to Search" in t for t in welcome_texts),
                f"Onboarding reflects Rill branding in {theme}: {welcome_texts}")
        shot(client, f"welcome-{theme}")
    for w, h, tag in [(900, 1000, "welcome-900"), (900, 560, "welcome-900x560")]:
        client.must(f"resize {w}x{h}")
        settled(client, w)
        shot(client, tag)
        nodes = client.must("uitree")["nodes"]
        card = next(node for node in nodes if "welcome-page" in node["classes"] or "welcome-card" in node["classes"])
        require(card["at"][0] >= 0 and card["at"][0] + card["size"][0] <= w + 1, f"Welcome surface fits {w}x{h}")
        skip_btn = next((n for n in nodes if "welcome-skip" in n["classes"]), None)
        next_btn = next((n for n in nodes if "welcome-next" in n["classes"] or "welcome-start" in n["classes"]), None)
        require(
            skip_btn is not None and next_btn is not None
            and skip_btn["at"][1] + skip_btn["size"][1] <= h + 1
            and next_btn["at"][1] + next_btn["size"][1] <= h + 1,
            f"Onboarding footer buttons remain reachable inside {w}x{h}: skip={skip_btn and (skip_btn['at'], skip_btn['size'])}, next={next_btn and (next_btn['at'], next_btn['size'])}",
        )
    client.must("escape")
    settled(client)
    require(client.probe()["visibleChild"] == "blank", "Escape dismisses full-tab onboarding to Start Page")
    require(not any("welcome-card" in node["classes"] for node in client.must("uitree")["nodes"]), "Onboarding widgets are unmapped after dismiss")
    require(client.must("chrome")["startFocused"], "Onboarding hands keyboard focus to the start entry")


def main():
    global OUTPUT
    OUTPUT = resolve_output()
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
            backend, environment = build_browser_env(os.environ, profile, socket_path)
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
                    require("Gtk-WARNING" not in output, "Native layout and capture finish without GTK warnings")
                    require("attempt to allocate widget" not in output and "Negative content height" not in output, "Native widgets finish without invalid allocations")
                    binary_hash = hashlib.sha256()
                    with BINARY.open("rb") as binary:
                        while chunk := binary.read(1024 * 1024):
                            binary_hash.update(chunk)
                    (OUTPUT / "manifest.json").write_text(json.dumps({
                        "binary": str(BINARY),
                        "binarySha256": binary_hash.hexdigest(),
                        "backend": backend,
                        "output": str(OUTPUT),
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


def self_test():
    # Output directory resolution: default build/redesign, or caller override without touching default.
    assert resolve_output({}) == DEFAULT_OUTPUT
    assert resolve_output({"SEARCH_UI_OUTPUT": "   "}) == DEFAULT_OUTPUT
    with tempfile.TemporaryDirectory(prefix="search-ui-selftest-") as tmp:
        custom = pathlib.Path(tmp) / "nested-out"
        resolved = resolve_output({"SEARCH_UI_OUTPUT": str(custom)})
        assert resolved == custom
        assert resolved != DEFAULT_OUTPUT
        assert resolve_output({"SEARCH_UI_OUTPUT": "~/search-ui-tmp"}) == pathlib.Path("~/search-ui-tmp").expanduser()

    # Backend selection matches tools/stress.py: explicit override > WAYLAND_DISPLAY > DISPLAY > wayland.
    assert select_backend({"SEARCH_TEST_BACKEND": "x11", "WAYLAND_DISPLAY": "wayland-0", "DISPLAY": ":0"}) == "x11"
    assert select_backend({"SEARCH_TEST_BACKEND": "X11"}) == "x11"
    assert select_backend({"SEARCH_TEST_BACKEND": "wayland", "DISPLAY": ":99"}) == "wayland"
    assert select_backend({"WAYLAND_DISPLAY": "wayland-0", "DISPLAY": ":0"}) == "wayland"
    assert select_backend({"WAYLAND_DISPLAY": "wayland-0"}) == "wayland"
    assert select_backend({"DISPLAY": ":99"}) == "x11"
    assert select_backend({"WAYLAND_DISPLAY": "", "DISPLAY": ":77"}) == "x11"
    assert select_backend({}) == "wayland"

    profile = pathlib.Path("/tmp/search-selftest-profile")
    sock = profile / "bench.sock"
    backend, env = build_browser_env({"WAYLAND_DISPLAY": "wayland-0", "DISPLAY": ":0", "SEARCH_TEST_BACKEND": "x11"}, profile, sock)
    assert backend == "x11" and env["GDK_BACKEND"] == "x11" and env["DISPLAY"] == ":0" and "WAYLAND_DISPLAY" not in env

    backend, env = build_browser_env({"DISPLAY": ":99"}, profile, sock)
    assert backend == "x11" and env["GDK_BACKEND"] == "x11" and env["DISPLAY"] == ":99" and "WAYLAND_DISPLAY" not in env

    backend, env = build_browser_env({"WAYLAND_DISPLAY": "wayland-1", "DISPLAY": ":0"}, profile, sock)
    assert backend == "wayland" and env["GDK_BACKEND"] == "wayland" and env["WAYLAND_DISPLAY"] == "wayland-1" and "DISPLAY" not in env

    backend, env = build_browser_env({}, profile, sock)
    assert backend == "wayland" and env["GDK_BACKEND"] == "wayland" and env["WAYLAND_DISPLAY"] == "/run/user/1000/wayland-0" and "DISPLAY" not in env
    print("test_ui self-test OK")


if __name__ == "__main__":
    if "--self-test" in sys.argv[1:]:
        self_test()
    else:
        main()
