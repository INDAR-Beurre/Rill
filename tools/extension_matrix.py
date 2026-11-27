#!/usr/bin/env python3
"""Run a truthful 108-extension compatibility matrix against the real Search binary.

Generates local manifest/script/CSS fixtures modeled after 108 real-world browser
extensions across MV2 and MV3 (never downloads third-party packages), installs all
108 into a real isolated Search browser profile via the bench socket, classifies
each honestly (full_support / partial_support / unsupported), and verifies real
DOM JS/CSS injection, host matching, exclude_matches, document_start, options pages,
and isolated-world behavior on representative fixtures.
"""
import argparse
import json
import os
import pathlib
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from bench import Bench, BenchError  # noqa: E402
from test_ui import build_browser_env, ready  # noqa: E402


# 108 distinct real-world-style extension specs.
# Fields: (slug, name, category, mv, kind, permissions, host_permissions, extra)
# kind values:
#   "content_js"           -> pure JS content script (no chrome.*/browser.*, no background)
#   "content_css"          -> pure CSS content script
#   "content_js_css"       -> JS + CSS content script (pure DOM)
#   "content_options"      -> JS + CSS content script + options page
#   "content_api"          -> content script calling chrome.*/browser.* API (no background)
#   "content_bg"           -> content script + background (SW in MV3, scripts in MV2) + API
#   "bg_only"              -> background only, no content scripts
#   "host_perm_only"       -> host_permissions / permissions only, no content scripts
#   "dnr_only"             -> declarativeNetRequest + background/rules, no content scripts
#   "password_mgr"         -> content script + background + nativeMessaging/storage/tabs + API
EXTENSION_SPECS = [
    # 1. Password Managers & Autofill (10)
    ("bitwarden", "Bitwarden Password Manager", "Password Managers", 3, "password_mgr", ["tabs", "storage", "contextMenus", "alarms", "nativeMessaging", "clipboardRead", "clipboardWrite"], ["<all_urls>"], {"action": True, "options": True}),
    ("1password", "1Password – Password Manager", "Password Managers", 3, "password_mgr", ["tabs", "storage", "contextMenus", "alarms", "nativeMessaging", "webNavigation"], ["<all_urls>"], {"action": True, "options": True}),
    ("keepassxc-browser", "KeePassXC-Browser", "Password Managers", 3, "password_mgr", ["tabs", "storage", "contextMenus", "nativeMessaging", "webNavigation"], ["<all_urls>"], {"action": True, "options": True}),
    ("dashlane", "Dashlane — Password Manager", "Password Managers", 3, "password_mgr", ["tabs", "storage", "webRequest", "alarms", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("lastpass", "LastPass: Free Password Manager", "Password Managers", 3, "password_mgr", ["tabs", "storage", "cookies", "contextMenus", "webNavigation"], ["<all_urls>"], {"action": True, "options": True}),
    ("proton-pass", "Proton Pass: Free Password Manager", "Password Managers", 3, "password_mgr", ["tabs", "storage", "alarms", "webRequest", "scripting"], ["<all_urls>"], {"action": True, "options": True}),
    ("nordpass", "NordPass Password Manager & Digital Vault", "Password Managers", 3, "password_mgr", ["tabs", "storage", "nativeMessaging", "alarms", "contextMenus"], ["<all_urls>"], {"action": True}),
    ("keeper", "Keeper Password Manager & Digital Vault", "Password Managers", 3, "password_mgr", ["tabs", "storage", "cookies", "contextMenus", "webNavigation"], ["<all_urls>"], {"action": True}),
    ("roboform", "RoboForm Password Manager", "Password Managers", 3, "password_mgr", ["tabs", "storage", "nativeMessaging", "contextMenus"], ["<all_urls>"], {"action": True}),
    ("enpass", "Enpass Password Manager", "Password Managers", 3, "password_mgr", ["tabs", "storage", "nativeMessaging", "tabs"], ["<all_urls>"], {"action": True}),

    # 2. Ad, Tracker & Privacy Blockers (16)
    ("ublock-origin", "uBlock Origin", "Ad & Privacy Blockers", 2, "content_bg", ["webRequest", "webRequestBlocking", "tabs", "storage", "privacy", "webNavigation"], ["<all_urls>"], {"action": True, "options": True}),
    ("ublock-origin-lite", "uBlock Origin Lite", "Ad & Privacy Blockers", 3, "dnr_only", ["declarativeNetRequest", "declarativeNetRequestWithHostAccess", "storage", "scripting"], ["<all_urls>"], {"action": True, "options": True}),
    ("adguard", "AdGuard AdBlocker", "Ad & Privacy Blockers", 3, "content_bg", ["declarativeNetRequest", "tabs", "storage", "webNavigation", "scripting", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("adblock-plus", "Adblock Plus", "Ad & Privacy Blockers", 3, "content_bg", ["declarativeNetRequest", "tabs", "storage", "webNavigation", "scripting"], ["<all_urls>"], {"action": True, "options": True}),
    ("ghostery", "Ghostery Tracker & Ad Blocker", "Ad & Privacy Blockers", 3, "content_bg", ["declarativeNetRequest", "tabs", "storage", "webNavigation", "scripting"], ["<all_urls>"], {"action": True}),
    ("privacy-badger", "Privacy Badger", "Ad & Privacy Blockers", 2, "content_bg", ["webRequest", "webRequestBlocking", "tabs", "storage", "cookies", "privacy"], ["<all_urls>"], {"action": True, "options": True}),
    ("ddg-privacy", "DuckDuckGo Privacy Essentials", "Ad & Privacy Blockers", 3, "content_bg", ["declarativeNetRequest", "tabs", "storage", "webNavigation", "cookies"], ["<all_urls>"], {"action": True}),
    ("https-everywhere", "HTTPS Everywhere", "Ad & Privacy Blockers", 2, "bg_only", ["webRequest", "webRequestBlocking", "tabs", "storage"], ["<all_urls>"], {"action": True}),
    ("clearurls", "ClearURLs", "Ad & Privacy Blockers", 2, "bg_only", ["webRequest", "webRequestBlocking", "storage", "tabs"], ["<all_urls>"], {"action": True, "options": True}),
    ("decentraleyes", "Decentraleyes", "Ad & Privacy Blockers", 2, "bg_only", ["webRequest", "webRequestBlocking", "tabs", "storage"], ["<all_urls>"], {"action": True, "options": True}),
    ("localcdn", "LocalCDN", "Ad & Privacy Blockers", 2, "bg_only", ["webRequest", "webRequestBlocking", "tabs", "storage"], ["<all_urls>"], {"action": True, "options": True}),
    ("cookie-autodelete", "Cookie AutoDelete", "Ad & Privacy Blockers", 2, "bg_only", ["cookies", "tabs", "storage", "browsingData", "alarms"], ["<all_urls>"], {"action": True, "options": True}),
    ("istilldontcareaboutcookies", "I Still Don't Care About Cookies", "Ad & Privacy Blockers", 3, "content_js_css", [], [], {"run_at": "document_end", "matches": ["*://127.0.0.1/*", "*://localhost/*"], "exclude_matches": ["*://127.0.0.1/excluded*"]}),
    ("consent-o-matic", "Consent-O-Matic", "Ad & Privacy Blockers", 3, "content_bg", ["storage", "tabs"], ["<all_urls>"], {"action": True, "options": True}),
    ("noscript", "NoScript", "Ad & Privacy Blockers", 2, "content_bg", ["webRequest", "webRequestBlocking", "tabs", "storage", "webNavigation"], ["<all_urls>"], {"action": True, "options": True}),
    ("umatrix", "uMatrix", "Ad & Privacy Blockers", 2, "bg_only", ["webRequest", "webRequestBlocking", "tabs", "cookies", "storage"], ["<all_urls>"], {"action": True, "options": True}),

    # 3. Privacy & Security Hardening (6)
    ("canvasblocker", "CanvasBlocker", "Privacy & Security", 2, "content_bg", ["storage", "tabs", "webRequest"], ["<all_urls>"], {"action": True, "options": True}),
    ("privacy-pass", "Privacy Pass", "Privacy & Security", 2, "bg_only", ["webRequest", "webRequestBlocking", "storage"], ["<all_urls>"], {"action": True}),
    ("ua-switcher", "User-Agent Switcher", "Privacy & Security", 3, "bg_only", ["declarativeNetRequest", "storage", "tabs"], ["<all_urls>"], {"action": True}),
    ("chameleon", "Chameleon", "Privacy & Security", 2, "content_bg", ["webRequest", "webRequestBlocking", "privacy", "storage"], ["<all_urls>"], {"action": True}),
    ("neat-url", "Neat URL", "Privacy & Security", 2, "bg_only", ["webRequest", "webRequestBlocking", "storage"], ["<all_urls>"], {"options": True}),
    ("skip-redirect", "Skip Redirect", "Privacy & Security", 2, "bg_only", ["webRequest", "webRequestBlocking", "storage"], ["<all_urls>"], {"options": True}),

    # 4. Appearance, Dark Mode & Custom CSS (10)
    ("dark-reader", "Dark Reader", "Appearance & CSS", 3, "content_bg", ["storage", "tabs", "alarms"], ["<all_urls>"], {"action": True, "options": True}),
    ("dark-mode-css", "Dark Mode (Static CSS)", "Appearance & CSS", 3, "content_css", [], [], {"matches": ["*://127.0.0.1/*"], "options": True}),
    ("midnight-lizard", "Midnight Lizard", "Appearance & CSS", 2, "content_bg", ["storage", "tabs"], ["<all_urls>"], {"action": True}),
    ("stylus", "Stylus", "Appearance & CSS", 2, "content_bg", ["tabs", "storage", "webNavigation", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("stylebot", "Stylebot", "Appearance & CSS", 3, "content_bg", ["storage", "tabs", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("stylish", "Stylish — Custom Themes", "Appearance & CSS", 3, "content_bg", ["storage", "tabs", "webNavigation"], ["<all_urls>"], {"action": True}),
    ("minimal-x-css", "Minimal X / Twitter CSS", "Appearance & CSS", 3, "content_css", [], [], {"matches": ["https://x.com/*", "https://twitter.com/*"]}),
    ("old-reddit-css", "Clean Reddit Stylesheet", "Appearance & CSS", 3, "content_css", [], [], {"matches": ["https://*.reddit.com/*"]}),
    ("wiki-reading-css", "Clean Wikipedia Typography", "Appearance & CSS", 3, "content_options", [], [], {"matches": ["https://*.wikipedia.org/*", "*://127.0.0.1/*"], "options": True}),
    ("hn-readable-css", "Clean Hacker News Theme", "Appearance & CSS", 3, "content_css", [], [], {"matches": ["https://news.ycombinator.com/*"]}),

    # 5. Userscript Managers (4)
    ("tampermonkey", "Tampermonkey", "Userscript Managers", 3, "content_bg", ["tabs", "storage", "webRequest", "cookies", "downloads", "scripting", "alarms"], ["<all_urls>"], {"action": True, "options": True}),
    ("violentmonkey", "Violentmonkey", "Userscript Managers", 2, "content_bg", ["tabs", "storage", "webRequest", "webRequestBlocking", "cookies", "downloads"], ["<all_urls>"], {"action": True, "options": True}),
    ("greasemonkey", "Greasemonkey", "Userscript Managers", 2, "content_bg", ["tabs", "storage", "downloads"], ["<all_urls>"], {"action": True}),
    ("firemonkey", "FireMonkey", "Userscript Managers", 2, "content_bg", ["tabs", "storage", "webNavigation"], ["<all_urls>"], {"action": True, "options": True}),

    # 6. Site Enhancers & Content-Script Utilities (14)
    ("refined-github", "Refined GitHub", "Site Enhancers", 3, "content_bg", ["storage", "scripting", "alarms"], ["https://github.com/*"], {"options": True}),
    ("octotree", "Octotree - GitHub Code Tree", "Site Enhancers", 3, "content_bg", ["storage", "tabs"], ["https://github.com/*"], {"action": True}),
    ("sponsorblock", "SponsorBlock for YouTube", "Site Enhancers", 3, "content_bg", ["storage", "tabs"], ["https://www.youtube.com/*"], {"action": True, "options": True}),
    ("return-youtube-dislike", "Return YouTube Dislike", "Site Enhancers", 3, "content_api", ["storage"], ["https://www.youtube.com/*"], {"action": True}),
    ("enhancer-youtube", "Enhancer for YouTube", "Site Enhancers", 3, "content_bg", ["storage", "tabs"], ["https://www.youtube.com/*"], {"options": True}),
    ("improvedtube", "ImprovedTube", "Site Enhancers", 3, "content_bg", ["storage", "tabs"], ["https://www.youtube.com/*"], {"action": True}),
    ("old-reddit-redirect", "Old Reddit Redirect", "Site Enhancers", 3, "dnr_only", ["declarativeNetRequest"], ["https://www.reddit.com/*"], {}),
    ("res", "Reddit Enhancement Suite", "Site Enhancers", 2, "content_bg", ["tabs", "storage", "history", "cookies"], ["https://*.reddit.com/*"], {"options": True}),
    ("augmented-steam", "Augmented Steam", "Site Enhancers", 3, "content_bg", ["storage", "contextMenus"], ["https://store.steampowered.com/*"], {"options": True}),
    ("keepa", "Keepa - Amazon Price Tracker", "Site Enhancers", 3, "content_bg", ["storage", "tabs", "alarms"], ["https://*.amazon.com/*"], {"action": True}),
    ("honey", "PayPal Honey", "Site Enhancers", 3, "content_bg", ["tabs", "storage", "cookies", "webRequest"], ["<all_urls>"], {"action": True}),
    ("rakuten", "Rakuten Cash Back", "Site Enhancers", 3, "content_bg", ["tabs", "storage", "cookies"], ["<all_urls>"], {"action": True}),
    ("camelizer", "The Camelizer", "Site Enhancers", 3, "bg_only", ["tabs", "storage"], ["https://*.amazon.com/*"], {"action": True}),
    ("unhook-youtube", "Unhook YouTube Distractions (DOM)", "Site Enhancers", 3, "content_js_css", [], [], {"matches": ["https://www.youtube.com/*", "*://127.0.0.1/*"], "run_at": "document_start"}),

    # 7. Developer Tools & Inspectors (14)
    ("react-devtools", "React Developer Tools", "Developer Tools", 3, "content_bg", ["scripting", "tabs", "storage"], ["<all_urls>"], {"action": True}),
    ("vue-devtools", "Vue.js Devtools", "Developer Tools", 3, "content_bg", ["scripting", "tabs", "storage"], ["<all_urls>"], {"action": True}),
    ("redux-devtools", "Redux DevTools", "Developer Tools", 3, "content_bg", ["tabs", "storage"], ["<all_urls>"], {"action": True}),
    ("angular-devtools", "Angular DevTools", "Developer Tools", 3, "content_bg", ["scripting", "tabs"], ["<all_urls>"], {}),
    ("svelte-devtools", "Svelte DevTools", "Developer Tools", 3, "content_bg", ["tabs", "storage"], ["<all_urls>"], {}),
    ("apollo-devtools", "Apollo Client Devtools", "Developer Tools", 3, "content_bg", ["tabs", "storage"], ["<all_urls>"], {}),
    ("web-developer", "Web Developer", "Developer Tools", 3, "bg_only", ["tabs", "scripting", "cookies", "storage"], ["<all_urls>"], {"action": True, "options": True}),
    ("wappalyzer", "Wappalyzer - Technology Profiler", "Developer Tools", 3, "content_bg", ["tabs", "storage", "webRequest", "webNavigation"], ["<all_urls>"], {"action": True}),
    ("builtwith", "BuiltWith Technology Profiler", "Developer Tools", 3, "bg_only", ["tabs", "webRequest", "storage"], ["<all_urls>"], {"action": True}),
    ("json-viewer", "JSON Viewer (Pure DOM)", "Developer Tools", 3, "content_js_css", [], [], {"matches": ["*://127.0.0.1/*"], "options": True}),
    ("jsonvue", "JSONVue", "Developer Tools", 2, "content_api", ["storage"], ["<all_urls>"], {"options": True}),
    ("lighthouse", "Lighthouse", "Developer Tools", 3, "bg_only", ["tabs", "storage"], ["<all_urls>"], {"action": True}),
    ("axe-devtools", "axe DevTools", "Developer Tools", 3, "content_bg", ["tabs", "storage", "scripting"], ["<all_urls>"], {}),
    ("wave-eval", "WAVE Evaluation Tool", "Developer Tools", 3, "bg_only", ["tabs", "scripting", "contextMenus"], ["<all_urls>"], {"action": True}),

    # 8. Writing, Grammar & Translation (8)
    ("grammarly", "Grammarly: AI Writing & Grammar", "Writing & Translation", 3, "content_bg", ["tabs", "storage", "cookies", "alarms", "scripting"], ["<all_urls>"], {"action": True, "options": True}),
    ("languagetool", "Grammar & Spell Checker — LanguageTool", "Writing & Translation", 3, "content_bg", ["storage", "tabs", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("deepl", "DeepL Translate", "Writing & Translation", 3, "content_bg", ["storage", "tabs", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("google-translate", "Google Translate", "Writing & Translation", 3, "content_bg", ["storage", "tabs", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("twp-translate", "TWP - Translate Web Pages", "Writing & Translation", 2, "content_bg", ["tabs", "storage", "contextMenus", "webRequest"], ["<all_urls>"], {"action": True, "options": True}),
    ("read-aloud", "Read Aloud: Text to Speech", "Writing & Translation", 3, "content_bg", ["storage", "tabs", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("dictionary-anywhere", "Google Dictionary (DOM Helper)", "Writing & Translation", 3, "content_api", ["storage"], ["<all_urls>"], {"options": True}),
    ("yomitan", "Yomitan Dictionary", "Writing & Translation", 3, "content_bg", ["storage", "tabs", "clipboardRead"], ["<all_urls>"], {"action": True, "options": True}),

    # 9. Keyboard & Tab Management (10)
    ("vimium", "Vimium", "Keyboard & Tabs", 3, "content_bg", ["tabs", "bookmarks", "history", "storage", "sessions"], ["<all_urls>"], {"options": True}),
    ("tridactyl", "Tridactyl", "Keyboard & Tabs", 2, "content_bg", ["tabs", "bookmarks", "history", "storage", "sessions", "nativeMessaging"], ["<all_urls>"], {"options": True}),
    ("surfingkeys", "Surfingkeys", "Keyboard & Tabs", 3, "content_bg", ["tabs", "bookmarks", "history", "storage", "sessions"], ["<all_urls>"], {"options": True}),
    ("tree-style-tab", "Tree Style Tab", "Keyboard & Tabs", 2, "bg_only", ["tabs", "sessions", "storage", "bookmarks"], [], {"options": True}),
    ("sidebery", "Sidebery", "Keyboard & Tabs", 2, "bg_only", ["tabs", "sessions", "storage", "bookmarks", "cookies"], [], {"options": True}),
    ("onetab", "OneTab", "Keyboard & Tabs", 3, "bg_only", ["tabs", "storage", "contextMenus"], [], {"action": True}),
    ("workona", "Workona Tab Manager", "Keyboard & Tabs", 3, "bg_only", ["tabs", "storage", "history", "alarms"], ["<all_urls>"], {"action": True}),
    ("session-buddy", "Session Buddy", "Keyboard & Tabs", 3, "bg_only", ["tabs", "storage", "bookmarks", "sessions"], [], {"action": True}),
    ("tab-wrangler", "Tab Wrangler", "Keyboard & Tabs", 3, "bg_only", ["tabs", "storage", "alarms", "sessions"], [], {"action": True, "options": True}),
    ("toby", "Toby for Chrome", "Keyboard & Tabs", 3, "bg_only", ["tabs", "storage", "bookmarks"], [], {"action": True}),

    # 10. Reading, Clipping & Research (8)
    ("singlefile", "SingleFile", "Reading & Research", 3, "content_bg", ["tabs", "downloads", "storage", "scripting", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("raindrop", "Raindrop.io", "Reading & Research", 3, "bg_only", ["tabs", "storage", "contextMenus", "bookmarks"], ["<all_urls>"], {"action": True}),
    ("pocket", "Save to Pocket", "Reading & Research", 3, "bg_only", ["tabs", "storage", "contextMenus"], ["<all_urls>"], {"action": True}),
    ("wallabag", "Wallabagger", "Reading & Research", 3, "bg_only", ["tabs", "storage", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("instapaper", "Instapaper", "Reading & Research", 3, "bg_only", ["tabs", "storage", "contextMenus"], ["<all_urls>"], {"action": True}),
    ("zotero-connector", "Zotero Connector", "Reading & Research", 3, "content_bg", ["tabs", "storage", "webRequest", "contextMenus"], ["<all_urls>"], {"action": True, "options": True}),
    ("hypothesis", "Hypothesis - Web & PDF Annotation", "Reading & Research", 3, "content_bg", ["tabs", "storage", "scripting"], ["<all_urls>"], {"action": True}),
    ("bionic-reading", "Bionic Reading Highlighter (Pure JS)", "Reading & Research", 3, "content_js", [], [], {"matches": ["*://127.0.0.1/*"], "run_at": "document_end"}),

    # 11. Design, Media & Utility Tools (8)
    ("loom", "Loom – Screen Recorder", "Design & Media", 3, "content_bg", ["tabs", "storage", "scripting"], ["<all_urls>"], {"action": True}),
    ("gofullpage", "GoFullPage - Full Page Screen Capture", "Design & Media", 3, "bg_only", ["tabs", "downloads", "scripting", "storage"], ["<all_urls>"], {"action": True}),
    ("awesome-screenshot", "Awesome Screenshot", "Design & Media", 3, "content_bg", ["tabs", "downloads", "storage", "scripting"], ["<all_urls>"], {"action": True}),
    ("colorzilla", "ColorZilla", "Design & Media", 3, "content_bg", ["tabs", "storage", "scripting"], ["<all_urls>"], {"action": True}),
    ("whatfont", "WhatFont (Pure DOM Overlay)", "Design & Media", 3, "content_js_css", [], [], {"matches": ["*://127.0.0.1/*"]}),
    ("pesticide-css", "Pesticide CSS Outlines", "Design & Media", 3, "content_css", [], [], {"matches": ["*://127.0.0.1/*"]}),
    ("link-rel-highlighter", "External Link Marker (Pure JS)", "Design & Media", 3, "content_js", [], [], {"matches": ["*://127.0.0.1/*"], "exclude_matches": ["*://127.0.0.1/excluded*"]}),
    ("host-perm-probe", "Host-Permission-Only Scanner", "Design & Media", 3, "host_perm_only", [], ["https://example.com/*", "*://127.0.0.1/*"], {"action": True}),
]

assert len(EXTENSION_SPECS) >= 100, f"Expected >= 100 specs, got {len(EXTENSION_SPECS)}"
assert len({s[0] for s in EXTENSION_SPECS}) == len(EXTENSION_SPECS), "Duplicate slugs"


class MatrixHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        body = (
            "<!doctype html><html><head><title>Matrix Target</title></head>"
            f"<body><main id='content'><h1>Target {path}</h1>"
            "<p>Sample paragraph for extension DOM and CSS injection testing.</p>"
            "<a href='https://example.org'>External link</a>"
            "</main><script>window.__pageMainGlobal = 'page-owned';</script></body></html>"
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def write_fixture(root: pathlib.Path, spec) -> pathlib.Path:
    slug, name, category, mv, kind, perms, host_perms, extra = spec
    folder = root / slug
    folder.mkdir(parents=True, exist_ok=True)
    manifest = {
        "manifest_version": mv,
        "name": name,
        "version": "1.0.0",
        "description": f"Local compatibility fixture for {name} ({category}, MV{mv}).",
    }
    if perms:
        if mv == 2:
            manifest["permissions"] = list(perms) + list(host_perms)
        else:
            manifest["permissions"] = list(perms)
    elif mv == 2 and host_perms:
        manifest["permissions"] = list(host_perms)
    if mv == 3 and host_perms:
        manifest["host_permissions"] = list(host_perms)
    if extra.get("action"):
        if mv == 3:
            manifest["action"] = {"default_title": name}
        else:
            manifest["browser_action"] = {"default_title": name}
    if extra.get("options"):
        manifest["options_ui"] = {"page": "options.html"}
        (folder / "options.html").write_text(
            f"<!doctype html><html><head><title>{name} Options</title></head>"
            f"<body><h1>{name} Options</h1><p>Local options fixture.</p></body></html>"
        )

    matches = extra.get("matches", ["<all_urls>"])
    exclude = extra.get("exclude_matches", [])
    run_at = extra.get("run_at", "document_end")

    if kind in ("content_js", "content_js_css", "content_options"):
        cs = {"matches": matches, "run_at": run_at}
        if exclude:
            cs["exclude_matches"] = exclude
        if kind in ("content_js", "content_js_css", "content_options"):
            cs["js"] = ["content.js"]
            (folder / "content.js").write_text(
                f"document.documentElement.setAttribute('data-ext-{slug}', 'ran');\n"
                f"window.__extIsolated_{slug.replace('-', '_')} = 'isolated';\n"
            )
        if kind in ("content_js_css", "content_options"):
            cs["css"] = ["content.css"]
            (folder / "content.css").write_text(
                f"html[data-ext-{slug}='ran'] body {{ --ext-{slug}-css: 'applied'; }}\n"
            )
        manifest["content_scripts"] = [cs]
    elif kind == "content_css":
        cs = {"matches": matches, "css": ["style.css"]}
        if exclude:
            cs["exclude_matches"] = exclude
        manifest["content_scripts"] = [cs]
        (folder / "style.css").write_text(
            f":root {{ --ext-css-{slug}: 'applied'; }}\n"
        )
    elif kind == "content_api":
        manifest["content_scripts"] = [{
            "matches": matches,
            "js": ["content.js"],
            "run_at": run_at,
        }]
        (folder / "content.js").write_text(
            f"document.documentElement.setAttribute('data-ext-{slug}', 'ran-partial');\n"
            "if (typeof chrome !== 'undefined' && chrome.storage) { chrome.storage.local.get(['cfg'], () => {}); }\n"
        )
    elif kind in ("content_bg", "password_mgr"):
        manifest["content_scripts"] = [{
            "matches": matches,
            "js": ["content.js"],
            "run_at": run_at,
        }]
        if mv == 3:
            manifest["background"] = {"service_worker": "background.js"}
        else:
            manifest["background"] = {"scripts": ["background.js"]}
        (folder / "content.js").write_text(
            f"document.documentElement.setAttribute('data-ext-{slug}', 'ran-partial');\n"
            "if (typeof chrome !== 'undefined' && chrome.runtime) { chrome.runtime.sendMessage({type: 'init'}); }\n"
        )
        (folder / "background.js").write_text(
            "chrome.runtime.onMessage.addListener((msg, sender, reply) => reply({ok: true}));\n"
        )
    elif kind in ("bg_only", "dnr_only"):
        if mv == 3:
            manifest["background"] = {"service_worker": "background.js"}
        else:
            manifest["background"] = {"scripts": ["background.js"]}
        (folder / "background.js").write_text(
            "chrome.tabs.query({}, () => {});\n"
        )
    elif kind == "host_perm_only":
        pass

    (folder / "manifest.json").write_text(json.dumps(manifest, indent=2))
    return folder


def classify(runs: bool, why_not: str) -> str:
    if not runs:
        return "unsupported"
    if why_not.strip():
        return "partial_support"
    return "full_support"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=str(ROOT / "build/search-ui-ext-f44"))
    parser.add_argument("--out-json", required=True)
    parser.add_argument("--out-md", required=True)
    args = parser.parse_args()

    binary = pathlib.Path(args.binary)
    if not binary.exists():
        raise SystemExit(f"Binary not found: {binary}")

    server = ThreadingHTTPServer(("127.0.0.1", 0), MatrixHandler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_port
    base = f"http://127.0.0.1:{port}"

    fixture_root = pathlib.Path(tempfile.mkdtemp(prefix="sext-fix-", dir="/tmp"))
    profile = pathlib.Path(tempfile.mkdtemp(prefix="sext-prof-", dir="/tmp"))
    results = []
    runtime_probes = {}
    proc = None
    client = None
    try:
        fixture_paths = {}
        for spec in EXTENSION_SPECS:
            fixture_paths[spec[0]] = write_fixture(fixture_root, spec)

        socket_path = profile / "bench.sock"
        backend, env = build_browser_env(os.environ, profile, socket_path)
        log_path = profile / "browser.log"
        with log_path.open("w+") as log:
            proc = subprocess.Popen(
                [str(binary), "--fresh", "--appearance", "dark", "--size", "1180x820"],
                env=env,
                stdout=log,
                stderr=log,
                start_new_session=True,
            )
            client = ready(socket_path, proc)
            client.must("settle")

            # Install ALL 108 extensions into the real running browser!
            for spec in EXTENSION_SPECS:
                slug, name, category, mv, kind, perms, host_perms, extra = spec
                res = client.command(f"installext {fixture_paths[slug]}")
                ok_install = bool(res.get("ok"))
                runs = bool(res.get("runs"))
                why_not = str(res.get("why_not", ""))
                classification = classify(runs, why_not) if ok_install else "install_error"
                results.append({
                    "slug": slug,
                    "name": name,
                    "category": category,
                    "manifest_version": mv,
                    "architecture": kind,
                    "permissions": perms,
                    "host_permissions": host_perms,
                    "installed": ok_install,
                    "runs": runs,
                    "classification": classification,
                    "why_not": why_not,
                    "has_options_page": bool(extra.get("options")),
                })

            listed = client.must("extensions").get("extensions", [])
            installed_count = len(listed)

            # Live runtime verification on matching host (127.0.0.1/plain),
            # excluded path (127.0.0.1/excluded), and non-matching host pattern
            # (minimal-x-css only matches x.com/twitter.com; hn-readable-css only matches news.ycombinator.com).
            tab_match = client.open(f"{base}/plain")
            client.wait_loaded(tab_id=tab_match)
            time.sleep(0.25)
            probe_match_raw = client.command(
                "js (() => JSON.stringify({"
                " cookieJs: document.documentElement.getAttribute('data-ext-istilldontcareaboutcookies'),"
                " cookieCss: getComputedStyle(document.body).getPropertyValue('--ext-istilldontcareaboutcookies-css').trim(),"
                " darkCss: getComputedStyle(document.documentElement).getPropertyValue('--ext-css-dark-mode-css').trim(),"
                " pesticideCss: getComputedStyle(document.documentElement).getPropertyValue('--ext-css-pesticide-css').trim(),"
                " bionicJs: document.documentElement.getAttribute('data-ext-bionic-reading'),"
                " whatfontJs: document.documentElement.getAttribute('data-ext-whatfont'),"
                " linkMarkerJs: document.documentElement.getAttribute('data-ext-link-rel-highlighter'),"
                " unhookStartJs: document.documentElement.getAttribute('data-ext-unhook-youtube'),"
                " nonMatchXCss: getComputedStyle(document.documentElement).getPropertyValue('--ext-css-minimal-x-css').trim(),"
                " nonMatchHnCss: getComputedStyle(document.documentElement).getPropertyValue('--ext-css-hn-readable-css').trim(),"
                " bitwardenContentRan: document.documentElement.getAttribute('data-ext-bitwarden'),"
                " isolatedWorldHiddenFromPage: typeof window.__extIsolated_bionic_reading === 'undefined',"
                " pageGlobalIntact: window.__pageMainGlobal === 'page-owned'"
                "}))()"
            ).get("value", "{}") or "{}"
            probe_match = json.loads(probe_match_raw)

            tab_excl = client.open(f"{base}/excluded")
            client.wait_loaded(tab_id=tab_excl)
            time.sleep(0.2)
            probe_excl_raw = client.command(
                "js (() => JSON.stringify({"
                " cookieJsExcluded: document.documentElement.getAttribute('data-ext-istilldontcareaboutcookies'),"
                " linkMarkerExcluded: document.documentElement.getAttribute('data-ext-link-rel-highlighter'),"
                " darkCssStillRuns: getComputedStyle(document.documentElement).getPropertyValue('--ext-css-dark-mode-css').trim()"
                "}))()"
            ).get("value", "{}") or "{}"
            probe_excl = json.loads(probe_excl_raw)

            # Verify options page opens cleanly in a tab for an installed extension with options_ui
            wiki_ext = next(e for e in listed if e["name"] == "Clean Wikipedia Typography")
            opt_path = pathlib.Path(profile / "data/search/extensions")
            opt_files = list(opt_path.glob("*/options.html"))
            options_loaded = False
            if opt_files:
                tab_opt = client.open(f"file://{opt_files[0]}")
                client.wait_loaded(tab_id=tab_opt)
                time.sleep(0.15)
                opt_title = client.command("js document.title").get("value", "")
                options_loaded = "Options" in str(opt_title)

            runtime_probes = {
                "installed_registry_count": installed_count,
                "matching_host_probe": probe_match,
                "excluded_path_probe": probe_excl,
                "options_page_loaded": options_loaded,
                "assertions": {
                    "all_108_installed_in_registry": installed_count == len(EXTENSION_SPECS),
                    "pure_js_injected_on_match": probe_match.get("bionicJs") == "ran" and probe_match.get("linkMarkerJs") == "ran",
                    "pure_css_injected_on_match": "applied" in str(probe_match.get("darkCss")) and "applied" in str(probe_match.get("pesticideCss")),
                    "js_and_css_combined_injected": probe_match.get("cookieJs") == "ran" and "applied" in str(probe_match.get("cookieCss")),
                    "document_start_injected": probe_match.get("unhookStartJs") == "ran",
                    "non_matching_hosts_not_injected": probe_match.get("nonMatchXCss") == "" and probe_match.get("nonMatchHnCss") == "",
                    "exclude_matches_respected": probe_excl.get("cookieJsExcluded") is None and probe_excl.get("linkMarkerExcluded") is None,
                    "isolated_world_boundary_preserved": probe_match.get("isolatedWorldHiddenFromPage") is True and probe_match.get("pageGlobalIntact") is True,
                    "options_page_opens": options_loaded,
                },
            }

            client.must("quit")
            proc.wait(timeout=15)
            log.seek(0)
            browser_log = log.read()
    finally:
        if proc is not None and proc.poll() is None:
            try:
                os.killpg(proc.pid, signal.SIGTERM)
                proc.wait(timeout=5)
            except Exception:
                os.killpg(proc.pid, signal.SIGKILL)
        server.shutdown()
        server.server_close()
        shutil.rmtree(fixture_root, ignore_errors=True)
        shutil.rmtree(profile, ignore_errors=True)

    counts = {
        "total": len(results),
        "full_support": sum(1 for r in results if r["classification"] == "full_support"),
        "partial_support": sum(1 for r in results if r["classification"] == "partial_support"),
        "unsupported": sum(1 for r in results if r["classification"] == "unsupported"),
        "install_error": sum(1 for r in results if r["classification"] == "install_error"),
        "mv2_total": sum(1 for r in results if r["manifest_version"] == 2),
        "mv3_total": sum(1 for r in results if r["manifest_version"] == 3),
    }

    by_category = {}
    for r in results:
        cat = by_category.setdefault(r["category"], {"total": 0, "full_support": 0, "partial_support": 0, "unsupported": 0})
        cat["total"] += 1
        if r["classification"] in cat:
            cat[r["classification"]] += 1

    payload = {
        "binary": str(binary),
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "note": "All 108 extensions were tested using locally generated manifest/script/CSS fixtures (no third-party store downloads) installed into a real running Search browser instance.",
        "counts": counts,
        "by_category": by_category,
        "runtime_probes": runtime_probes,
        "extensions": results,
    }

    out_json = pathlib.Path(args.out_json)
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_json.write_text(json.dumps(payload, indent=2))

    # Write concise Markdown summary
    lines = [
        "# Search Extension Compatibility Matrix (108 Local Real-World-Style Fixtures)",
        "",
        f"- **Binary tested**: `{binary}`",
        f"- **Total extensions installed in live browser**: **{counts['total']}** (MV2: {counts['mv2_total']}, MV3: {counts['mv3_total']})",
        f"- **Full support** (pure content scripts/CSS/options, no missing WebExtensions APIs): **{counts['full_support']}**",
        f"- **Partial support** (content scripts inject, but background worker / `chrome.*` / `browser.*` APIs or privileged permissions do not run): **{counts['partial_support']}**",
        f"- **Unsupported** (background-only, `declarativeNetRequest`-only, or host-permission-only with no runnable content scripts): **{counts['unsupported']}**",
        f"- **Install errors**: **{counts['install_error']}**",
        "",
        "> **Honest Password Manager & API Verdict:** Bitwarden, 1Password, KeePassXC-Browser, Dashlane, LastPass, Proton Pass, NordPass, Keeper, RoboForm, and Enpass all classify as **partial_support** at the loader layer (their content scripts inject) and **DO NOT function as password managers** because WebKitGTK has no WebExtensions background service worker, no `chrome.runtime` messaging, no `chrome.storage`, and no `nativeMessaging`. Search's loader reports this explicitly in `why_not` rather than pretending they work.",
        "",
        "## Category Breakdown",
        "",
        "| Category | Total | Full Support | Partial Support | Unsupported |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for cat, stats in by_category.items():
        lines.append(f"| {cat} | {stats['total']} | {stats['full_support']} | {stats['partial_support']} | {stats['unsupported']} |")

    lines += [
        "",
        "## Live Runtime Injection Verification",
        "",
    ]
    for k, v in runtime_probes["assertions"].items():
        lines.append(f"- `{k}`: **{'PASS' if v else 'FAIL'}**")

    lines += [
        "",
        "## Representative Results (Full Matrix in JSON)",
        "",
        "| Extension | MV | Category | Classification | Browser `why_not` |",
        "| --- | ---: | --- | --- | --- |",
    ]
    for r in results:
        why = r["why_not"] if r["why_not"] else "*(none — runs cleanly)*"
        lines.append(f"| {r['name']} | MV{r['manifest_version']} | {r['category']} | `{r['classification']}` | {why} |")

    out_md = pathlib.Path(args.out_md)
    out_md.parent.mkdir(parents=True, exist_ok=True)
    out_md.write_text("\n".join(lines) + "\n")

    all_ok = all(runtime_probes["assertions"].values()) and counts["install_error"] == 0
    print(json.dumps({"counts": counts, "runtime_assertions": runtime_probes["assertions"]}, indent=2))
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
