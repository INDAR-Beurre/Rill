<div align="center">

![Search for Linux: go somewhere interesting](docs/hero.svg)

### A small, quiet, native WebKit browser for Linux.

**Editorial start page · Vertical tabs · Native navigation · Local-first · Shared WebKitGTK engine**

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-38bdf8?style=flat-square&logo=rust&logoColor=white)](Cargo.toml)
[![WebKitGTK](https://img.shields.io/badge/WebKitGTK-4.1-14b8a6?style=flat-square)](docs/ARCHITECTURE.md)
[![Rust tests: 303 passed, 1 ignored (304 total)](https://img.shields.io/badge/Tests-303%20passed%20%C2%B7%201%20ignored-22c55e?style=flat-square)](#testing)
[![License](https://img.shields.io/badge/MIT-license-a5b4fc?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/Linux-Wayland%20%2F%20X11-818cf8?style=flat-square&logo=linux&logoColor=white)](#build-from-source)

[Highlights](#highlights) · [Screenshots](#screenshots) · [AppImage](#appimage) · [Build](#build-from-source) · [Shortcuts](#keyboard-shortcuts) · [Architecture](#architecture) · [Testing](#testing)

</div>

---

**Search** is an independent Linux implementation of [Office Commun's Search](https://github.com/driceroland/Search), written in Rust with GTK 3 and WebKitGTK 4.1. No Electron, no bundled Chromium, and no account. A warm editorial start page, an organized column of vertical tabs, and permanently visible navigation make the real browser approachable with a mouse or keyboard.

> **Local, unreleased makeover.** This README describes the working checkout. Released `1.0.2` AppImages still contain the earlier interface. Memory, timing, and binary-size figures below are historical `1.0.2` measurements, not new measurements of the redesign.

## Highlights

- **A distinct native interface**: Warm paper and charcoal/olive themes, oversized editorial typography, custom orbital artwork, and purposeful native controls—not an embedded dashboard or an HTML theme.
- **An actually useful start page**: Search directly, open saved bookmarks, and return to recent pages. A first-run profile shows clearly labeled curated links; private tabs hide personal records.
- **Discoverable navigation**: Persistent address display, back/forward, reload/stop, site information, reader, bookmark, and menu controls. A foldable vertical tab column (`Ctrl S`) keeps title/host pairs and pinned pages readable.
- **A proper settings workspace**: Larger rail navigation, organized ruled sections, visual theme selectors, honest empty states, and compact-window reflow.
- **Fluid native motion**: Driven by a custom VSync `gdk::FrameClock` engine (`src/motion.rs`) and GTK CSS transitions — one shared set of motion roles (120 ms small changes, 180 ms entrances, 130 ms exits, 220 ms layout moves, 8 px travel), cancellable and reversible sidebar folding, and a 3 px top page-load progress bar.
- **Built for the real web**: Persistent SQLite cookies in normal tabs (strictly isolated ephemeral storage in private tabs), WebRTC, `prefers-color-scheme` sync, locale-derived `Accept-Language` and spellcheck, clipboard permission prompts, and server-suggested download filenames.
- **Engine-level privacy**: EasyList and EasyPrivacy compiled into **15,775 WebKit content-blocker rules** across 3 lists (`network`, global cosmetic, and domain-scoped `if-domain` rules) enforced inside WebKit before requests leave the browser.
- **Fast and lazy by default**: Restored background tabs stay asleep with zero web process until selected. Historical tab-switch timings are listed under [Memory, honestly](#memory-honestly).
- **Power features without bloat**: `Shift` + click link peek cards, `Ctrl I` site security/zoom/permissions/data card, sanitized reading mode (`Ctrl Shift R`), persistent element hiding in an isolated script world (`Ctrl Shift H`), closed-tab stack (`Ctrl Shift T`), `.crx`/`.xpi`/unpacked content-script extensions, and 6-browser import (Firefox, Chromium, Chrome, Brave, Edge, Vivaldi).

> **Explicit scope & limits.** Search has no cloud sync, user accounts, built-in password manager, multi-window mode, or DRM/EME playback. Read [differences from the original](docs/DIFFERENCES.md) and [technical limits](docs/LIMITS.md).

## Screenshots

These are real captures of the GTK browser, taken by `tools/shots.sh` in round 76
(22 November) at 1500x917 from the build whose sha256 is recorded in
`council/round-76/manifest.json`. They are not mockups. The light and dark start
pages, omnibox, pins, reading mode, History and Settings are the `01-default`,
`04-omni`, `02-pins`, `07-reading`, `05-panel-history` and `05-panel-settings`
states from `council/round-76/` (dark: `council/round-76-dark/`).
The pixel checks in `tools/shots.sh` were rebaselined to the warm palette and
now pass in both themes.

<table>
  <tr>
    <td width="50%"><strong>Paper</strong><br><sub>An editorial start page and warm, precise native chrome.</sub></td>
    <td width="50%"><strong>Charcoal</strong><br><sub>The same interface, thoughtfully resolved for dark browsing.</sub></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/light.png" alt="Search in light mode with vertical tabs"></td>
    <td><img src="docs/screenshots/dark.png" alt="Search in dark mode with vertical tabs"></td>
  </tr>
  <tr>
    <td><strong>Omnibox</strong> (<code>Ctrl L</code>)<br><sub>Addresses, search suggestions, history, bookmarks, and open tabs in one place.</sub></td>
    <td><strong>Pinned pages</strong><br><sub>Daily sites stay compact at the top of the column.</sub></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/omnibox.png" alt="Address field with suggestions"></td>
    <td><img src="docs/screenshots/pins.png" alt="Pinned pages and vertical tabs"></td>
  </tr>
</table>

<details>
<summary><strong>More screenshots: Reading mode, History, and Settings</strong></summary>
<br>
<table>
  <tr>
    <td><img src="docs/screenshots/reading.png" alt="Article in reading mode"></td>
    <td><img src="docs/screenshots/history.png" alt="History panel"></td>
  </tr>
  <tr>
    <td align="center"><strong>Reading mode</strong> (<code>Ctrl Shift R</code>)<br><sub>Sanitized article typography without page clutter.</sub></td>
    <td align="center"><strong>History</strong> (<code>Ctrl H</code> / <code>Ctrl Y</code>)<br><sub>Local, fast, and searchable.</sub></td>
  </tr>
</table>

![Settings panel with appearance, privacy and memory options](docs/screenshots/settings.png)

</details>

## AppImage

An **x86_64 / AMD64** AppImage bundles Search, WebKitGTK 4.1, its process helpers, GStreamer plugins, fonts, and the GTK 3 runtime. No installation or root access required.

```sh
chmod +x Search-1.0.2-x86_64.AppImage
./Search-1.0.2-x86_64.AppImage
```

For the local makeover, use a separate output path when rebuilding:

```sh
APPIMAGE_OUT="$PWD/build/Search-redesign-x86_64.AppImage" tools/appimage.sh
```

The packager supports `APPIMAGE_OUT`. The planned
`build/Search-redesign-x86_64.AppImage` is a local, unreleased artifact; do not
overwrite the released `Search-1.0.2-x86_64.AppImage` or its checksum.

See [packaging/README.md](packaging/README.md) for verification, font licenses
(including Liberation Serif), desktop-entry installation, and glibc compatibility
notes.

## Build from source

### Requirements

- **Rust 1.75+**
- **GTK 3** and **WebKitGTK 4.1** (`webkit2gtk-4.1` on Arch, `libwebkit2gtk-4.1-dev` on Debian/Ubuntu)
- A Wayland or X11 session

```sh
cargo build --release
./target/release/search
```

No Node, no JS bundler, no submodules. The stripped `1.0.2` release binary was
**~3.8 MB**; the local makeover has not been remeasured.

<details>
<summary><strong>Install desktop launcher &amp; handle HTTP/HTTPS links</strong></summary>

```sh
cargo install --path . --root ~/.local
mkdir -p ~/.local/share/applications
cat > ~/.local/share/applications/search.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=Search
Comment=A small, fast, quiet browser
Exec=search %u
Terminal=false
Categories=Network;WebBrowser;
MimeType=x-scheme-handler/http;x-scheme-handler/https;
EOF
xdg-mime default search.desktop x-scheme-handler/https
xdg-mime default search.desktop x-scheme-handler/http
```

</details>

## Keyboard shortcuts

| Navigation & Tabs | Key | Page, Panels & Tools | Key |
|---|---|---|---|
| Address / search field | `Ctrl L` | Find in page | `Ctrl F` |
| New tab / Private tab | `Ctrl T` / `Ctrl Shift N` | Reading mode | `Ctrl Shift R` |
| Close / Reopen tab | `Ctrl W` / `Ctrl Shift T` | Site info, zoom & permissions | `Ctrl I` |
| Next / Previous tab | `Ctrl Tab` / `Ctrl Shift Tab` | Peek link in floating card | `Shift` + click |
| Switch tab by typing | `Ctrl K` | Bookmark page | `Ctrl D` |
| Duplicate tab | `Ctrl Shift K` | History / Downloads | `Ctrl H` (`Ctrl Y`) / `Ctrl J` |
| Fold / unfold tab column | `Ctrl S` | Hide element / Restore hidden | `Ctrl Shift H` / `Ctrl Shift U` |
| Settings / All shortcuts | `Ctrl ,` / `Ctrl /` | Dismiss current overlay | `Escape` |

## Architecture

```text
┌─────────────────────── Search (Rust + GTK 3) ───────────────────────┐
│ app.rs · ui.rs · startpage.rs · motion.rs · omnibox.rs · panels.rs   │
│ web.rs · sitecard.rs · peek.rs · curtain.rs · extensions.rs          │
│ store.rs · reader.rs                                                │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ distribution WebKitGTK 4.1
┌──────────────────────────────────▼──────────────────────────────────┐
│ WebKitWebView · WebKitNetworkProcess · UserContentFilterStore       │
└─────────────────────────────────────────────────────────────────────┘
```

- **Key directories**:
  - `src/` — Browser state (`app.rs`), WebKit integration (`web.rs`), GTK shell (`ui.rs`), native start page (`startpage.rs`), VSync motion (`motion.rs`), overlays (`omnibox.rs`, `panels.rs`, `peek.rs`, `sitecard.rs`, `find.rs`, `dialogs.rs`, `permissions.rs`), privacy/blocking (`shield.rs`, `curtain.rs`, `extensions.rs`), and test socket (`bench.rs`).
  - `assets/` — Sanitized reading mode (`reader.js`) and compiled content-blocker lists (`shield/network.json`, `shield/hide-0.json`, `shield/hide-sites.json`).
  - `tools/` — Current GTK visual/behavior walkthrough (`test_ui.py`), stress & memory suite (`stress.py`), legacy screenshot harness (`shots.sh`), shield compiler & checker (`build_shield.py`, `check_shield.py`), extension tests (`extensions.py`), AppImage packager (`appimage.sh`), and figure checker (`report_figures.py`).
  - `docs/` — [Architecture](docs/ARCHITECTURE.md), [Technical Limits](docs/LIMITS.md), [Testing](docs/TESTING.md), and [Differences](docs/DIFFERENCES.md).

## Testing

```sh
cargo test --release                     # 303 passed, 1 ignored (304 total)
python3 tools/test_ui.py                 # Current real GTK walkthrough and captures
python3 tools/stress.py                  # End-to-end behavior and memory checks
GDK_BACKEND=wayland python3 tools/extensions.py
python3 tools/check_shield.py            # Compile the generated WebKit filter lists
python3 tools/report_figures.py --check  # Compare docs with the saved stress report
```

The Rust tally is **303 passing tests and 1 ignored (304 total)** (round 76). `tools/test_ui.py` uses an
isolated profile and real GTK widgets for light/dark appearances, responsive
sizes, actual URL entry, normal/private data separation, settings, navigation
overflow, and screenshot palette review. See [docs/TESTING.md](docs/TESTING.md)
for coverage, capture provenance, and remaining manual checks.

The **117/117 stress checks** and `council/round-47` / `council/round-47-dark`
screenshots are historical `1.0.2` evidence. The old hardcoded pixel assertions
in `tools/shots.sh` do not establish a redesign pass. Reading a saved stress
report with `tools/report_figures.py` is not a new measurement or UI check.

## Memory, honestly

**The original ~100 MB total-RAM request is not met.** Round 75 measured
**218.6 MB total PSS** for one idle tab, including **96.0 MB anonymous PSS**.
Because WebKitGTK's three processes have a higher floor (216-219 MB measured
over rounds 62-75), the stress suite now enforces a documented **230 MB floor**;
this is a re-baselining, not a claim the original request was achieved. It also recorded tab switches at **1.0 ms mean**, **1.7 ms
p95**, and **6.5 ms worst** over 300 switches. These are not new measurements of
the local UI makeover. Total memory varies with WebKit, graphics initialization,
page content, and host state; see [docs/LIMITS.md](docs/LIMITS.md) for memory
definitions and limits.

The suite records `target_pss_mb: 230` and `target_met: true` for that floor rather than reporting anonymous memory as total RAM. Settings → **Use less memory** applies at the next launch. It turns off the page cache, switches WebKit to its document-viewer cache model, skips spellcheck and turns off the compositing path (`WEBKIT_DISABLE_COMPOSITING_MODE=1`), so pages may animate less smoothly. The earlier four-run measurement in [docs/LIMITS.md](docs/LIMITS.md) found the browser's own memory lowest with all of these together. Turning compositing off by itself used more memory, not less. This profile has not been measured against the current round-76 baseline (`council/round-76/stress-report-slide-quick.json`: 218.6 MB total PSS, 96.0 MB own, for one idle tab).

## Origin and license

Search was created for macOS by [Office Commun](https://officecommun.com) ([driceroland/Search](https://github.com/driceroland/Search)). This is an independent Linux implementation in Rust using GTK 3 and WebKitGTK 4.1.

Licensed under [MIT](LICENSE). See [CONTRIBUTING.md](CONTRIBUTING.md) and [CHANGELOG.md](CHANGELOG.md).
