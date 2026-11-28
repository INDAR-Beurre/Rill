# Packaging & Native Linux Distribution Formats

Rill (internal executable name `search`, user-facing alias `rill`) ships native Linux x86_64 packaging recipes and portable AppImage tooling designed around **honest ABI compatibility**:

- **Never bundle host glibc/loader or graphics drivers** (`libc.so.6`, `ld-linux-x86-64.so.2`, `libm.so.6`, `libdrm.so.2`, `libGL*`/`libEGL*`, Mesa DRI, `libsystemd`/`libelogind`) into portable artifacts.
- **Build portable release binaries on an older supported glibc floor** (Debian 12 `bookworm`, glibc 2.36, WebKitGTK 4.1 >= 2.40) so binaries run honestly across Debian 12+, Ubuntu 24.04+, Linux Mint 22+, Pop!_OS 24.04+, Fedora 38+, openSUSE Leap 15.6 / Tumbleweed, and Arch-family systems.
- **Preserve legacy profile identity** across all package formats: the executable remains `search` (with a `rill` symlink), `StartupWMClass=search`, `Icon=search`, and user data remains in `$XDG_CONFIG_HOME/search`, `$XDG_DATA_HOME/search`, and `$XDG_CACHE_HOME/search`. Uninstalling any package never deletes user profile directories.
- **Never overwrite frozen historical release artifacts** (`build/Search-1.0.1-x86_64.AppImage`, `build/Search-1.0.2-x86_64.AppImage`, `build/Search-redesign-x86_64.AppImage`, and `build/Rill-1.1.0-x86_64.AppImage`).

## Package Formats & Installed Layout

All native packages install the same standard FreeDesktop layout:

| Path | Purpose |
| --- | --- |
| `/usr/bin/search` | Native Rill browser binary |
| `/usr/bin/rill` | Symlink to `search` |
| `/usr/share/applications/Search.desktop` | Desktop launcher (`Name=Rill`, `Icon=search`, `StartupWMClass=search`, `Exec=/usr/bin/rill --url %u`) |
| `/usr/share/icons/hicolor/512x512/apps/search.png` | 512×512 PNG application icon |
| `/usr/share/icons/hicolor/scalable/apps/search.svg` | Scalable SVG application icon |
| `/usr/share/metainfo/io.github.indar_beurre.Search.metainfo.xml` | AppStream metadata (`id=io.github.indar_beurre.Search`) |
| `/usr/share/doc/rill/copyright` or `/usr/share/licenses/rill/LICENSE` | MIT license |

### 1. Debian / Ubuntu (`.deb`)

- **Recipe & builder:** `packaging/debian/` (`control.in`, `copyright`, `postinst`, `postrm`) and `tools/package-deb.sh`.
- **Artifact naming:** `rill_<version>-1_amd64.deb` + `.sha256` + `.abi.json`.
- **Supported targets:** Debian 12 (`bookworm`), Debian 13 (`trixie`), Ubuntu 24.04 LTS (`noble`) and newer, Linux Mint 22+, Kali, Pop!_OS 24.04+. Runtime `Depends` use `t64` alternatives (`libgtk-3-0 | libgtk-3-0t64`, `libglib2.0-0 | libglib2.0-0t64`, `libatk1.0-0 | libatk1.0-0t64`) so one Debian 12-built `.deb` installs cleanly on both pre-`t64` and post-`t64` systems.
- **Build (inside Debian 12 / glibc 2.36 environment):**
  ```sh
  tools/package-deb.sh
  ```
- **Install & uninstall:**
  ```sh
  sha256sum -c rill_1.1.0-1_amd64.deb.sha256
  sudo apt install ./rill_1.1.0-1_amd64.deb
  # Uninstall (keeps ~/.config/search and ~/.local/share/search):
  sudo apt remove rill
  ```

### 2. Fedora / openSUSE / RPM (`.rpm`)

- **Recipe & builder:** `packaging/rpm/rill.spec` and `tools/package-rpm.sh`.
- **Artifact naming:** `rill-<version>-1.<dist>.x86_64.rpm` + `.sha256` + `.abi.json`.
- **Dependency model:** Uses RPM's automatic ELF SONAME dependency generator (`libwebkit2gtk-4.1.so.0()(64bit)`, `libgtk-3.so.0()(64bit)`, `libsoup-3.0.so.0()(64bit)`, `libsqlite3.so.0()(64bit)`) plus distro-aware GStreamer base/good plugin dependencies (`gstreamer1-plugins-*` on Fedora, `gstreamer-plugins-*` on openSUSE).
- **Compatibility note:** RHEL 9 / Rocky 9 / AlmaLinux 9 ship `webkit2gtk-4.0` (libsoup 2) rather than `webkit2gtk-4.1` (libsoup 3), so native `.rpm` requires Fedora 38+, openSUSE Leap 15.6 / Tumbleweed, or EL10+.
- **Build (inside a Fedora / openSUSE host with `rpm-build`):**
  ```sh
  tools/package-rpm.sh
  ```
- **Install & uninstall:**
  ```sh
  sha256sum -c rill-1.1.0-1.*.x86_64.rpm.sha256
  sudo dnf install ./rill-1.1.0-1.*.x86_64.rpm
  # Uninstall (keeps ~/.config/search and ~/.local/share/search):
  sudo dnf remove rill
  ```

### 3. Portable Native Tarball (`.tar.gz`)

- **Recipe & builder:** `packaging/tarball/` (`install.sh`, `uninstall.sh`, `README.txt.in`) and `tools/package-tarball.sh`.
- **Artifact naming:** `Rill-<version>-linux-x86_64.tar.gz` + `.sha256` + `.abi.json`.
- **Contents:** `bin/search`, `bin/rill -> search`, desktop/icon/metainfo files, `LICENSE`, `README.txt`, `BUILD_INFO.txt` (exact glibc floor and `DT_NEEDED`), and prefix-aware `install.sh` / `uninstall.sh`.
- **Build (inside Debian 12 / glibc 2.36 environment):**
  ```sh
  tools/package-tarball.sh
  ```
- **Install & uninstall (per-user `~/.local` or system `/usr/local`):**
  ```sh
  sha256sum -c Rill-1.1.0-linux-x86_64.tar.gz.sha256
  tar -xzf Rill-1.1.0-linux-x86_64.tar.gz
  cd Rill-1.1.0-linux-x86_64
  ./install.sh                    # installs to ~/.local (no root needed)
  ./uninstall.sh                  # removes only installed package files
  ```

### 4. Arch Linux / Artix / CachyOS / EndeavourOS (`PKGBUILD`)

- **Recipe & builder:** `packaging/arch/PKGBUILD`, `packaging/arch/.SRCINFO`, and `tools/package-arch.sh`.
- **Artifact naming:** `rill-<version>-1-x86_64.pkg.tar.zst` + `.sha256` + `.abi.json`.
- **Runtime dependencies:** `gtk3`, `webkit2gtk-4.1`, `libsoup3`, `sqlite`, `gst-plugins-base`, `gst-plugins-good`, `hicolor-icon-theme` (optional: `gst-libav`, `gst-plugins-bad`, `gst-plugins-ugly`, `ttf-liberation`).
- **Build locally (isolated `makepkg`, does not modify the host):**
  ```sh
  tools/package-arch.sh
  ```
- **Install & uninstall on Arch-family systems:**
  ```sh
  sha256sum -c rill-1.1.0-1-x86_64.pkg.tar.zst.sha256
  sudo pacman -U ./rill-1.1.0-1-x86_64.pkg.tar.zst
  # Uninstall (keeps ~/.config/search and ~/.local/share/search):
  sudo pacman -R rill
  ```

### 5. Portable AppImage (`.AppImage`)

- **Builder & entrypoint:** `tools/appimage.sh` and `packaging/AppRun`.
- **What it bundles:** Rill binary, WebKitGTK 4.1 + `WebKitWebProcess` / `WebKitNetworkProcess`, `librill_appimage_preload.so` (built from `packaging/rill_appimage_preload.c` to intercept GLib `posix_spawn`/`posix_spawnp` and redirect compiled-in distro WebKit helper paths such as `/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/*` to `$WEBKIT_EXEC_PATH` across Debian, Fedora, and Arch layouts), GTK 3, GIO/GSettings schemas, GStreamer plugins (`libgstautodetect.so`, `libgstaudiofx.so`, `gst-plugin-scanner`), fonts (DejaVu Sans, Liberation Serif + mandatory SIL OFL 1.1 license, optional Adwaita fonts), and desktop/metainfo resources.
- **What it explicitly excludes (`VM-001` regression guard):** `libc.so.*`, `ld-linux*.so*`, `libm.so.*`, `libpthread.so.*`, `libdl.so.*`, `librt.so.*`, `libresolv.so.*`, `libnss_*.so*`, `libgcc_s.so.*`, `libstdc++.so.*`, `libdrm*`, `libGL*`, `libEGL*`, `libgbm*`, `libvulkan*`, `libwayland-*`, `libX11*`, `libxcb*`, `libdbus-1*`, `libelogind*`, `libudev*`, and `/usr/lib/dri/*.so`. `libasound.so.2` is deliberately bundled so minimal Ubuntu/Arch hosts without `alsa-lib` can launch the browser. Note: `libsystemd.so.0` and `libselinux.so.1` remain bundled when built on Debian 12 so WebKitGTK loads cleanly on non-systemd / non-SELinux distributions (Artix, Devuan, Arch, Void). A post-stage audit fails the build if any forbidden host-ABI library enters `AppDir/usr/lib` or if any ELF exceeds `MAX_GLIBC`.
- **Extraction paths with whitespace:** `AppRun` creates a per-run, space-free alias directory under `${TMPDIR:-/tmp}` for the preload shims when the AppDir path contains whitespace. `ld.so` cannot parse whitespace-containing `LD_PRELOAD` entries, so the alias preserves WebKit helper redirection without changing `$HOME` profile identity.
- **Preflight & build (on Debian 12 for glibc 2.36 portability):**
  ```sh
  MAX_GLIBC=2.36 tools/appimage.sh --preflight
  MAX_GLIBC=2.36 APPIMAGE_OUT="$PWD/build/Rill-1.1.0-x86_64-glibc2.36-candidate.AppImage" tools/appimage.sh
  ```
- **Run:**
  ```sh
  sha256sum -c Rill-<version>-x86_64*.AppImage.sha256
  chmod +x Rill-<version>-x86_64*.AppImage
  ./Rill-<version>-x86_64*.AppImage
  # On hosts without FUSE:
  ./Rill-<version>-x86_64*.AppImage --appimage-extract-and-run
  ```

## Offline Package & ABI Verification

Run the offline inspection suite to validate all recipes, shell scripts, desktop/AppStream metadata, uninstall profile safety, historic 1.1.0 preservation, checksums, and built packages:

```sh
python3 tools/package_test.py --artifacts-dir /path/to/artifacts --max-glibc 2.36
```

## Historic 1.1.0 Portability Root Cause (`VM-001`)

The frozen `build/Rill-1.1.0-x86_64.AppImage` (SHA-256 `4fa30c48454b8ada52d23e9317cb40537f1ad25be1355a64c5fd88328bb655e2`, preserved untouched) was built on an Artix Linux host (glibc 2.44) before `tools/appimage.sh` had a host-ABI excludelist. On Debian 12 (glibc 2.36), executing it fails immediately with exit 127:

```text
symbol lookup error: .../usr/lib/libc.so.6: undefined symbol: __pointer_chk_guard, version GLIBC_PRIVATE
```

Because `collect()` copied host `libc.so.6` and `ld-linux-x86-64.so.2` into `AppDir/usr/lib` while the host kernel used Debian's older `/lib64/ld-linux-x86-64.so.2`, and because every bundled Artix library required `GLIBC_2.39`–`GLIBC_2.44`, the 1.1.0 AppImage cannot run on glibc < 2.44 distributions. Rebuilding on Debian 12 (glibc 2.36) with the fixed `tools/appimage.sh` excludelist + `MAX_GLIBC=2.36` gate resolves both defects.
