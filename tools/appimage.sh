#!/usr/bin/env bash
# Package the actual release binary and its WebKit helpers, not a wrapper around
# a system browser. The host needs a graphical session only for the smoke test.
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BUILD="$ROOT/build"
APPDIR="$BUILD/AppDir"
OUT="$BUILD/Search-1.0.0-x86_64.AppImage"
TOOL="${APPIMAGETOOL:-$BUILD/tools/appimagetool-x86_64.AppImage}"
[[ $(uname -m) == x86_64 ]] || { echo 'This script builds x86_64 only.' >&2; exit 2; }
mkdir -p "$BUILD/tools"
if [[ ! -x "$TOOL" ]]; then
  curl -fL --retry 2 --connect-timeout 15 -o "$TOOL" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$TOOL"
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
rm -rf "$APPDIR" "$OUT" "$BUILD/appimagetool"
mkdir -p "$APPDIR"/{usr/{bin,lib,share},etc/fonts}
cp target/release/search "$APPDIR/usr/bin/search"
cp packaging/Search.desktop "$APPDIR/Search.desktop"
cp packaging/search.png "$APPDIR/search.png"
cp packaging/search.png "$APPDIR/.DirIcon"
install -Dm644 packaging/search.png "$APPDIR/usr/share/icons/hicolor/512x512/apps/search.png"
install -Dm644 packaging/search.metainfo.xml "$APPDIR/usr/share/metainfo/search.metainfo.xml"
install -Dm755 packaging/AppRun "$APPDIR/AppRun"
mkdir -p "$APPDIR/usr/share/search"
cp -a assets/reader.js "$APPDIR/usr/share/search/reader.js"
cp -a assets/shield "$APPDIR/usr/share/search/shield"
cp -a /usr/lib/webkit2gtk-4.1 "$APPDIR/usr/lib/"
cp -a /usr/lib/gdk-pixbuf-2.0 "$APPDIR/usr/lib/"
cp -a /usr/lib/gio "$APPDIR/usr/lib/"
cp -a /usr/lib/gtk-3.0 "$APPDIR/usr/lib/"
cp -a /usr/lib/gstreamer-1.0 "$APPDIR/usr/lib/"
cp -a /usr/share/glib-2.0 "$APPDIR/usr/share/"
cp -a /usr/share/gtk-3.0 "$APPDIR/usr/share/"
cp -a /usr/share/mime "$APPDIR/usr/share/"
cp -a /usr/share/icons/Adwaita "$APPDIR/usr/share/icons/"
cp -a /usr/share/X11 "$APPDIR/usr/share/"
mkdir -p "$APPDIR/usr/share/fonts"
cp -a /usr/share/fonts/TTF/DejaVuSans*.ttf "$APPDIR/usr/share/fonts/"
cp -a /usr/share/fonts/Adwaita "$APPDIR/usr/share/fonts/"
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
  <dir prefix="xdg">fonts</dir>
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
sha256sum "$OUT" > "$OUT.sha256"
echo "Built $OUT"
