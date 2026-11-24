#!/usr/bin/env bash
# Package the actual release binary and its WebKit helpers, not a wrapper around
# a system browser. The host needs a graphical session only for the smoke test.
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BUILD="$ROOT/build"
APPDIR="$BUILD/AppDir"
VERSION=$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed -n 's/^version *= *"\([^"]*\)".*/\1/p' || true)
VERSION=${VERSION:-1.0.1}
OUT="${APPIMAGE_OUT:-$BUILD/Search-${VERSION}-x86_64.AppImage}"
TOOL="${APPIMAGETOOL:-$BUILD/tools/appimagetool-x86_64.AppImage}"
HOOK="$BUILD/tools/libapprun_hooks-x86_64.so"
[[ $(uname -m) == x86_64 ]] || { echo 'This script builds x86_64 only.' >&2; exit 2; }
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
if [[ ! -s "$BUILD/tools/AppRun-LICENSE" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$BUILD/tools/AppRun-LICENSE" \
    https://raw.githubusercontent.com/AppImageCrafters/AppRun/master/LICENSE
fi
if ! command -v patchelf >/dev/null; then
  PATCHER="$BUILD/tools/patchelf/bin/patchelf"
  if [[ ! -x "$PATCHER" ]]; then
    curl -fL --retry 2 --connect-timeout 15 -o "$BUILD/tools/patchelf.tar.gz" \
      https://github.com/NixOS/patchelf/releases/download/0.19.3/patchelf-0.19.3-x86_64.tar.gz
    mkdir -p "$BUILD/tools/patchelf"
    tar -xzf "$BUILD/tools/patchelf.tar.gz" -C "$BUILD/tools/patchelf"
  fi
  PATH="$(dirname "$PATCHER"):$PATH"
fi
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
cp "$BUILD/tools/AppRun-LICENSE" "$APPDIR/usr/share/AppRun-LICENSE"
mkdir -p "$APPDIR/usr/share/search"
cp -a assets/reader.js "$APPDIR/usr/share/search/reader.js"
cp -a assets/shield "$APPDIR/usr/share/search/shield"
cp -a /usr/lib/webkit2gtk-4.1 "$APPDIR/usr/lib/"
cp -a /usr/lib/gdk-pixbuf-2.0 "$APPDIR/usr/lib/"
cp -a /usr/lib/gio "$APPDIR/usr/lib/"
cp -a /usr/lib/gtk-3.0 "$APPDIR/usr/lib/"
cp -a /usr/lib/gstreamer-1.0 "$APPDIR/usr/lib/"
# WebKit's page process aborts when a page touches audio and GStreamer has no
# autoaudiosink (gst-plugins-good's autodetect plugin); GitHub crashed this way
# about 1 s into loading. scaletempo (audiofx) is needed for playback-rate
# changes. Both must ship in the AppImage, because the host may not have them.
# Take them from the host if it has them, otherwise from GST_EXTRA_PLUGIN_DIR
# (a gst-plugins-good build matching the host GStreamer), and refuse to build
# without them.
GST_EXTRA_PLUGIN_DIR="${GST_EXTRA_PLUGIN_DIR:-$HOME/.cache/search-gst-autodetect/root/usr/lib/gstreamer-1.0}"
for plugin in libgstautodetect.so libgstaudiofx.so; do
  if [[ -f "$APPDIR/usr/lib/gstreamer-1.0/$plugin" ]]; then
    continue
  elif [[ -f "$GST_EXTRA_PLUGIN_DIR/$plugin" ]]; then
    cp -L "$GST_EXTRA_PLUGIN_DIR/$plugin" "$APPDIR/usr/lib/gstreamer-1.0/$plugin"
  else
    echo "Missing GStreamer plugin $plugin: install gst-plugins-good or set GST_EXTRA_PLUGIN_DIR." >&2
    exit 3
  fi
done
cp -a /usr/share/glib-2.0 "$APPDIR/usr/share/"
cp -a /usr/share/gtk-3.0 "$APPDIR/usr/share/"
cp -a /usr/share/mime "$APPDIR/usr/share/"
cp -a /usr/share/icons/Adwaita "$APPDIR/usr/share/icons/"
cp -a /usr/share/X11 "$APPDIR/usr/share/"
mkdir -p "$APPDIR/usr/share/fonts"
cp -a /usr/share/fonts/TTF/DejaVuSans*.ttf "$APPDIR/usr/share/fonts/"
cp -a /usr/share/fonts/Adwaita "$APPDIR/usr/share/fonts/"
cp -a /usr/share/fonts/liberation/LiberationSerif-*.ttf "$APPDIR/usr/share/fonts/"
if [[ -f /usr/share/licenses/ttf-liberation/LICENSE ]]; then
  install -Dm644 /usr/share/licenses/ttf-liberation/LICENSE "$APPDIR/usr/share/licenses/search/LiberationSerif-LICENSE"
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
# Resolve dependencies of the executable, WebKit helpers, and optional plugins.
# libc remains host-provided: mixing a distro's libc with host graphics drivers
# is less portable than requiring glibc 2.27+.
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
