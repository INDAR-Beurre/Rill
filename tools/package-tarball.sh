#!/usr/bin/env bash
# Build a versioned native x86_64 binary tarball (.tar.gz) for Rill.
#
# Enforces an honest glibc ABI floor (default MAX_GLIBC=2.36). Refuses to build
# a portable tarball from a rolling-distro glibc 2.44 binary unless
# ALLOW_HOST_ABI=1 is explicitly set.
#
# Usage:
#   tools/package-tarball.sh [--skip-cargo-build] [--out-dir DIR]
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
      echo "  SEARCH_BINARY    pre-built release binary path (default: target/release/search)"
      echo "  MAX_GLIBC        maximum permitted GLIBC_* floor (default: 2.36)"
      echo "  ALLOW_HOST_ABI=1 bypass MAX_GLIBC guard (not for portable releases)"
      exit 0
      ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ $(uname -m) == x86_64 ]] || { echo "This script builds x86_64 only." >&2; exit 2; }

for cmd in tar gzip readelf sha256sum awk install; do
  command -v "$cmd" >/dev/null 2>&1 || { echo "Missing required tool: $cmd" >&2; exit 3; }
done

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
VERSION="${RILL_VERSION:-$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$ROOT/Cargo.toml")}"
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

BIN_MAX_GLIBC=$(readelf --version-info "$BINARY" 2>/dev/null \
  | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' | sed 's/^GLIBC_//' | sort -Vu | tail -n1 || echo "0.0")
BIN_REQUIRED_GLIBC=$(readelf -V "$BINARY" 2>/dev/null \
  | awk '/Name: GLIBC_/ && $0 !~ /WEAK/ {for (i=1;i<=NF;i++) if ($i ~ /^GLIBC_/) {sub(/^GLIBC_/,"",$i); print $i}}' \
  | sort -Vu | tail -n1 || echo "$BIN_MAX_GLIBC")
HOST_GLIBC=$(ldd --version 2>&1 | awk 'NR==1{print $NF}' || echo "unknown")

if [[ "${ALLOW_HOST_ABI:-0}" != "1" ]] && ! ver_le "$BIN_REQUIRED_GLIBC" "$MAX_GLIBC"; then
  echo "FATAL (ABI floor): $BINARY requires GLIBC_$BIN_REQUIRED_GLIBC (max $BIN_MAX_GLIBC), exceeding MAX_GLIBC=$MAX_GLIBC." >&2
  exit 7
fi
if [[ "${ALLOW_HOST_ABI:-0}" != "1" && "$HOST_GLIBC" != "unknown" ]] && ! ver_le "$HOST_GLIBC" "$MAX_GLIBC"; then
  echo "FATAL (ABI floor): build host glibc is $HOST_GLIBC (> MAX_GLIBC=$MAX_GLIBC)." >&2
  exit 7
fi

GLIBC_FLOOR="$BIN_REQUIRED_GLIBC"
if ver_le "$GLIBC_FLOOR" "2.36"; then
  GLIBC_FLOOR="2.36"
fi

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/rill-tarball-stage.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT
PKG_DIR="$STAGE/Rill-${VERSION}-linux-x86_64"

install -Dm755 "$BINARY" "$PKG_DIR/bin/search"
ln -sfn search "$PKG_DIR/bin/rill"
install -Dm644 "$ROOT/packaging/Search.desktop" "$PKG_DIR/share/applications/Search.desktop"
install -Dm644 "$ROOT/packaging/search.png" "$PKG_DIR/share/icons/hicolor/512x512/apps/search.png"
install -Dm644 "$ROOT/packaging/icon.svg" "$PKG_DIR/share/icons/hicolor/scalable/apps/search.svg"
install -Dm644 "$ROOT/packaging/search.metainfo.xml" "$PKG_DIR/share/metainfo/io.github.indar_beurre.Search.metainfo.xml"
install -Dm644 "$ROOT/LICENSE" "$PKG_DIR/share/licenses/rill/LICENSE"
install -Dm755 "$ROOT/packaging/tarball/install.sh" "$PKG_DIR/install.sh"
install -Dm755 "$ROOT/packaging/tarball/uninstall.sh" "$PKG_DIR/uninstall.sh"

sed \
  -e "s|@VERSION@|$VERSION|g" \
  -e "s|@GLIBC_FLOOR@|$GLIBC_FLOOR|g" \
  "$ROOT/packaging/tarball/README.txt.in" > "$PKG_DIR/README.txt"
chmod 0644 "$PKG_DIR/README.txt"

BUILD_OS=$(awk -F= '/^PRETTY_NAME=/{gsub(/"/,"",$2); print $2}' /etc/os-release 2>/dev/null || uname -sr)
BIN_SHA256=$(sha256sum "$BINARY" | awk '{print $1}')
NEEDED_LIST=$(readelf -d "$BINARY" | awk -F'[][]' '/\(NEEDED\)/{print $2}')

cat > "$PKG_DIR/BUILD_INFO.txt" <<EOF
package=rill
version=$VERSION
arch=x86_64
build_os=$BUILD_OS
host_glibc=$HOST_GLIBC
binary_required_glibc=$BIN_REQUIRED_GLIBC
binary_max_glibc=$BIN_MAX_GLIBC
declared_glibc_floor=$GLIBC_FLOOR
binary_sha256=$BIN_SHA256
dt_needed:
$(printf '  %s\n' $NEEDED_LIST)
EOF

mkdir -p "$OUT_DIR"
OUT_TAR="$OUT_DIR/Rill-${VERSION}-linux-x86_64.tar.gz"
rm -f "$OUT_TAR" "$OUT_TAR.sha256" "$OUT_TAR.abi.json"

tar --owner=0 --group=0 --numeric-owner -czf "$OUT_TAR" -C "$STAGE" "Rill-${VERSION}-linux-x86_64"
(cd "$OUT_DIR" && sha256sum "$(basename "$OUT_TAR")") > "$OUT_TAR.sha256"

OUT_SHA256=$(awk '{print $1}' "$OUT_TAR.sha256")
NEEDED_JSON=$(readelf -d "$BINARY" | awk -F'[][]' '/\(NEEDED\)/{printf "%s\"%s\"", (n++?", ":""), $2}')

cat > "$OUT_TAR.abi.json" <<EOF
{
  "format": "tar.gz",
  "artifact": "$(basename "$OUT_TAR")",
  "package": "rill",
  "version": "$VERSION",
  "arch": "x86_64",
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

echo "Built $OUT_TAR (SHA-256: $OUT_SHA256, required GLIBC: $BIN_REQUIRED_GLIBC, host GLIBC: $HOST_GLIBC)"
