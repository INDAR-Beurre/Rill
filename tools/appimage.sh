#!/usr/bin/env bash
# Package the actual release binary and its WebKit helpers, not a wrapper around
# a system browser. The host needs a graphical session only for the smoke test.
#
# Usage:
#   tools/appimage.sh               # build build/Search-redesign-x86_64.AppImage
#   tools/appimage.sh --preflight   # validate host & cached dependencies only
set -euo pipefail

PREFLIGHT_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --preflight) PREFLIGHT_ONLY=1 ;;
    -h|--help)
      echo "Usage: $0 [--preflight]"
      echo "  APPIMAGE_OUT          output AppImage path (default: build/Search-redesign-x86_64.AppImage)"
      echo "  WEBKIT_HELPER_DIR     override WebKitGTK 4.1 helper directory"
      echo "  GST_EXTRA_PLUGIN_DIR  fallback dir for libgstautodetect.so and libgstaudiofx.so"
      echo "  GST_EXTRA_LICENSE     optional LGPL license path for bundled GStreamer plugins"
      echo "  LIBERATION_FONT_DIR   override dir containing LiberationSerif-*.ttf"
      echo "  LIBERATION_LICENSE    override path to Liberation Serif SIL OFL license"
      echo "  DEJAVU_FONT_DIR       override dir containing DejaVuSans*.ttf"
      echo "  ADWAITA_FONT_DIR      override dir containing Adwaita fonts"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg (expected --preflight or --help)" >&2
      exit 2
      ;;
  esac
done

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BUILD="$ROOT/build"
APPDIR="$BUILD/AppDir"
OUT="${APPIMAGE_OUT:-$BUILD/Search-redesign-x86_64.AppImage}"
TOOL="${APPIMAGETOOL:-$BUILD/tools/appimagetool-x86_64.AppImage}"
HOOK="$BUILD/tools/libapprun_hooks-x86_64.so"
APPRUN_LICENSE="$BUILD/tools/AppRun-LICENSE"
GST_EXTRA_PLUGIN_DIR="${GST_EXTRA_PLUGIN_DIR:-$HOME/.cache/search-gst-autodetect/root/usr/lib/gstreamer-1.0}"
GST_EXTRA_LICENSE_DEFAULT="$HOME/.cache/search-gst-autodetect/root/usr/share/licenses/gst-plugins-good/LICENSE"
LIBERATION_CACHE_ROOT="$HOME/.cache/search-packaging/ttf-liberation"

[[ $(uname -m) == x86_64 ]] || { echo 'This script builds x86_64 only.' >&2; exit 2; }

# Never silently overwrite historic 1.0.1 / 1.0.2 release artifacts.
case "$(basename "$OUT")" in
  Search-1.0.1-*.AppImage|Search-1.0.2-*.AppImage)
    if [[ -e "$OUT" && "${ALLOW_OVERWRITE_HISTORIC:-0}" != "1" ]]; then
      echo "Refusing to overwrite historic artifact $OUT." >&2
      echo "Set APPIMAGE_OUT to a new path (default: $BUILD/Search-redesign-x86_64.AppImage) or ALLOW_OVERWRITE_HISTORIC=1." >&2
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

for cmd in pkg-config file ldd sha256sum gdk-pixbuf-query-loaders gtk-query-immodules-3.0 gio-querymodules glib-compile-schemas fc-cache; do
  require_cmd "$cmd"
done

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
# /usr/lib/gdk-pixbuf-2.0 module directory; treat it as optional when absent.
GDK_PIXBUF_DIR=$(find_first_dir "${GDK_PIXBUF_LIBDIR:+$GDK_PIXBUF_LIBDIR/gdk-pixbuf-2.0}" /usr/lib/gdk-pixbuf-2.0 /usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0 || true)

for share_dir in /usr/share/glib-2.0 /usr/share/gtk-3.0 /usr/share/mime /usr/share/icons/Adwaita /usr/share/X11; do
  if [[ ! -d "$share_dir" ]]; then
    echo "Missing required host data directory: $share_dir" >&2
    exit 3
  fi
done
GIO_DIR=$(find_first_dir "${GIO_LIBDIR:+$GIO_LIBDIR/gio}" /usr/lib/gio /usr/lib/x86_64-linux-gnu/gio) || {
  echo "Missing gio module directory." >&2; exit 3;
}
GTK3_DIR=$(find_first_dir "${GTK3_LIBDIR:+$GTK3_LIBDIR/gtk-3.0}" /usr/lib/gtk-3.0 /usr/lib/x86_64-linux-gnu/gtk-3.0) || {
  echo "Missing gtk-3.0 module directory." >&2; exit 3;
}
GST_HOST_PLUGIN_DIR=$(find_first_dir "$GST_PC_PLUGINSDIR" /usr/lib/gstreamer-1.0 /usr/lib/x86_64-linux-gnu/gstreamer-1.0) || {
  echo "Missing host gstreamer-1.0 plugin directory." >&2; exit 3;
}

# 3. Validate required GStreamer audio plugins (autodetect + audiofx).
# WebKit's page process aborts when a page touches audio and GStreamer has no
# autoaudiosink (gst-plugins-good's autodetect plugin); GitHub crashed this way
# about 1 s into loading. scaletempo (audiofx) is needed for playback-rate
# changes. Both must ship in the AppImage, because the host may not have them.
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
  echo "Missing DejaVuSans*.ttf fonts (install ttf-dejavu or set DEJAVU_FONT_DIR)." >&2
  exit 3
fi

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
if [[ -z "$ADWAITA_DIR" ]]; then
  echo "Missing Adwaita fonts (install adwaita-fonts or set ADWAITA_FONT_DIR)." >&2
  exit 3
fi

LIBERATION_DIR=""
if [[ -n "${LIBERATION_FONT_DIR:-}" ]]; then
  LIBERATION_CANDS=("$LIBERATION_FONT_DIR")
else
  LIBERATION_CANDS=(
    /usr/share/fonts/liberation
    /usr/share/fonts/TTF
    /usr/share/fonts/truetype/liberation
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
  echo "Install ttf-liberation or provision $LIBERATION_CACHE_ROOT/usr/share/fonts/liberation (or set LIBERATION_FONT_DIR)." >&2
  exit 3
fi

if [[ -n "${LIBERATION_LICENSE:-}" ]]; then
  LIBERATION_LIC_CANDS=("$LIBERATION_LICENSE")
else
  LIBERATION_LIC_CANDS=(
    /usr/share/licenses/ttf-liberation/LICENSE
    /usr/share/doc/fonts-liberation/copyright
    "$LIBERATION_CACHE_ROOT/usr/share/licenses/ttf-liberation/LICENSE"
  )
fi
RESOLVED_LIBERATION_LICENSE=$(find_first_file "${LIBERATION_LIC_CANDS[@]}" || true)
if [[ -z "$RESOLVED_LIBERATION_LICENSE" ]]; then
  echo "Missing Liberation Serif SIL OFL license file." >&2
  echo "Refusing to bundle Liberation Serif without its license. Set LIBERATION_LICENSE or provision $LIBERATION_CACHE_ROOT/usr/share/licenses/ttf-liberation/LICENSE." >&2
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

if [[ "$PREFLIGHT_ONLY" == "1" ]]; then
  WEBKIT_VERSION=$(pkg-config --modversion webkit2gtk-4.1 2>/dev/null || echo "unknown")
  GST_VERSION=$(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo "unknown")
  echo "AppImage packaging preflight OK:"
  echo "  output:              $OUT"
  echo "  webkit helpers:      $RESOLVED_WEBKIT_DIR (WebKitGTK $WEBKIT_VERSION)"
  echo "  gdk-pixbuf dir:      ${GDK_PIXBUF_DIR:-none (built-in / glycin loaders)}"
  echo "  gio dir:             $GIO_DIR"
  echo "  gtk-3.0 dir:         $GTK3_DIR"
  echo "  gstreamer host dir:  $GST_HOST_PLUGIN_DIR (GStreamer $GST_VERSION)"
  echo "  gst autodetect:      ${RESOLVED_GST_PLUGINS[libgstautodetect.so]}"
  echo "  gst audiofx:         ${RESOLVED_GST_PLUGINS[libgstaudiofx.so]}"
  echo "  gst license:         ${RESOLVED_GST_LICENSE:-none}"
  echo "  dejavu fonts:        $DEJAVU_DIR"
  echo "  adwaita fonts:       $ADWAITA_DIR"
  echo "  liberation serif:    $LIBERATION_DIR"
  echo "  liberation license:  $RESOLVED_LIBERATION_LICENSE"
  echo "  appimagetool:        $TOOL ($( [[ -x "$TOOL" ]] && echo cached || echo 'will download' ))"
  echo "  apprun hooks:        $HOOK ($( [[ -s "$HOOK" ]] && echo cached || echo 'will download' ))"
  echo "  patchelf:            ${PATCHELF_BIN:-will download}"
  exit 0
fi

# --- Tooling fetch (only when not cached) ---
mkdir -p "$BUILD/tools"
if [[ ! -x "$TOOL" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$TOOL" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$TOOL"
fi
if [[ ! -s "$HOOK" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$HOOK" \
    https://github.com/AppImageCrafters/AppRun/releases/download/v2.0.0/libapprun_hooks-Release-x86_64.so
fi
if [[ ! -s "$APPRUN_LICENSE" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$APPRUN_LICENSE" \
    https://raw.githubusercontent.com/AppImageCrafters/AppRun/master/LICENSE
fi
if ! command -v patchelf >/dev/null 2>&1; then
  PATCHER="$BUILD/tools/patchelf/bin/patchelf"
  if [[ ! -x "$PATCHER" ]]; then
    curl -fL --retry 2 --connect-timeout 15 -o "$BUILD/tools/patchelf.tar.gz" \
      https://github.com/NixOS/patchelf/releases/download/0.19.3/patchelf-0.19.3-x86_64.tar.gz
    mkdir -p "$BUILD/tools/patchelf"
    tar -xzf "$BUILD/tools/patchelf.tar.gz" -C "$BUILD/tools/patchelf"
  fi
  PATH="$(dirname "$PATCHER"):$PATH"
fi

# --- Build & stage AppDir ---
cd "$ROOT"
CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-1} cargo build --release
rm -rf "$APPDIR" "$BUILD/appimagetool"
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT" "$OUT.sha256"
mkdir -p "$APPDIR"/{usr/{bin,lib,share},etc/fonts}
cp target/release/search "$APPDIR/usr/bin/search"
cp packaging/Search.desktop "$APPDIR/Search.desktop"
cp packaging/search.png "$APPDIR/search.png"
cp packaging/search.png "$APPDIR/.DirIcon"
install -Dm644 packaging/search.png "$APPDIR/usr/share/icons/hicolor/512x512/apps/search.png"
install -Dm644 packaging/search.metainfo.xml "$APPDIR/usr/share/metainfo/search.metainfo.xml"
install -Dm755 packaging/AppRun "$APPDIR/AppRun"
cp "$HOOK" "$APPDIR/usr/lib/libapprun_hooks.so"
cp "$APPRUN_LICENSE" "$APPDIR/usr/share/AppRun-LICENSE"
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

for plugin in libgstautodetect.so libgstaudiofx.so; do
  if [[ ! -f "$APPDIR/usr/lib/gstreamer-1.0/$plugin" ]]; then
    cp -L "${RESOLVED_GST_PLUGINS[$plugin]}" "$APPDIR/usr/lib/gstreamer-1.0/$plugin"
  fi
done
if [[ -n "$RESOLVED_GST_LICENSE" ]]; then
  install -Dm644 "$RESOLVED_GST_LICENSE" "$APPDIR/usr/share/licenses/search/gst-plugins-good-LGPL-2.1-or-later.txt"
fi

cp -a /usr/share/glib-2.0 "$APPDIR/usr/share/"
cp -a /usr/share/gtk-3.0 "$APPDIR/usr/share/"
cp -a /usr/share/mime "$APPDIR/usr/share/"
cp -a /usr/share/icons/Adwaita "$APPDIR/usr/share/icons/"
cp -a /usr/share/X11 "$APPDIR/usr/share/"

mkdir -p "$APPDIR/usr/share/fonts"
cp -a "$DEJAVU_DIR"/DejaVuSans*.ttf "$APPDIR/usr/share/fonts/"
cp -a "$ADWAITA_DIR" "$APPDIR/usr/share/fonts/Adwaita"
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

# Mesa's driver dispatch needs these at their conventional DRI location.
if [[ -d /usr/lib/dri ]]; then
  mkdir -p "$APPDIR/usr/lib/dri"
  cp -a /usr/lib/dri/*.so "$APPDIR/usr/lib/dri/"
fi

# Resolve dependencies of the executable, WebKit helpers, and plugins.
# libc remains host-provided: mixing a distro's libc with host graphics drivers
# is less portable than matching the build host's glibc floor.
declare -A seen=()
collect() {
  local binary="$1" dep real
  while read -r dep; do
    [[ "$dep" == /* ]] || continue
    real=$(readlink -f "$dep")
    [[ -f "$real" ]] || continue
    [[ -n "${seen[$real]:-}" ]] && continue
    seen[$real]=1
    cp -L "$dep" "$APPDIR/usr/lib/$(basename "$dep")"
    collect "$real"
  done < <(ldd "$binary" 2>/dev/null | awk '/=> \// {print $3} /^\// {print $1}')
}
while IFS= read -r -d '' binary; do collect "$binary"; done < <(
  find "$APPDIR/usr/bin" "$APPDIR/usr/lib/webkit2gtk-4.1" "$APPDIR/usr/lib/gdk-pixbuf-2.0" "$APPDIR/usr/lib/gio" "$APPDIR/usr/lib/gtk-3.0" "$APPDIR/usr/lib/gstreamer-1.0" -type f -exec file --mime-type {} + | awk -F: '$2 ~ /application\/x-(sharedlib|executable)/ {print $1}' | tr '\n' '\0'
)
# The DRI dependencies are collected separately because they are dlopened.
if [[ -d "$APPDIR/usr/lib/dri" ]]; then
  while IFS= read -r -d '' binary; do collect "$binary"; done < <(find "$APPDIR/usr/lib/dri" -type f -name '*.so' -print0)
fi

# Keep sonames and the WebKit process lookup independent of /usr.
find "$APPDIR/usr/bin" "$APPDIR/usr/lib" -type f -exec file --mime-type {} + | awk -F: '$2 ~ /application\/x-(sharedlib|executable)/ {print $1}' | while IFS= read -r binary; do
  patchelf --set-rpath '$ORIGIN/../lib:$ORIGIN' "$binary" 2>/dev/null || true
done
patchelf --set-rpath '$ORIGIN/../lib' "$APPDIR/usr/bin/search"

# All module caches must contain relative-looking AppDir paths, never /usr.
env GDK_PIXBUF_MODULEDIR="$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders" \
  gdk-pixbuf-query-loaders > "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
env GTK_PATH="$APPDIR/usr/lib/gtk-3.0" gtk-query-immodules-3.0 > "$APPDIR/usr/lib/gtk-3.0/3.0.0/immodules.cache"
env GIO_MODULE_DIR="$APPDIR/usr/lib/gio/modules" gio-querymodules "$APPDIR/usr/lib/gio/modules"
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

# AppImage's own launcher uses the packaged runtime and mksquashfs.
rm -rf "$BUILD/appimagetool"
(cd "$BUILD" && "$TOOL" --appimage-extract >/dev/null && mv squashfs-root appimagetool)
ARCH=x86_64 "$BUILD/appimagetool/AppRun" --no-appstream "$APPDIR" "$OUT"
chmod +x "$OUT"
(cd "$(dirname "$OUT")" && sha256sum "$(basename "$OUT")") > "$OUT.sha256"
echo "Built $OUT"
