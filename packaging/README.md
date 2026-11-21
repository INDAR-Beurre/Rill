# AppImage

The x86_64 AppImage contains Search, WebKitGTK, GTK, the WebKit process helpers,
GStreamer, fonts and the required desktop resources. It is not a launcher for a
browser already on your system, and it does not use Chromium.

## Download and run

```sh
chmod +x Search-1.0.1-x86_64.AppImage
./Search-1.0.1-x86_64.AppImage
```

Open a web address directly:

```sh
./Search-1.0.1-x86_64.AppImage https://example.com
```

Settings, tabs, history and bookmarks use the normal XDG user folders. Launching
the AppImage does not reset or overwrite your existing Search profile.

For a desktop-menu entry, extract the image and install the supplied metadata:

```sh
./Search-1.0.1-x86_64.AppImage --appimage-extract
install -Dm644 squashfs-root/Search.desktop ~/.local/share/applications/Search.desktop
install -Dm644 squashfs-root/search.png ~/.local/share/icons/hicolor/512x512/apps/search.png
```

The desktop entry uses `Exec=search %u`; change that line to the absolute path of
your AppImage if you do not also put the AppImage on your `PATH`.

## Architecture and compatibility

- CPU: **x86_64 / AMD64** (the 64-bit x86 family). There is no 32-bit i686 build.
- Display: Wayland or X11 with a running desktop session.
- Current artifact: built on x86_64 CachyOS with glibc 2.44 and WebKitGTK 2.54.2.
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
tools/appimage.sh
```

The script downloads the official AppImage tooling and patchelf when needed,
builds the release, writes `build/Search-1.0.1-x86_64.AppImage`, and writes a
SHA-256 sidecar. It only supports an x86_64 build host at present.

## Verify the exact packaged browser

```sh
SEARCH_BINARY="$PWD/build/Search-1.0.1-x86_64.AppImage" python3 tools/stress.py
sha256sum -c build/Search-1.0.1-x86_64.AppImage.sha256
```

The latest build passed all 117 checks in that full suite, including actual
WebKit navigation, downloads, private tabs, reading mode, sleeping tabs and
session restoration. This verifies the current host, not every Linux
distribution or graphics driver. The one-idle-tab total-PSS target of 100 MB is
still unmet; packaging does not change the engine's memory requirements.
