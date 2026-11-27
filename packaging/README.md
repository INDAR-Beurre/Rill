# Packaging & AppImage

The x86_64 AppImage packages **Rill** (internal executable name `search`), WebKitGTK 4.1, GTK 3, the WebKit process
helpers, GStreamer (including `libgstautodetect.so` and `libgstaudiofx.so`),
fonts, licenses, and the required desktop resources. It is not a launcher for a
browser already on your system, and it does not use Chromium.

Rill 1.1.0 publishes `Rill-1.1.0-x86_64.AppImage` with its `.sha256` sidecar. Historic `1.0.1` and `1.0.2` AppImages
(`build/Search-1.0.2-x86_64.AppImage`) and the earlier F39 local candidate
(`build/Search-redesign-x86_64.AppImage`) remain frozen historical artifacts
and must not be overwritten.

## Running the AppImage

```sh
sha256sum -c Rill-1.1.0-x86_64.AppImage.sha256
chmod +x Rill-1.1.0-x86_64.AppImage
./Rill-1.1.0-x86_64.AppImage
```

Open a web address directly (in the current build, use `--url`):

```sh
./build/rill --url https://example.com
```

Settings, tabs, history and bookmarks use the normal XDG user folders (`search`). Launching
an AppImage or local build does not reset or overwrite your existing profile.

For a desktop-menu entry, install the supplied metadata from `packaging/`:

```sh
install -Dm644 packaging/Search.desktop ~/.local/share/applications/Search.desktop
install -Dm644 packaging/search.png ~/.local/share/icons/hicolor/512x512/apps/search.png
```

The desktop entry sets `Name=Rill`, `StartupWMClass=search`, and `Exec=search --url %u`; change `search` to the absolute path of
your binary or AppImage if you do not put it on your `PATH`.

## Architecture and compatibility

- CPU: **x86_64 / AMD64** (the 64-bit x86 family). There is no 32-bit i686 build.
- Display: Wayland or X11 with a running desktop session.
- Current build host: x86_64 Artix Linux with glibc 2.44, WebKitGTK 2.54.2
  (helpers discovered dynamically via `pkg-config --variable=libdir webkit2gtk-4.1`,
  resolving to `/usr/lib/webkit2gtk-4.1` on this host), and GStreamer 1.28.8.
- Released `1.0.2` baseline: built on x86_64 CachyOS with glibc 2.44 and
  WebKitGTK 2.54.2.
- The AppImage uses the host's glibc loader (glibc 2.44 floor for binaries built
  on this host), so older distributions need an artifact built against their
  older glibc instead. Build on an older supported distribution for wider
  compatibility; do not mix this host's newer libraries into an older glibc
  loader.
- No installation or root access is required. AppImage mounting normally uses
  FUSE; `--appimage-extract-and-run` is available on hosts without FUSE.
- Hardware and kernel graphics drivers are supplied by the host. Mesa userspace
  drivers are included where available.

## Preflight and rebuild

Run preflight first to verify host libraries, GStreamer audio plugins, fonts,
and mandatory licenses without modifying `build/AppDir` or building an image:

```sh
tools/appimage.sh --preflight
```

Build the local redesign candidate:

```sh
APPIMAGE_OUT="$PWD/build/Rill-1.1.0-x86_64.AppImage" tools/appimage.sh
```

`tools/appimage.sh` defaults `APPIMAGE_OUT` to
`build/Search-redesign-x86_64.AppImage` and refuses to overwrite an existing
historic `Search-1.0.1-*.AppImage` or `Search-1.0.2-*.AppImage` unless
`ALLOW_OVERWRITE_HISTORIC=1` is set explicitly. The script downloads the
official AppImage tooling and `patchelf` when not already cached in
`build/tools/`, builds the release binary, writes the selected `APPIMAGE_OUT`,
and writes a SHA-256 sidecar at `<output>.sha256`. It only supports an x86_64
build host at present.

### Reproducible dependency overrides

When the build host lacks system-installed `gst-plugins-good` or
`ttf-liberation`, `tools/appimage.sh` resolves compatible user-local caches
automatically (and fails cleanly if any required plugin, font face, or license
is missing or has unresolved shared libraries):

- `WEBKIT_HELPER_DIR`: overrides the WebKitGTK 4.1 helper directory (default:
  discovered from `pkg-config` for `webkit2gtk-4.1` (`libdir`/`libexecdir`/`prefix`) and standard
  distro paths; verified for `WebKitWebProcess` and `WebKitNetworkProcess`).
- `GST_EXTRA_PLUGIN_DIR`: fallback directory for `libgstautodetect.so` and
  `libgstaudiofx.so` matching the host GStreamer ABI (default:
  `$HOME/.cache/search-gst-autodetect/root/usr/lib/gstreamer-1.0`).
- `GST_EXTRA_LICENSE`: LGPL-2.1-or-later license text copied to
  `usr/share/licenses/search/gst-plugins-good-LGPL-2.1-or-later.txt` when
  available.
- `LIBERATION_FONT_DIR`: directory containing all four `LiberationSerif-*.ttf`
  faces (`Regular`, `Bold`, `Italic`, `BoldItalic`; default checks system font
  directories and `$HOME/.cache/search-packaging/ttf-liberation/usr/share/fonts/liberation`).
- `LIBERATION_LICENSE`: mandatory SIL Open Font License 1.1 text copied to
  `usr/share/licenses/search/LiberationSerif-LICENSE` (default checks system
  license paths and `$HOME/.cache/search-packaging/ttf-liberation/usr/share/licenses/ttf-liberation/LICENSE`).
- `DEJAVU_FONT_DIR` / `ADWAITA_FONT_DIR`: optional overrides for DejaVu Sans
  and Adwaita font directories.

## Fonts and licenses

The packager bundles DejaVu Sans, Adwaita fonts, and **Liberation Serif**
(all four faces for serif fallback and web rendering; browser chrome uses clean
sans-serif `Inter` / `Adwaita Sans` / `Noto Sans` / `Cantarell`). Liberation
Serif is licensed under the **SIL Open Font License 1.1**, separately from
Search's MIT license.

`tools/appimage.sh` requires all four `LiberationSerif-*.ttf` faces and the
corresponding OFL license file to be present, copying the license to
`usr/share/licenses/search/LiberationSerif-LICENSE` inside the AppDir (and
refusing to package if either the font faces or license file are missing). It
also bundles `DejaVu-LICENSE`, `AdwaitaFonts-LICENSE`, and
`gst-plugins-good-LGPL-2.1-or-later.txt` under `usr/share/licenses/search/`
when present. After extraction, the expected Liberation Serif license path is
`squashfs-root/usr/share/licenses/search/LiberationSerif-LICENSE`.

## Verify the exact packaged browser

```sh
sha256sum -c build/Search-redesign-x86_64.AppImage.sha256
SEARCH_TEST_BACKEND=x11 SEARCH_UI_OUTPUT=/tmp/search-appimage-ui \
  SEARCH_BINARY="$PWD/build/Search-redesign-x86_64.AppImage" python3 tools/test_ui.py
SEARCH_TEST_BACKEND=x11 \
  SEARCH_BINARY="$PWD/build/Search-redesign-x86_64.AppImage" python3 tools/stress.py
```

Use `SEARCH_TEST_BACKEND=x11` (under X11 or `Xvfb`) for deterministic
programmatic window-resize verification, especially on tiled Wayland desktops
that may override requested widths; pass `SEARCH_UI_OUTPUT=<dir>` to keep
`build/redesign/` untouched. When `SEARCH_TEST_BACKEND` is unset, both
`tools/test_ui.py` and `tools/stress.py` preserve Wayland if `WAYLAND_DISPLAY`
is set and choose X11 automatically when only `DISPLAY` is set. Native Wayland
runs remain useful for interactive compositor smoke checks, and X11/`Xvfb`
verification is separate from measured native Wayland or hardware-compositor
motion performance.

Design F39 verification against the exact packaged artifact
`build/Search-redesign-x86_64.AppImage` (125.9 MiB, SHA-256
`d5b00497b2a87d6f1941081a2f2d73fcd3835e92130b447e3fca2586c11214bb`):
- `sha256sum -c build/Search-redesign-x86_64.AppImage.sha256`: **OK** (historic
  `build/Search-1.0.2-x86_64.AppImage` untouched at `7393bab26511…`).
- Embedded `usr/bin/search` SHA-256: `c5cc0d19b7fad79d0882a283f8f5329616e67f70d3973e73a8a6fea4ea6569da` (derived from release
  binary `c2974efd71a0660e631806a05727b0be51d0815e58b3b0179420c007c665f624`, differing only by `patchelf --set-rpath '$ORIGIN/../lib'`, zero missing libs).
- Extracted AppDir verified to contain all four `LiberationSerif-*.ttf` faces,
  mandatory `LiberationSerif-LICENSE` (SIL OFL 1.1), `DejaVu-LICENSE`,
  `AdwaitaFonts-LICENSE`, `libgstautodetect.so` + `libgstaudiofx.so` +
  `gst-plugins-good-LGPL-2.1-or-later.txt`, and WebKitGTK 4.1 helpers under
  `usr/lib/webkit2gtk-4.1/`.
- Native FUSE execution (`SEARCH_BINARY="$PWD/build/Search-redesign-x86_64.AppImage"`):
  full `tools/test_ui.py` **PASSED** (0 GTK warnings/criticals/allocations),
  **3 consecutive** `tools/stress.py --quick` runs **151/151 passed**
  (`build/stress-report-f39-appimage-quick-{1,2,3}.json`, Run #1 with auto-X11
  and `SEARCH_TEST_BACKEND` unset: one idle tab at **213.9 / 196.2 / 213.6 MB**
  warm PSS, **216.1 / 198.4 / 215.9 MB** cold PSS, **94.1 / 76.5 / 93.8 MB** own,
  **413.1 / 395.5 / 413.0 MB** RSS, **3** processes across idle, History,
  Bookmarks, and Settings open/close with zero `glycin`/`bwrap`),
  and live HTTPS (`example.com`, Wikipedia Reader, `expired.badssl.com` TLS +
  `errorfocus`), live non-`SEARCH_BENCH` folded controls, and local
  WebAudio/`<audio>` + live `https://github.com/` smoke all stayed crash-free
  with exit 0 and 0 bad stderr lines.

The released `1.0.2` artifact historically passed all 117 stress checks on its
original host. The full Design F39 source-build stress run
(`docs/benchmarks/stress-report-f39.json`: **151/151 checks passed**)
measured one idle tab at **182.9 MB** warm total PSS
(185.3 MB cold), **94.5 MB** own anonymous PSS,
and **441.9 MB** RSS across **3** processes.
The original ~100 MB total-PSS target remains unmet (230 MB WebKitGTK floor is
a separate baseline). See [docs/TESTING.md](../docs/TESTING.md) and
[docs/design-f39-evidence.md](../docs/design-f39-evidence.md) (historical F38
preserved at [docs/design-f38-evidence.md](../docs/design-f38-evidence.md)).
