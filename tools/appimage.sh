#!/usr/bin/env bash
# Package the actual release binary and its WebKit helpers, not a wrapper around
# a system browser. The host needs a graphical session only for the smoke test.
#
# Portability contract:
#   - Core host-ABI libraries (glibc/ld-linux, libm/libpthread/libdl, libgcc_s,
#     libstdc++, libdrm/Mesa/GL/EGL/Wayland/X11, systemd/elogind/dbus, etc.)
#     are NEVER bundled into the AppImage.
#   - Build on an older supported distribution (e.g. Debian 12, glibc 2.36) for
#     cross-distro compatibility; enforce with MAX_GLIBC=2.36.
#
# Usage:
#   tools/appimage.sh               # build candidate AppImage + .sha256 + .abi.json
#   tools/appimage.sh --preflight   # validate host & cached dependencies only
set -euo pipefail

PREFLIGHT_ONLY=0
SKIP_CARGO_BUILD=0
for arg in "$@"; do
  case "$arg" in
    --preflight) PREFLIGHT_ONLY=1 ;;
    --skip-cargo-build) SKIP_CARGO_BUILD=1 ;;
    -h|--help)
      echo "Usage: $0 [--preflight] [--skip-cargo-build]"
      echo "  APPIMAGE_OUT          output AppImage path (default: build/Rill-<ver>-x86_64-candidate.AppImage)"
      echo "  SEARCH_BINARY         pre-built release binary path (default: target/release/search)"
      echo "  MAX_GLIBC             fail build if any bundled ELF requires a newer GLIBC_* symbol (e.g. 2.36)"
      echo "  RILL_VERSION          override version read from Cargo.toml"
      echo "  WEBKIT_HELPER_DIR     override WebKitGTK 4.1 helper directory"
      echo "  GST_EXTRA_PLUGIN_DIR  fallback dir for libgstautodetect.so and libgstaudiofx.so"
      echo "  GST_EXTRA_LICENSE     optional LGPL license path for bundled GStreamer plugins"
      echo "  LIBERATION_FONT_DIR   override dir containing LiberationSerif-*.ttf"
      echo "  LIBERATION_LICENSE    override path to Liberation Serif SIL OFL license"
      echo "  DEJAVU_FONT_DIR       override dir containing DejaVuSans*.ttf"
      echo "  ADWAITA_FONT_DIR      optional dir containing Adwaita fonts"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg (expected --preflight, --skip-cargo-build, or --help)" >&2
      exit 2
      ;;
  esac
done

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BUILD="$ROOT/build"
APPDIR="${APPIMAGE_APPDIR:-$BUILD/AppDir}"
VERSION="${RILL_VERSION:-$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$ROOT/Cargo.toml")}"
OUT="${APPIMAGE_OUT:-$BUILD/Rill-${VERSION}-x86_64-candidate.AppImage}"
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac
if [[ -n "${SEARCH_BINARY:-}" ]]; then
  case "$SEARCH_BINARY" in /*) ;; *) SEARCH_BINARY="$PWD/$SEARCH_BINARY" ;; esac
fi
TOOL="${APPIMAGETOOL:-$BUILD/tools/appimagetool-x86_64.AppImage}"
HOOK="${APPRUN_HOOK:-$BUILD/tools/libapprun_hooks-x86_64.so}"
APPRUN_LICENSE="${APPRUN_LICENSE_PATH:-$BUILD/tools/AppRun-LICENSE}"
GST_EXTRA_PLUGIN_DIR="${GST_EXTRA_PLUGIN_DIR:-$HOME/.cache/search-gst-autodetect/root/usr/lib/gstreamer-1.0}"
GST_EXTRA_LICENSE_DEFAULT="$HOME/.cache/search-gst-autodetect/root/usr/share/licenses/gst-plugins-good/LICENSE"
LIBERATION_CACHE_ROOT="$HOME/.cache/search-packaging/ttf-liberation"

# Verified SHA-256 digests for official upstream tooling downloads.
HOOK_URL="https://github.com/AppImageCrafters/AppRun/releases/download/v2.0.0/libapprun_hooks-Release-x86_64.so"
HOOK_SHA256="${APPRUN_HOOK_SHA256:-1f64dd2161be3242f106c891b2daf27bbdd2103c20b28f57f61a56cdccb1eb6a}"
APPRUN_LICENSE_URL="https://raw.githubusercontent.com/AppImageCrafters/AppRun/master/LICENSE"
APPRUN_LICENSE_SHA256="${APPRUN_LICENSE_SHA256:-5ef8cd7c08c4831c4af5083e6d4efd4d9468aa70c6e272c2754dcc1266eba25d}"
PATCHELF_URL="https://github.com/NixOS/patchelf/releases/download/0.19.3/patchelf-0.19.3-x86_64.tar.gz"
PATCHELF_BIN_SHA256="${PATCHELF_BIN_SHA256:-3acab15a88e5150b394a051fa573a0bc4faf354decfa8710c901bfa1a051ed56}"
APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
APPIMAGETOOL_SHA256="${APPIMAGETOOL_SHA256:-e8c1169c4af66c3184d7ee110713d4be04befe671fa358e2c1591ee840553b03}"

[[ $(uname -m) == x86_64 ]] || { echo 'This script builds x86_64 only.' >&2; exit 2; }

# Never silently overwrite historic release artifacts (1.0.1, 1.0.2, F39 redesign, 1.1.0).
case "$(basename "$OUT")" in
  Search-1.0.1-*.AppImage|Search-1.0.2-*.AppImage|Search-redesign-*.AppImage|Rill-1.1.0-x86_64.AppImage)
    if [[ -e "$OUT" && "${ALLOW_OVERWRITE_HISTORIC:-0}" != "1" ]]; then
      echo "Refusing to overwrite historic artifact $OUT." >&2
      echo "Set APPIMAGE_OUT to a new path (default: $BUILD/Rill-${VERSION}-x86_64-candidate.AppImage) or ALLOW_OVERWRITE_HISTORIC=1." >&2
      exit 4
    fi
    ;;
esac

# --- Dynamic host & cache dependency discovery (preflight) ---

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required host tool: $cmd" >&2
    exit 3
  fi
}

find_first_cmd() {
  local cand
  for cand in "$@"; do
    [[ -n "$cand" ]] || continue
    if [[ "$cand" == */* ]]; then
      [[ -x "$cand" ]] && { printf '%s\n' "$cand"; return 0; }
    elif command -v "$cand" >/dev/null 2>&1; then
      command -v "$cand"
      return 0
    fi
  done
  return 1
}

find_first_dir() {
  local dir
  for dir in "$@"; do
    [[ -n "$dir" && -d "$dir" ]] && { printf '%s\n' "$dir"; return 0; }
  done
  return 1
}

find_first_file() {
  local f
  for f in "$@"; do
    [[ -n "$f" && -f "$f" ]] && { printf '%s\n' "$f"; return 0; }
  done
  return 1
}

verify_sha256() {
  local path="$1" expected="$2" label="$3"
  [[ -n "$expected" ]] || return 0
  local actual
  actual=$(sha256sum "$path" | awk '{print $1}')
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $label ($path):" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 5
  fi
}

for cmd in pkg-config file ldd readelf sha256sum glib-compile-schemas fc-cache; do
  require_cmd "$cmd"
done

GDK_PIXBUF_QUERY_LOADERS=$(find_first_cmd \
  gdk-pixbuf-query-loaders \
  /usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders \
  /usr/lib64/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders \
  /usr/bin/gdk-pixbuf-query-loaders-64 || true)

GTK_QUERY_IMMODULES=$(find_first_cmd \
  gtk-query-immodules-3.0 \
  /usr/lib/x86_64-linux-gnu/libgtk-3-0/gtk-query-immodules-3.0 \
  /usr/lib/x86_64-linux-gnu/libgtk-3-0t64/gtk-query-immodules-3.0 \
  /usr/bin/gtk-query-immodules-3.0-64 || true)

GIO_QUERYMODULES=$(find_first_cmd \
  gio-querymodules \
  /usr/lib/x86_64-linux-gnu/glib-2.0/gio-querymodules \
  /usr/bin/gio-querymodules-64 || true)

# 1. Discover WebKitGTK 4.1 helper directory from the actual host.
WEBKIT_PC_LIBDIR=$(pkg-config --variable=libdir webkit2gtk-4.1 2>/dev/null || true)
WEBKIT_PC_LIBEXECDIR=$(pkg-config --variable=libexecdir webkit2gtk-4.1 2>/dev/null || true)
WEBKIT_PC_PREFIX=$(pkg-config --variable=prefix webkit2gtk-4.1 2>/dev/null || true)
WEBKIT_HELPER_CANDIDATES=()
if [[ -n "${WEBKIT_HELPER_DIR:-}" ]]; then
  WEBKIT_HELPER_CANDIDATES=("$WEBKIT_HELPER_DIR")
else
  [[ -n "$WEBKIT_PC_LIBDIR" ]] && WEBKIT_HELPER_CANDIDATES+=("$WEBKIT_PC_LIBDIR/webkit2gtk-4.1")
  [[ -n "$WEBKIT_PC_LIBEXECDIR" ]] && WEBKIT_HELPER_CANDIDATES+=("$WEBKIT_PC_LIBEXECDIR/webkit2gtk-4.1")
  [[ -n "$WEBKIT_PC_PREFIX" ]] && WEBKIT_HELPER_CANDIDATES+=("$WEBKIT_PC_PREFIX/libexec/webkit2gtk-4.1")
  WEBKIT_HELPER_CANDIDATES+=(
    "/usr/lib/webkit2gtk-4.1"
    "/usr/lib64/webkit2gtk-4.1"
    "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
    "/usr/libexec/webkit2gtk-4.1"
  )
fi
RESOLVED_WEBKIT_DIR=""
for cand in "${WEBKIT_HELPER_CANDIDATES[@]}"; do
  if [[ -d "$cand" && -x "$cand/WebKitWebProcess" && -x "$cand/WebKitNetworkProcess" ]]; then
    RESOLVED_WEBKIT_DIR="$cand"
    break
  fi
done
if [[ -z "$RESOLVED_WEBKIT_DIR" ]]; then
  echo "Missing WebKitGTK 4.1 helper directory (WebKitWebProcess / WebKitNetworkProcess not found)." >&2
  echo "Install webkit2gtk-4.1 or set WEBKIT_HELPER_DIR." >&2
  exit 3
fi

# 2. Discover GTK / GDK-Pixbuf / GIO / GStreamer host module directories.
GDK_PIXBUF_LIBDIR=$(pkg-config --variable=libdir gdk-pixbuf-2.0 2>/dev/null || true)
GIO_LIBDIR=$(pkg-config --variable=libdir gio-2.0 2>/dev/null || true)
GTK3_LIBDIR=$(pkg-config --variable=libdir gtk+-3.0 2>/dev/null || true)
GST_PC_PLUGINSDIR=$(pkg-config --variable=pluginsdir gstreamer-1.0 2>/dev/null || true)

# gdk-pixbuf >= 2.44 uses built-in/glycin loaders and may not have an external
# gdk-pixbuf-2.0 module directory; treat it as optional when absent.
GDK_PIXBUF_DIR=$(find_first_dir \
  "${GDK_PIXBUF_LIBDIR:+$GDK_PIXBUF_LIBDIR/gdk-pixbuf-2.0}" \
  /usr/lib/gdk-pixbuf-2.0 \
  /usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0 \
  /usr/lib64/gdk-pixbuf-2.0 || true)

for share_dir in /usr/share/glib-2.0 /usr/share/mime /usr/share/icons/Adwaita /usr/share/X11; do
  if [[ ! -d "$share_dir" ]]; then
    echo "Missing required host data directory: $share_dir" >&2
    exit 3
  fi
done
GTK3_SHARE_DIR=$(find_first_dir /usr/share/gtk-3.0 /usr/share/themes/Default || true)

GIO_DIR=$(find_first_dir \
  "${GIO_LIBDIR:+$GIO_LIBDIR/gio}" \
  /usr/lib/gio \
  /usr/lib/x86_64-linux-gnu/gio \
  /usr/lib64/gio) || {
  echo "Missing gio module directory." >&2; exit 3;
}
GTK3_DIR=$(find_first_dir \
  "${GTK3_LIBDIR:+$GTK3_LIBDIR/gtk-3.0}" \
  /usr/lib/gtk-3.0 \
  /usr/lib/x86_64-linux-gnu/gtk-3.0 \
  /usr/lib64/gtk-3.0) || {
  echo "Missing gtk-3.0 module directory." >&2; exit 3;
}
GST_HOST_PLUGIN_DIR=$(find_first_dir \
  "$GST_PC_PLUGINSDIR" \
  /usr/lib/gstreamer-1.0 \
  /usr/lib/x86_64-linux-gnu/gstreamer-1.0 \
  /usr/lib64/gstreamer-1.0) || {
  echo "Missing host gstreamer-1.0 plugin directory." >&2; exit 3;
}
GST_SCANNER_BIN=$(find_first_cmd \
  "$GST_HOST_PLUGIN_DIR/gst-plugin-scanner" \
  /usr/lib/x86_64-linux-gnu/gstreamer1.0/gstreamer-1.0/gst-plugin-scanner \
  /usr/libexec/gstreamer-1.0/gst-plugin-scanner \
  /usr/lib/gstreamer-1.0/gst-plugin-scanner || true)

# 3. Validate required GStreamer audio plugins (autodetect + audiofx).
declare -A RESOLVED_GST_PLUGINS=()
for plugin in libgstautodetect.so libgstaudiofx.so; do
  plugin_path=""
  if [[ -f "$GST_HOST_PLUGIN_DIR/$plugin" ]]; then
    plugin_path="$GST_HOST_PLUGIN_DIR/$plugin"
  elif [[ -f "$GST_EXTRA_PLUGIN_DIR/$plugin" ]]; then
    plugin_path="$GST_EXTRA_PLUGIN_DIR/$plugin"
  else
    echo "Missing required GStreamer plugin $plugin: install gst-plugins-good or set GST_EXTRA_PLUGIN_DIR." >&2
    exit 3
  fi
  if ldd "$plugin_path" 2>&1 | grep -q 'not found'; then
    echo "GStreamer plugin $plugin_path has unresolved shared libraries:" >&2
    ldd "$plugin_path" 2>&1 | grep 'not found' >&2 || true
    exit 3
  fi
  RESOLVED_GST_PLUGINS["$plugin"]="$plugin_path"
done

if command -v gst-inspect-1.0 >/dev/null 2>&1; then
  gst_probe_path="$GST_HOST_PLUGIN_DIR"
  [[ -d "$GST_EXTRA_PLUGIN_DIR" ]] && gst_probe_path="$GST_EXTRA_PLUGIN_DIR:$gst_probe_path"
  for elem in autoaudiosink scaletempo; do
    if ! GST_PLUGIN_PATH="$gst_probe_path" gst-inspect-1.0 "$elem" >/dev/null 2>&1; then
      echo "GStreamer element '$elem' failed to load via gst-inspect-1.0 (GST_PLUGIN_PATH=$gst_probe_path)." >&2
      exit 3
    fi
  done
fi

RESOLVED_GST_LICENSE=$(find_first_file \
  "${GST_EXTRA_LICENSE:-}" \
  "$GST_EXTRA_LICENSE_DEFAULT" \
  "/usr/share/licenses/gst-plugins-good/LICENSE" \
  "/usr/share/doc/gstreamer1.0-plugins-good/copyright" \
  "/usr/share/licenses/spdx/LGPL-2.1-or-later.txt" || true)

# 4. Validate required fonts and mandatory font licenses (no silent skips).
DEJAVU_DIR=""
if [[ -n "${DEJAVU_FONT_DIR:-}" ]]; then
  DEJAVU_CANDS=("$DEJAVU_FONT_DIR")
else
  DEJAVU_CANDS=(/usr/share/fonts/TTF /usr/share/fonts/truetype/dejavu /usr/share/fonts/dejavu-sans-fonts)
fi
for cand in "${DEJAVU_CANDS[@]}"; do
  if [[ -d "$cand" ]] && compgen -G "$cand/DejaVuSans*.ttf" >/dev/null; then
    DEJAVU_DIR="$cand"
    break
  fi
done
if [[ -z "$DEJAVU_DIR" ]]; then
  echo "Missing DejaVuSans*.ttf fonts (install ttf-dejavu / fonts-dejavu-core or set DEJAVU_FONT_DIR)." >&2
  exit 3
fi

# Adwaita fonts are optional (present on GNOME 48+ distros; absent on Debian 12).
ADWAITA_DIR=""
if [[ -n "${ADWAITA_FONT_DIR:-}" ]]; then
  ADWAITA_CANDS=("$ADWAITA_FONT_DIR")
else
  ADWAITA_CANDS=(/usr/share/fonts/Adwaita /usr/share/fonts/adwaita)
fi
for cand in "${ADWAITA_CANDS[@]}"; do
  if [[ -d "$cand" ]] && compgen -G "$cand/*.ttf" >/dev/null; then
    ADWAITA_DIR="$cand"
    break
  fi
done

LIBERATION_DIR=""
if [[ -n "${LIBERATION_FONT_DIR:-}" ]]; then
  LIBERATION_CANDS=("$LIBERATION_FONT_DIR")
else
  LIBERATION_CANDS=(
    /usr/share/fonts/liberation
    /usr/share/fonts/TTF
    /usr/share/fonts/truetype/liberation
    /usr/share/fonts/truetype/liberation2
    /usr/share/fonts/liberation-serif
    "$LIBERATION_CACHE_ROOT/usr/share/fonts/liberation"
  )
fi
for cand in "${LIBERATION_CANDS[@]}"; do
  if [[ -f "$cand/LiberationSerif-Regular.ttf" \
     && -f "$cand/LiberationSerif-Bold.ttf" \
     && -f "$cand/LiberationSerif-Italic.ttf" \
     && -f "$cand/LiberationSerif-BoldItalic.ttf" ]]; then
    LIBERATION_DIR="$cand"
    break
  fi
done
if [[ -z "$LIBERATION_DIR" ]]; then
  echo "Missing Liberation Serif font faces (Regular/Bold/Italic/BoldItalic)." >&2
  echo "Install ttf-liberation / fonts-liberation or set LIBERATION_FONT_DIR." >&2
  exit 3
fi

if [[ -n "${LIBERATION_LICENSE:-}" ]]; then
  LIBERATION_LIC_CANDS=("$LIBERATION_LICENSE")
else
  LIBERATION_LIC_CANDS=(
    /usr/share/licenses/ttf-liberation/LICENSE
    /usr/share/doc/fonts-liberation/copyright
    /usr/share/doc/fonts-liberation2/copyright
    "$LIBERATION_CACHE_ROOT/usr/share/licenses/ttf-liberation/LICENSE"
  )
fi
RESOLVED_LIBERATION_LICENSE=$(find_first_file "${LIBERATION_LIC_CANDS[@]}" || true)
if [[ -z "$RESOLVED_LIBERATION_LICENSE" ]]; then
  echo "Missing Liberation Serif SIL OFL license file." >&2
  echo "Refusing to bundle Liberation Serif without its license. Set LIBERATION_LICENSE or install fonts-liberation." >&2
  exit 3
fi

RESOLVED_DEJAVU_LICENSE=$(find_first_file /usr/share/licenses/ttf-dejavu/LICENSE /usr/share/doc/fonts-dejavu-core/copyright || true)
RESOLVED_ADWAITA_LICENSE=$(find_first_file /usr/share/licenses/adwaita-fonts/LICENSE || true)

# Locate patchelf (host or cached).
PATCHELF_BIN=""
if command -v patchelf >/dev/null 2>&1; then
  PATCHELF_BIN=$(command -v patchelf)
elif [[ -x "$BUILD/tools/patchelf/bin/patchelf" ]]; then
  PATCHELF_BIN="$BUILD/tools/patchelf/bin/patchelf"
fi

HOST_GLIBC=$(ldd --version 2>&1 | awk 'NR==1{print $NF}' || echo "unknown")

RESOLVED_ALSA_PATH=$(find_first_file \
  /usr/lib/x86_64-linux-gnu/libasound.so.2 \
  /usr/lib64/libasound.so.2 \
  /usr/lib/libasound.so.2 || true)
if [[ -z "$RESOLVED_ALSA_PATH" ]]; then
  echo "Missing libasound.so.2 on the build host; install alsa-lib / libasound2 to stage portable audio support." >&2
  exit 3
fi

if [[ "$PREFLIGHT_ONLY" == "1" ]]; then
  WEBKIT_VERSION=$(pkg-config --modversion webkit2gtk-4.1 2>/dev/null || echo "unknown")
  GST_VERSION=$(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo "unknown")
  echo "AppImage packaging preflight OK:"
  echo "  version:             $VERSION"
  echo "  output:              $OUT"
  echo "  host glibc:          $HOST_GLIBC (MAX_GLIBC=${MAX_GLIBC:-unset})"
  echo "  webkit helpers:      $RESOLVED_WEBKIT_DIR (WebKitGTK $WEBKIT_VERSION)"
  echo "  gdk-pixbuf dir:      ${GDK_PIXBUF_DIR:-none (built-in / glycin loaders)}"
  echo "  gdk-pixbuf query:    ${GDK_PIXBUF_QUERY_LOADERS:-none}"
  echo "  gio dir:             $GIO_DIR (query: ${GIO_QUERYMODULES:-none})"
  echo "  gtk-3.0 dir:         $GTK3_DIR (immodules query: ${GTK_QUERY_IMMODULES:-none})"
  echo "  gstreamer host dir:  $GST_HOST_PLUGIN_DIR (GStreamer $GST_VERSION)"
  echo "  gst-plugin-scanner:  ${GST_SCANNER_BIN:-none}"
  echo "  gst autodetect:      ${RESOLVED_GST_PLUGINS[libgstautodetect.so]}"
  echo "  gst audiofx:         ${RESOLVED_GST_PLUGINS[libgstaudiofx.so]}"
  echo "  gst license:         ${RESOLVED_GST_LICENSE:-none}"
  echo "  dejavu fonts:        $DEJAVU_DIR"
  echo "  adwaita fonts:       ${ADWAITA_DIR:-optional (absent on host)}"
  echo "  liberation serif:    $LIBERATION_DIR"
  echo "  liberation license:  $RESOLVED_LIBERATION_LICENSE"
  echo "  appimagetool:        $TOOL ($( [[ -x "$TOOL" ]] && echo cached || echo 'will download' ))"
  echo "  apprun hooks:        $HOOK ($( [[ -s "$HOOK" ]] && echo cached || echo 'will download' ))"
  echo "  patchelf:            ${PATCHELF_BIN:-will download}"
  echo "  alsa runtime:        $RESOLVED_ALSA_PATH"
  exit 0
fi

# --- Tooling fetch & SHA-256 verification ---
mkdir -p "$BUILD/tools"
if [[ ! -x "$TOOL" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$TOOL.tmp" "$APPIMAGETOOL_URL"
  verify_sha256 "$TOOL.tmp" "$APPIMAGETOOL_SHA256" "appimagetool-x86_64.AppImage"
  mv "$TOOL.tmp" "$TOOL"
  chmod +x "$TOOL"
else
  verify_sha256 "$TOOL" "$APPIMAGETOOL_SHA256" "appimagetool-x86_64.AppImage"
fi
if [[ ! -s "$HOOK" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$HOOK.tmp" "$HOOK_URL"
  verify_sha256 "$HOOK.tmp" "$HOOK_SHA256" "libapprun_hooks-x86_64.so"
  mv "$HOOK.tmp" "$HOOK"
else
  verify_sha256 "$HOOK" "$HOOK_SHA256" "libapprun_hooks-x86_64.so"
fi
if [[ ! -s "$APPRUN_LICENSE" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$APPRUN_LICENSE.tmp" "$APPRUN_LICENSE_URL"
  verify_sha256 "$APPRUN_LICENSE.tmp" "$APPRUN_LICENSE_SHA256" "AppRun-LICENSE"
  mv "$APPRUN_LICENSE.tmp" "$APPRUN_LICENSE"
else
  verify_sha256 "$APPRUN_LICENSE" "$APPRUN_LICENSE_SHA256" "AppRun-LICENSE"
fi
if [[ -z "$PATCHELF_BIN" ]]; then
  PATCHER="$BUILD/tools/patchelf/bin/patchelf"
  if [[ ! -x "$PATCHER" ]]; then
    curl -fL --retry 2 --connect-timeout 15 -o "$BUILD/tools/patchelf.tar.gz" "$PATCHELF_URL"
    mkdir -p "$BUILD/tools/patchelf"
    tar -xzf "$BUILD/tools/patchelf.tar.gz" -C "$BUILD/tools/patchelf"
    verify_sha256 "$PATCHER" "$PATCHELF_BIN_SHA256" "patchelf"
  fi
  PATCHELF_BIN="$PATCHER"
fi
PATH="$(dirname "$PATCHELF_BIN"):$PATH"

# --- Build & stage AppDir ---
cd "$ROOT"
BINARY_PATH="${SEARCH_BINARY:-$ROOT/target/release/search}"
if [[ "$SKIP_CARGO_BUILD" != "1" ]]; then
  CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-1} cargo build --release --bin search
fi
[[ -x "$BINARY_PATH" ]] || { echo "Missing release binary at $BINARY_PATH" >&2; exit 3; }

rm -rf "$APPDIR" "$BUILD/appimagetool"
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT" "$OUT.sha256" "$OUT.abi.json"
mkdir -p "$APPDIR"/{usr/{bin,lib,share},etc/fonts}
install -Dm755 "$BINARY_PATH" "$APPDIR/usr/bin/search"
ln -sfn search "$APPDIR/usr/bin/rill"
cp packaging/Search.desktop "$APPDIR/Search.desktop"
cp packaging/search.png "$APPDIR/search.png"
cp packaging/search.png "$APPDIR/.DirIcon"
install -Dm644 packaging/search.png "$APPDIR/usr/share/icons/hicolor/512x512/apps/search.png"
install -Dm644 packaging/icon.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/search.svg"
install -Dm644 packaging/search.metainfo.xml "$APPDIR/usr/share/metainfo/io.github.indar_beurre.Search.metainfo.xml"
install -Dm644 LICENSE "$APPDIR/usr/share/licenses/rill/LICENSE"
install -Dm755 packaging/AppRun "$APPDIR/AppRun"
cp "$HOOK" "$APPDIR/usr/lib/libapprun_hooks.so"
cp "$APPRUN_LICENSE" "$APPDIR/usr/share/AppRun-LICENSE"
# Compile our posix_spawn/execve preload shim against the build host's glibc
# so WebKitGTK's compiled-in helper path redirects to $WEBKIT_EXEC_PATH across
# all distro filesystem layouts (Debian multiarch, Fedora libexec, Arch /usr/lib).
CC_BIN="${CC:-$(command -v cc || command -v gcc || true)}"
[[ -n "$CC_BIN" ]] || { echo "Missing C compiler (cc/gcc) required to build librill_appimage_preload.so." >&2; exit 3; }
"$CC_BIN" -shared -fPIC -O2 -Wall -Wextra -Werror \
  "$ROOT/packaging/rill_appimage_preload.c" \
  -o "$APPDIR/usr/lib/librill_appimage_preload.so" -ldl
mkdir -p "$APPDIR/usr/share/search"
cp -a assets/reader.js "$APPDIR/usr/share/search/reader.js"
cp -a assets/shield "$APPDIR/usr/share/search/shield"

# Normalize WebKitGTK helper directory name inside AppDir to match packaging/AppRun.
cp -a "$RESOLVED_WEBKIT_DIR" "$APPDIR/usr/lib/webkit2gtk-4.1"
if [[ -n "$GDK_PIXBUF_DIR" ]]; then
  cp -a "$GDK_PIXBUF_DIR" "$APPDIR/usr/lib/gdk-pixbuf-2.0"
else
  mkdir -p "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders"
fi
cp -a "$GIO_DIR" "$APPDIR/usr/lib/gio"
cp -a "$GTK3_DIR" "$APPDIR/usr/lib/gtk-3.0"
cp -a "$GST_HOST_PLUGIN_DIR" "$APPDIR/usr/lib/gstreamer-1.0"
if [[ -n "$GST_SCANNER_BIN" && ! -x "$APPDIR/usr/lib/gstreamer-1.0/gst-plugin-scanner" ]]; then
  install -Dm755 "$GST_SCANNER_BIN" "$APPDIR/usr/lib/gstreamer-1.0/gst-plugin-scanner"
fi

for plugin in libgstautodetect.so libgstaudiofx.so; do
  if [[ ! -f "$APPDIR/usr/lib/gstreamer-1.0/$plugin" ]]; then
    cp -L "${RESOLVED_GST_PLUGINS[$plugin]}" "$APPDIR/usr/lib/gstreamer-1.0/$plugin"
  fi
done
if [[ -n "$RESOLVED_GST_LICENSE" ]]; then
  install -Dm644 "$RESOLVED_GST_LICENSE" "$APPDIR/usr/share/licenses/search/gst-plugins-good-LGPL-2.1-or-later.txt"
fi

cp -a /usr/share/glib-2.0 "$APPDIR/usr/share/"
if [[ -n "$GTK3_SHARE_DIR" && -d /usr/share/gtk-3.0 ]]; then
  cp -a /usr/share/gtk-3.0 "$APPDIR/usr/share/"
fi
cp -a /usr/share/mime "$APPDIR/usr/share/"
cp -a /usr/share/icons/Adwaita "$APPDIR/usr/share/icons/"
cp -a /usr/share/X11 "$APPDIR/usr/share/"

mkdir -p "$APPDIR/usr/share/fonts"
cp -a "$DEJAVU_DIR"/DejaVuSans*.ttf "$APPDIR/usr/share/fonts/"
if [[ -n "$ADWAITA_DIR" ]]; then
  cp -a "$ADWAITA_DIR" "$APPDIR/usr/share/fonts/Adwaita"
fi
cp -a "$LIBERATION_DIR"/LiberationSerif-*.ttf "$APPDIR/usr/share/fonts/"
install -Dm644 "$RESOLVED_LIBERATION_LICENSE" "$APPDIR/usr/share/licenses/search/LiberationSerif-LICENSE"
if [[ -n "$RESOLVED_DEJAVU_LICENSE" ]]; then
  install -Dm644 "$RESOLVED_DEJAVU_LICENSE" "$APPDIR/usr/share/licenses/search/DejaVu-LICENSE"
fi
if [[ -n "$RESOLVED_ADWAITA_LICENSE" ]]; then
  install -Dm644 "$RESOLVED_ADWAITA_LICENSE" "$APPDIR/usr/share/licenses/search/AdwaitaFonts-LICENSE"
fi

# GTK/WebKit use the standard locale directory if present. It is optional.
for catalog in gtk30 glib20; do
  find /usr/share/locale -path "*/LC_MESSAGES/$catalog.mo" -print0 2>/dev/null | while IFS= read -r -d '' source; do
    dest="$APPDIR/usr/share/locale/${source#/usr/share/locale/}"
    mkdir -p "$(dirname "$dest")"
    cp "$source" "$dest"
  done
done

# Host-ABI excludelist (standard AppImage portability contract).
# NEVER bundle glibc, ld-linux, libgcc_s, libstdc++, host graphics/Mesa/DRI,
# or X11/Wayland core libs. Note: libsystemd.so.0 / libselinux.so.1 are
# intentionally NOT excluded when built on Debian 12 (glibc 2.36) because
# WebKitGTK links them directly while non-systemd/non-SELinux distros (Artix,
# Devuan, Arch, Void) do not ship libsystemd.so.0 or libselinux.so.1 on host.
is_excluded_lib() {
  local base="$1"
  case "$base" in
    ld-linux*.so*|\
    libc.so.*|libm.so.*|libmvec.so.*|libdl.so.*|libpthread.so.*|librt.so.*|\
    libresolv.so.*|libutil.so.*|libanl.so.*|libnsl.so.*|libBrokenLocale.so.*|\
    libthread_db.so.*|libnss_*.so*|\
    libgcc_s.so.*|libstdc++.so.*|\
    libdrm.so.*|libdrm_*.so*|\
    libGL.so.*|libGLX.so.*|libGLdispatch.so.*|libEGL.so.*|libGLESv2.so.*|libGLESv1_CM.so.*|libOpenGL.so.*|\
    libgbm.so.*|libvulkan.so.*|\
    libwayland-client.so.*|libwayland-cursor.so.*|libwayland-egl.so.*|libwayland-server.so.*|\
    libX11.so.*|libX11-xcb.so.*|libxcb.so.*|libxcb-dri2.so.*|libxcb-dri3.so.*|\
    libxcb-glx.so.*|libxcb-present.so.*|libxcb-sync.so.*|libxshmfence.so.*|\
    libICE.so.*|libSM.so.*|\
    libdbus-1.so.*|libelogind.so.*|libudev.so.*|\
    libpipewire-*.so*|libjack.so.*|\
    libz.so.*|libexpat.so.*|libuuid.so.*|libgpg-error.so.*|libcom_err.so.*|\
    libfontconfig.so.*|libfreetype.so.*|libharfbuzz.so.*|libfribidi.so.*|libthai.so.*|\
    libp11-kit.so.*|libgmp.so.*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

# Resolve non-host-ABI dependencies of the executable, WebKit helpers, and plugins.
declare -A seen=()
collect() {
  local binary="$1" dep base real
  while read -r dep; do
    [[ "$dep" == /* ]] || continue
    base=$(basename "$dep")
    if is_excluded_lib "$base"; then
      continue
    fi
    real=$(readlink -f "$dep")
    [[ -f "$real" ]] || continue
    if is_excluded_lib "$(basename "$real")"; then
      continue
    fi
    [[ -n "${seen[$real]:-}" ]] && continue
    seen[$real]=1
    cp -L "$dep" "$APPDIR/usr/lib/$base"
    collect "$real"
  done < <(ldd "$binary" 2>/dev/null | awk '/=> \// {print $3} /^\// {print $1}')
}
while IFS= read -r -d '' binary; do collect "$binary"; done < <(
  find "$APPDIR/usr/bin" "$APPDIR/usr/lib/webkit2gtk-4.1" "$APPDIR/usr/lib/gdk-pixbuf-2.0" "$APPDIR/usr/lib/gio" "$APPDIR/usr/lib/gtk-3.0" "$APPDIR/usr/lib/gstreamer-1.0" -type f -exec file --mime-type {} + | awk -F: '$2 ~ /application\/x-(sharedlib|executable|pie-executable)/ {print $1}' | tr '\n' '\0'
)

# Keep sonames and the WebKit process lookup independent of /usr.
find "$APPDIR/usr/bin" "$APPDIR/usr/lib" -type f -exec file --mime-type {} + | awk -F: '$2 ~ /application\/x-(sharedlib|executable|pie-executable)/ {print $1}' | while IFS= read -r binary; do
  patchelf --set-rpath '$ORIGIN/../lib:$ORIGIN' "$binary" 2>/dev/null || true
done
patchelf --set-rpath '$ORIGIN/../lib' "$APPDIR/usr/bin/search"

# All module caches must contain /usr paths (redirected by libapprun_hooks),
# never absolute build-machine staging paths.
mkdir -p "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders"
if [[ -n "$GDK_PIXBUF_QUERY_LOADERS" ]]; then
  env GDK_PIXBUF_MODULEDIR="$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders" \
    "$GDK_PIXBUF_QUERY_LOADERS" | sed "s|$APPDIR||g" > "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
fi
mkdir -p "$APPDIR/usr/lib/gtk-3.0/3.0.0"
if [[ -n "$GTK_QUERY_IMMODULES" ]]; then
  env GTK_PATH="$APPDIR/usr/lib/gtk-3.0" "$GTK_QUERY_IMMODULES" | sed "s|$APPDIR||g" \
    > "$APPDIR/usr/lib/gtk-3.0/3.0.0/immodules.cache"
fi
if [[ -n "$GIO_QUERYMODULES" && -d "$APPDIR/usr/lib/gio/modules" ]]; then
  env GIO_MODULE_DIR="$APPDIR/usr/lib/gio/modules" "$GIO_QUERYMODULES" "$APPDIR/usr/lib/gio/modules"
fi
glib-compile-schemas "$APPDIR/usr/share/glib-2.0/schemas"
fc-cache -f "$APPDIR/usr/share/fonts" >/dev/null
cat > "$APPDIR/etc/fonts/fonts.conf" <<'EOF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>/usr/share/fonts</dir>
  <cachedir>/var/cache/fontconfig</cachedir>
  <cachedir prefix="xdg">fontconfig</cachedir>
  <include ignore_missing="yes">/etc/fonts/conf.d</include>
</fontconfig>
EOF

# Remove development/documentation files that cannot affect the runtime.
rm -rf "$APPDIR/usr/share/mime/packages" "$APPDIR/usr/share/doc" "$APPDIR/usr/share/man"

# --- Post-stage ABI & VM-001 regression gate ---
FORBIDDEN_FOUND=()
while IFS= read -r -d '' libfile; do
  base=$(basename "$libfile")
  if is_excluded_lib "$base"; then
    FORBIDDEN_FOUND+=("$base")
  fi
done < <(find "$APPDIR/usr/lib" -maxdepth 1 -type f -name '*.so*' -print0)

if (( ${#FORBIDDEN_FOUND[@]} > 0 )); then
  echo "FATAL (VM-001 guard): Forbidden host-ABI libraries staged in AppDir/usr/lib:" >&2
  printf '  %s\n' "${FORBIDDEN_FOUND[@]}" >&2
  exit 6
fi

if [[ -d "$APPDIR/usr/lib/dri" ]]; then
  echo "FATAL (VM-001 guard): Host Mesa DRI directory must not be bundled in AppDir/usr/lib/dri." >&2
  exit 6
fi

ver_le() {
  [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" == "$2" ]]
}

BINARY_MAX_GLIBC=$(readelf --version-info "$APPDIR/usr/bin/search" 2>/dev/null \
  | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' | sed 's/^GLIBC_//' | sort -Vu | tail -n1 || echo "0.0")
APPDIR_MAX_GLIBC="$BINARY_MAX_GLIBC"
APPDIR_MAX_GLIBC_FILE="usr/bin/search"
BUNDLED_LIB_COUNT=0

while IFS= read -r -d '' elffile; do
  ((BUNDLED_LIB_COUNT += 1)) || true
  maxv=$(readelf --version-info "$elffile" 2>/dev/null \
    | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' | sed 's/^GLIBC_//' | sort -Vu | tail -n1 || true)
  if [[ -n "$maxv" ]] && ! ver_le "$maxv" "$APPDIR_MAX_GLIBC"; then
    APPDIR_MAX_GLIBC="$maxv"
    APPDIR_MAX_GLIBC_FILE="${elffile#$APPDIR/}"
  fi
done < <(find "$APPDIR/usr/lib" -type f -name '*.so*' -print0)

echo "AppDir ABI audit:"
echo "  host glibc:            $HOST_GLIBC"
echo "  binary max GLIBC:      $BINARY_MAX_GLIBC"
echo "  AppDir max GLIBC:      $APPDIR_MAX_GLIBC ($APPDIR_MAX_GLIBC_FILE)"
echo "  bundled .so count:     $BUNDLED_LIB_COUNT"
echo "  forbidden host libs:   0"

if [[ -n "${MAX_GLIBC:-}" ]]; then
  if ! ver_le "$APPDIR_MAX_GLIBC" "$MAX_GLIBC"; then
    echo "FATAL (ABI floor): AppDir requires GLIBC_$APPDIR_MAX_GLIBC ($APPDIR_MAX_GLIBC_FILE), exceeding MAX_GLIBC=$MAX_GLIBC." >&2
    echo "Build on an older supported distribution (e.g. Debian 12 / glibc 2.36)." >&2
    exit 7
  fi
elif ! ver_le "$APPDIR_MAX_GLIBC" "2.36"; then
  echo "WARNING: AppDir requires GLIBC_$APPDIR_MAX_GLIBC (> 2.36); this artifact is NOT portable to older distros like Debian 12." >&2
fi

# AppImage's own launcher uses the packaged runtime and mksquashfs.
rm -rf "$BUILD/appimagetool"
(cd "$BUILD" && "$TOOL" --appimage-extract >/dev/null && mv squashfs-root appimagetool)
ARCH=x86_64 "$BUILD/appimagetool/AppRun" --no-appstream "$APPDIR" "$OUT"
chmod +x "$OUT"
(cd "$(dirname "$OUT")" && sha256sum "$(basename "$OUT")") > "$OUT.sha256"

OUT_SHA256=$(awk '{print $1}' "$OUT.sha256")
BUILD_OS=$(awk -F= '/^PRETTY_NAME=/{gsub(/"/,"",$2); print $2}' /etc/os-release 2>/dev/null || uname -sr)
WEBKIT_VERSION=$(pkg-config --modversion webkit2gtk-4.1 2>/dev/null || echo "unknown")
GST_VERSION=$(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo "unknown")

cat > "$OUT.abi.json" <<EOF
{
  "format": "appimage",
  "artifact": "$(basename "$OUT")",
  "version": "$VERSION",
  "arch": "x86_64",
  "sha256": "$OUT_SHA256",
  "build_os": "$BUILD_OS",
  "host_glibc": "$HOST_GLIBC",
  "binary_max_glibc": "$BINARY_MAX_GLIBC",
  "max_glibc": "$APPDIR_MAX_GLIBC",
  "max_glibc_culprit": "$APPDIR_MAX_GLIBC_FILE",
  "max_glibc_limit": "${MAX_GLIBC:-none}",
  "bundled_so_count": $BUNDLED_LIB_COUNT,
  "forbidden_host_libs": [],
  "webkit2gtk_version": "$WEBKIT_VERSION",
  "gstreamer_version": "$GST_VERSION"
}
EOF

echo "Built $OUT (SHA-256: $OUT_SHA256, max GLIBC: $APPDIR_MAX_GLIBC)"
