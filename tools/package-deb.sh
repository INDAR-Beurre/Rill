#!/usr/bin/env bash
# Build a native Debian/Ubuntu amd64 .deb package for Rill.
#
# Enforces an honest glibc ABI floor (default MAX_GLIBC=2.36 for Debian 12+
# and Ubuntu 24.04+ compatibility). Refuses to repackage a rolling-distro host
# binary unless ALLOW_HOST_ABI=1 is explicitly set.
#
# Usage:
#   tools/package-deb.sh [--skip-cargo-build] [--out-dir DIR]
set -euo pipefail

SKIP_CARGO_BUILD=0
OUT_DIR=""
while (( $# > 0 )); do
  case "$1" in
    --skip-cargo-build) SKIP_CARGO_BUILD=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--skip-cargo-build] [--out-dir DIR]"
      echo "  RILL_VERSION     override version from Cargo.toml"
      echo "  DEB_REVISION     Debian package revision (default: 1)"
      echo "  SEARCH_BINARY    pre-built release binary path (default: target/release/search)"
      echo "  MAX_GLIBC        maximum permitted GLIBC_* symbol floor (default: 2.36)"
      echo "  ALLOW_HOST_ABI=1 bypass MAX_GLIBC guard (not for portable releases)"
      exit 0
      ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ $(uname -m) == x86_64 ]] || { echo "This script builds amd64 (x86_64) only." >&2; exit 2; }

for cmd in dpkg-deb readelf sha256sum md5sum awk install; do
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "Missing required tool '$cmd'. Build .deb inside a Debian/Ubuntu environment with dpkg-dev." >&2
    exit 3
  }
done

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
VERSION="${RILL_VERSION:-$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$ROOT/Cargo.toml")}"
REVISION="${DEB_REVISION:-1}"
MAX_GLIBC="${MAX_GLIBC:-2.36}"
OUT_DIR="${OUT_DIR:-$ROOT/build/packages}"
case "$OUT_DIR" in /*) ;; *) OUT_DIR="$PWD/$OUT_DIR" ;; esac
BINARY="${SEARCH_BINARY:-$ROOT/target/release/search}"
case "$BINARY" in /*) ;; *) BINARY="$PWD/$BINARY" ;; esac

cd "$ROOT"
if [[ "$SKIP_CARGO_BUILD" != "1" ]]; then
  CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-1} cargo build --release --bin search
fi
[[ -x "$BINARY" ]] || { echo "Missing release binary: $BINARY" >&2; exit 3; }

ver_le() {
  [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" == "$2" ]]
}

# Inspect actual ELF GLIBC floor and DT_NEEDED dependencies.
BIN_MAX_GLIBC=$(readelf --version-info "$BINARY" 2>/dev/null \
  | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' | sed 's/^GLIBC_//' | sort -Vu | tail -n1 || echo "0.0")
BIN_REQUIRED_GLIBC=$(readelf -V "$BINARY" 2>/dev/null \
  | awk '/Name: GLIBC_/ && $0 !~ /WEAK/ {for (i=1;i<=NF;i++) if ($i ~ /^GLIBC_/) {sub(/^GLIBC_/,"",$i); print $i}}' \
  | sort -Vu | tail -n1 || echo "$BIN_MAX_GLIBC")
HOST_GLIBC=$(ldd --version 2>&1 | awk 'NR==1{print $NF}' || echo "unknown")

if [[ "${ALLOW_HOST_ABI:-0}" != "1" ]] && ! ver_le "$BIN_REQUIRED_GLIBC" "$MAX_GLIBC"; then
  echo "FATAL (ABI floor): $BINARY requires GLIBC_$BIN_REQUIRED_GLIBC (max symbol $BIN_MAX_GLIBC), exceeding MAX_GLIBC=$MAX_GLIBC." >&2
  echo "Compile inside a Debian 12 (glibc 2.36) environment; do not repackage a rolling-host binary." >&2
  exit 7
fi

if [[ "${ALLOW_HOST_ABI:-0}" != "1" && "$HOST_GLIBC" != "unknown" ]] && ! ver_le "$HOST_GLIBC" "$MAX_GLIBC"; then
  echo "FATAL (ABI floor): build host glibc is $HOST_GLIBC (> MAX_GLIBC=$MAX_GLIBC)." >&2
  echo "Build .deb on Debian 12 (glibc 2.36) so linked library ABIs match target distros." >&2
  exit 7
fi

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/rill-deb-stage.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/DEBIAN"
install -Dm755 "$BINARY" "$STAGE/usr/bin/search"
ln -sfn search "$STAGE/usr/bin/rill"
install -Dm644 "$ROOT/packaging/Search.desktop" "$STAGE/usr/share/applications/Search.desktop"
install -Dm644 "$ROOT/packaging/search.png" "$STAGE/usr/share/icons/hicolor/512x512/apps/search.png"
install -Dm644 "$ROOT/packaging/icon.svg" "$STAGE/usr/share/icons/hicolor/scalable/apps/search.svg"
install -Dm644 "$ROOT/packaging/search.metainfo.xml" "$STAGE/usr/share/metainfo/io.github.indar_beurre.Search.metainfo.xml"
install -Dm644 "$ROOT/packaging/debian/copyright" "$STAGE/usr/share/doc/rill/copyright"
install -Dm755 "$ROOT/packaging/debian/postinst" "$STAGE/DEBIAN/postinst"
install -Dm755 "$ROOT/packaging/debian/postrm" "$STAGE/DEBIAN/postrm"

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$STAGE/usr/share/applications/Search.desktop"
fi

# Generate DEBIAN/md5sums for all regular installed files.
(cd "$STAGE" && find usr -type f | LC_ALL=C sort | xargs md5sum) > "$STAGE/DEBIAN/md5sums"
chmod 0644 "$STAGE/DEBIAN/md5sums"

INSTALLED_SIZE=$(du -sk "$STAGE/usr" | awk '{print $1}')
GLIBC_FLOOR="$BIN_REQUIRED_GLIBC"
if ver_le "$GLIBC_FLOOR" "2.36"; then
  GLIBC_FLOOR="2.36"
fi

sed \
  -e "s|@VERSION@|$VERSION|g" \
  -e "s|@REVISION@|$REVISION|g" \
  -e "s|@INSTALLED_SIZE@|$INSTALLED_SIZE|g" \
  -e "s|@GLIBC_FLOOR@|$GLIBC_FLOOR|g" \
  "$ROOT/packaging/debian/control.in" > "$STAGE/DEBIAN/control"
chmod 0644 "$STAGE/DEBIAN/control"

mkdir -p "$OUT_DIR"
OUT_DEB="$OUT_DIR/rill_${VERSION}-${REVISION}_amd64.deb"
rm -f "$OUT_DEB" "$OUT_DEB.sha256" "$OUT_DEB.abi.json"

dpkg-deb --root-owner-group --build "$STAGE" "$OUT_DEB"
(cd "$OUT_DIR" && sha256sum "$(basename "$OUT_DEB")") > "$OUT_DEB.sha256"

OUT_SHA256=$(awk '{print $1}' "$OUT_DEB.sha256")
BIN_SHA256=$(sha256sum "$BINARY" | awk '{print $1}')
BUILD_OS=$(awk -F= '/^PRETTY_NAME=/{gsub(/"/,"",$2); print $2}' /etc/os-release 2>/dev/null || uname -sr)
NEEDED_JSON=$(readelf -d "$BINARY" | awk -F'[][]' '/\(NEEDED\)/{printf "%s\"%s\"", (n++?", ":""), $2}')

cat > "$OUT_DEB.abi.json" <<EOF
{
  "format": "deb",
  "artifact": "$(basename "$OUT_DEB")",
  "package": "rill",
  "version": "${VERSION}-${REVISION}",
  "arch": "amd64",
  "sha256": "$OUT_SHA256",
  "binary_sha256": "$BIN_SHA256",
  "build_os": "$BUILD_OS",
  "host_glibc": "$HOST_GLIBC",
  "binary_required_glibc": "$BIN_REQUIRED_GLIBC",
  "binary_max_glibc": "$BIN_MAX_GLIBC",
  "declared_glibc_floor": "$GLIBC_FLOOR",
  "dt_needed": [$NEEDED_JSON]
}
EOF

echo "Built $OUT_DEB (SHA-256: $OUT_SHA256, required GLIBC: $BIN_REQUIRED_GLIBC, host GLIBC: $HOST_GLIBC)"
