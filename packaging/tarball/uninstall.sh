#!/bin/sh
# Uninstall Rill files installed by install.sh from PREFIX.
# NEVER deletes user profile data (~/.config/search, ~/.local/share/search, ~/.cache/search).
set -eu

if [ "$(id -u)" -eq 0 ]; then
  PREFIX="/usr/local"
else
  PREFIX="${HOME}/.local"
fi

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)
      PREFIX="$2"
      shift 2
      ;;
    --prefix=*)
      PREFIX="${1#--prefix=}"
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [--prefix DIR]"
      echo "Removes installed Rill package files from PREFIX without touching user profiles."
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 2
      ;;
  esac
done

for rel in \
  bin/rill \
  bin/search \
  share/applications/Search.desktop \
  share/icons/hicolor/512x512/apps/search.png \
  share/icons/hicolor/scalable/apps/search.svg \
  share/metainfo/io.github.indar_beurre.Search.metainfo.xml \
  share/licenses/rill/LICENSE \
  share/rill/install-manifest.txt
do
  rm -f "$PREFIX/$rel"
done

rmdir "$PREFIX/share/rill" 2>/dev/null || true
rmdir "$PREFIX/share/licenses/rill" 2>/dev/null || true

if command -v update-desktop-database >/dev/null 2>&1 && [ -d "$PREFIX/share/applications" ]; then
  update-desktop-database -q "$PREFIX/share/applications" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d "$PREFIX/share/icons/hicolor" ]; then
  gtk-update-icon-cache -q -t -f "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

echo "Uninstalled Rill from $PREFIX (user profile data in ~/.config/search and ~/.local/share/search was preserved)."
