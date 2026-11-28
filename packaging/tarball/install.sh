#!/bin/sh
# Install Rill from an extracted binary tarball into PREFIX (default: ~/.local
# for unprivileged users, /usr/local when run as root).
#
# Preserves legacy profile directories (~/.config/search, ~/.local/share/search,
# ~/.cache/search).
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "$(id -u)" -eq 0 ]; then
  PREFIX="/usr/local"
else
  PREFIX="${HOME}/.local"
fi

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)
      [ $# -ge 2 ] || { echo "Missing argument for --prefix" >&2; exit 2; }
      PREFIX="$2"
      shift 2
      ;;
    --prefix=*)
      PREFIX="${1#--prefix=}"
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [--prefix DIR]"
      echo "Default prefix: ~/.local (non-root) or /usr/local (root)."
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 2
      ;;
  esac
done

# Reject empty prefixes or prefixes containing control characters, newlines,
# backslashes, double-quotes, or '%' (GLib GDesktopAppInfo rejects '%' inside
# the Exec executable token even when escaped as '%%').
if [ -z "$PREFIX" ]; then
  echo "Refusing empty --prefix." >&2
  exit 2
fi
case "$PREFIX" in
  *"
"*|*"	"*|*"\\"*|*'"'*|*"%"*)
    echo "Refusing unsupported --prefix containing newline, tab, backslash, double-quote, or '%': $PREFIX" >&2
    exit 2
    ;;
esac

# Canonicalize relative prefix to an absolute path so Search.desktop Exec works
# from any working directory.
case "$PREFIX" in
  /*) ;;
  *) PREFIX="$PWD/$PREFIX" ;;
esac
# Strip trailing slash unless PREFIX is "/".
while [ "$PREFIX" != "/" ] && [ "${PREFIX%/}" != "$PREFIX" ]; do
  PREFIX="${PREFIX%/}"
done

# Serialize a binary path for the Desktop Entry Exec= key per the FreeDesktop
# Desktop Entry Specification:
#   1. Escape literal '%' as '%%' (field-code escaping).
#   2. If the path contains spaces or reserved Exec characters, enclose it in
#      double quotes and escape '$' and '`' (backslash and double-quote were
#      already rejected above).
desktop_exec_token() {
  raw="$1"
  # Escape '%' -> '%%' using POSIX parameter expansion loop.
  rest="$raw"
  pct_escaped=""
  while :; do
    case "$rest" in
      *%*)
        pct_escaped="${pct_escaped}${rest%%\%*}%%"
        rest="${rest#*\%}"
        ;;
      *)
        pct_escaped="${pct_escaped}${rest}"
        break
        ;;
    esac
  done

  needs_quote=0
  case "$pct_escaped" in
    *[[:space:]]*|*"'"*|*">"*|*"<"*|*"~"*|*"|"*|*"&"*|*";"*|*'$'*|*"*"*|*"?"*|*"#"*|*"("*|*")"*)
      needs_quote=1
      ;;
  esac
  case "$pct_escaped" in
    *'`'*) needs_quote=1 ;;
  esac

  if [ "$needs_quote" -eq 0 ]; then
    printf '%s' "$pct_escaped"
    return 0
  fi

  # Inside double quotes in Desktop Entry Exec, escape '$' as '\$' and '`' as '\`',
  # and double the backslash because Desktop Entry string unescaping runs before
  # Exec argument unquoting (so '\\$' in file -> '\$' for Exec -> literal '$').
  rest="$pct_escaped"
  out=""
  while [ -n "$rest" ]; do
    ch=$(printf '%s' "$rest" | cut -c1)
    rest=$(printf '%s' "$rest" | cut -c2-)
    case "$ch" in
      '$') out="${out}\\\\\$" ;;
      '`') out="${out}\\\\\`" ;;
      *)   out="${out}${ch}" ;;
    esac
  done
  printf '"%s"' "$out"
}

install -Dm755 "$HERE/bin/search" "$PREFIX/bin/search"
ln -sfn search "$PREFIX/bin/rill"
install -Dm644 "$HERE/share/icons/hicolor/512x512/apps/search.png" \
  "$PREFIX/share/icons/hicolor/512x512/apps/search.png"
install -Dm644 "$HERE/share/icons/hicolor/scalable/apps/search.svg" \
  "$PREFIX/share/icons/hicolor/scalable/apps/search.svg"
install -Dm644 "$HERE/share/metainfo/io.github.indar_beurre.Search.metainfo.xml" \
  "$PREFIX/share/metainfo/io.github.indar_beurre.Search.metainfo.xml"
install -Dm644 "$HERE/share/licenses/rill/LICENSE" \
  "$PREFIX/share/licenses/rill/LICENSE"

mkdir -p "$PREFIX/share/applications"
if [ "$PREFIX" = "/usr" ]; then
  install -m644 "$HERE/share/applications/Search.desktop" \
    "$PREFIX/share/applications/Search.desktop"
else
  # Non-/usr installs (such as ~/.local) may not be in the desktop session PATH.
  EXEC_TOKEN=$(desktop_exec_token "$PREFIX/bin/rill")
  TMP_DESKTOP="$PREFIX/share/applications/Search.desktop.tmp.$$"
  : > "$TMP_DESKTOP"
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      "Exec=/usr/bin/rill "*)
        printf 'Exec=%s %s\n' "$EXEC_TOKEN" "${line#Exec=/usr/bin/rill }" >> "$TMP_DESKTOP"
        ;;
      *)
        printf '%s\n' "$line" >> "$TMP_DESKTOP"
        ;;
    esac
  done < "$HERE/share/applications/Search.desktop"
  chmod 644 "$TMP_DESKTOP"
  mv -f "$TMP_DESKTOP" "$PREFIX/share/applications/Search.desktop"
fi

mkdir -p "$PREFIX/share/rill"
cat > "$PREFIX/share/rill/install-manifest.txt" <<EOF
bin/search
bin/rill
share/applications/Search.desktop
share/icons/hicolor/512x512/apps/search.png
share/icons/hicolor/scalable/apps/search.svg
share/metainfo/io.github.indar_beurre.Search.metainfo.xml
share/licenses/rill/LICENSE
share/rill/install-manifest.txt
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q "$PREFIX/share/applications" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

echo "Installed Rill to $PREFIX"
echo "Run: $PREFIX/bin/rill (or $PREFIX/bin/search)"
