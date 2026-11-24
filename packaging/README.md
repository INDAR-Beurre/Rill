# AppImage

The x86_64 AppImage contains Search, WebKitGTK, GTK, the WebKit process helpers,
GStreamer, fonts and the required desktop resources. It is not a launcher for a
browser already on your system, and it does not use Chromium.

The native UI makeover is local and unreleased. Published `1.0.2` assets still
contain the earlier interface and must remain unchanged. The planned local
candidate is `build/Search-redesign-x86_64.AppImage`, not a replacement release.

## Download and run

```sh
chmod +x Search-1.0.2-x86_64.AppImage
./Search-1.0.2-x86_64.AppImage
```

Open a web address directly:

```sh
./Search-1.0.2-x86_64.AppImage https://example.com
```

Settings, tabs, history and bookmarks use the normal XDG user folders. Launching
the AppImage does not reset or overwrite your existing Search profile.

For a desktop-menu entry, extract the image and install the supplied metadata:

```sh
./Search-1.0.2-x86_64.AppImage --appimage-extract
install -Dm644 squashfs-root/Search.desktop ~/.local/share/applications/Search.desktop
install -Dm644 squashfs-root/search.png ~/.local/share/icons/hicolor/512x512/apps/search.png
```

The desktop entry uses `Exec=search %u`; change that line to the absolute path of
your AppImage if you do not also put the AppImage on your `PATH`.

## Architecture and compatibility

- CPU: **x86_64 / AMD64** (the 64-bit x86 family). There is no 32-bit i686 build.
- Display: Wayland or X11 with a running desktop session.
- Released `1.0.2` baseline: built on x86_64 CachyOS with glibc 2.44 and WebKitGTK 2.54.2.
- Its bundled glibc is the matching 2.44 runtime, so older distributions need an
  artifact built against their older glibc instead. Build on an older supported
  distribution for wider compatibility; do not mix this host's newer libraries
  into an older glibc loader.
- No installation or root access is required. AppImage mounting normally uses
  FUSE; `--appimage-extract-and-run` is available on hosts without FUSE.
- Hardware and kernel graphics drivers are supplied by the host. Mesa userspace
  drivers are included where available.

## Rebuild

```sh
APPIMAGE_OUT="$PWD/build/Search-redesign-x86_64.AppImage" tools/appimage.sh
```

The script downloads the official AppImage tooling and patchelf when needed,
builds the release binary, writes the path selected by `APPIMAGE_OUT`, and writes
a SHA-256 sidecar at `<output>.sha256`. It only supports an x86_64 build host at
present.

Without `APPIMAGE_OUT`, the filename comes from the version in `Cargo.toml`,
currently `build/Search-1.0.2-x86_64.AppImage`. For the unreleased makeover,
always select the separate redesign path above. Do not overwrite the released
`1.0.2` AppImage or its checksum, and do not publish the local candidate as the
released asset. Selecting an output filename does not make a new release.

## Fonts and licenses

The local packager includes DejaVu Sans, Adwaita fonts, and **Liberation Serif**
for the editorial headings. Liberation Serif is licensed under the **SIL Open
Font License 1.1**, separately from Search's MIT license.

`tools/appimage.sh` copies `/usr/share/fonts/liberation/LiberationSerif-*.ttf`
and, when present on the build host, `/usr/share/licenses/ttf-liberation/LICENSE`
to `usr/share/licenses/search/LiberationSerif-LICENSE` inside the AppDir. Ensure
that host license file exists before packaging, and retain it with the fonts in
any distributable artifact. After extraction, its expected location is
`squashfs-root/usr/share/licenses/search/LiberationSerif-LICENSE`.

## Verify the exact packaged browser

```sh
SEARCH_BINARY="$PWD/build/Search-redesign-x86_64.AppImage" python3 tools/test_ui.py
SEARCH_BINARY="$PWD/build/Search-redesign-x86_64.AppImage" python3 tools/stress.py
sha256sum -c build/Search-redesign-x86_64.AppImage.sha256
```

These are verification commands for the planned candidate, not a claim that it
has been built or verified. The current GTK walkthrough and capture provenance
are documented in [docs/TESTING.md](../docs/TESTING.md); the old hardcoded
`tools/shots.sh` pixel checks do not validate the redesign.

The released `1.0.2` artifact historically passed all 117 stress checks,
including actual WebKit navigation, downloads, private tabs, reading mode,
sleeping tabs and
session restoration. That is baseline evidence for its original host, not a new
result for the makeover or a guarantee for every Linux distribution or graphics
driver. Existing memory/performance figures are historical `1.0.2` measurements;
the one-idle-tab total-PSS target of 100 MB remains unmet, and no new measurement
is claimed for the planned package.
