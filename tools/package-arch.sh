#!/usr/bin/env bash
# Build an honest native Arch/Artix x86_64 package (.pkg.tar.zst) using makepkg
# in an isolated temporary directory without modifying the host or repo.
#
# Usage:
#   tools/package-arch.sh [--skip-cargo-build] [--out-dir DIR]
set -euo pipefail

SKIP_CARGO_BUILD=0
OUT_DIR=""
while (( $# > 0 )); do
  case "$1" in
    --skip-cargo-build) SKIP_CARGO_BUILD=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--skip-cargo-build] [--out-dir DIR]"
      echo "  RILL_VERSION   override version from Cargo.toml"
      echo "  ARCH_PKGREL    Arch pkgrel (default: 1)"
      echo "  SEARCH_BINARY  pre-built release binary (with --skip-cargo-build)"
      exit 0
      ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ $(uname -m) == x86_64 ]] || { echo "This script builds x86_64 only." >&2; exit 2; }
[[ $(id -u) -ne 0 ]] || { echo "makepkg must not be run as root." >&2; exit 2; }

for cmd in makepkg tar readelf sha256sum awk; do
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "Missing required tool '$cmd'. Build Arch packages on an Arch/Artix host with makepkg." >&2
    exit 3
  }
done

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
VERSION="${RILL_VERSION:-$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$ROOT/Cargo.toml")}"
PKGREL="${ARCH_PKGREL:-1}"
OUT_DIR="${OUT_DIR:-$ROOT/build/packages}"
case "$OUT_DIR" in /*) ;; *) OUT_DIR="$PWD/$OUT_DIR" ;; esac
BINARY="${SEARCH_BINARY:-$ROOT/target/release/search}"
case "$BINARY" in /*) ;; *) BINARY="$PWD/$BINARY" ;; esac

WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/rill-archpkg.XXXXXX")
trap 'rm -rf "$WORKDIR"' EXIT

if [[ "$SKIP_CARGO_BUILD" == "1" ]]; then
  [[ -x "$BINARY" ]] || { echo "Missing release binary: $BINARY" >&2; exit 3; }
  mkdir -p "$WORKDIR/src-tree/Rill-${VERSION}/target/release"
  cp "$BINARY" "$WORKDIR/src-tree/Rill-${VERSION}/target/release/search"
  cp -a "$ROOT/packaging" "$ROOT/LICENSE" "$WORKDIR/src-tree/Rill-${VERSION}/"
  tar -czf "$WORKDIR/rill-${VERSION}.tar.gz" -C "$WORKDIR/src-tree" "Rill-${VERSION}"
  SRC_SHA=$(sha256sum "$WORKDIR/rill-${VERSION}.tar.gz" | awk '{print $1}')
  sed \
    -e "s|^pkgver=.*|pkgver=${VERSION}|" \
    -e "s|^pkgrel=.*|pkgrel=${PKGREL}|" \
    -e "s|^source=.*|source=(\"rill-${VERSION}.tar.gz\")|" \
    -e "s|^sha256sums=.*|sha256sums=('${SRC_SHA}')|" \
    -e '/^prepare()/,/^}/d' \
    -e '/^build()/,/^}/d' \
    "$ROOT/packaging/arch/PKGBUILD" > "$WORKDIR/PKGBUILD"
else
  tar --exclude='.git' --exclude='target' --exclude='build' --exclude='council' \
    --transform="s|^\.|Rill-${VERSION}|" \
    -czf "$WORKDIR/rill-${VERSION}.tar.gz" -C "$ROOT" .
  SRC_SHA=$(sha256sum "$WORKDIR/rill-${VERSION}.tar.gz" | awk '{print $1}')
  sed \
    -e "s|^pkgver=.*|pkgver=${VERSION}|" \
    -e "s|^pkgrel=.*|pkgrel=${PKGREL}|" \
    -e "s|^source=.*|source=(\"rill-${VERSION}.tar.gz\")|" \
    -e "s|^sha256sums=.*|sha256sums=('${SRC_SHA}')|" \
    "$ROOT/packaging/arch/PKGBUILD" > "$WORKDIR/PKGBUILD"
fi

# Build cleanly inside WORKDIR without installing anything onto the host.
(cd "$WORKDIR" && PKGDEST="$WORKDIR/out" SRCDEST="$WORKDIR" BUILDDIR="$WORKDIR/build" makepkg --nodeps --clean --noconfirm)

BUILT_PKG=$(find "$WORKDIR/out" -maxdepth 1 -name "rill-${VERSION}-${PKGREL}-x86_64.pkg.tar.*" ! -name '*.sig' | head -n1)
[[ -n "$BUILT_PKG" && -f "$BUILT_PKG" ]] || { echo "makepkg did not produce a package archive." >&2; exit 1; }

mkdir -p "$OUT_DIR"
OUT_PKG="$OUT_DIR/$(basename "$BUILT_PKG")"
cp -f "$BUILT_PKG" "$OUT_PKG"
(cd "$OUT_DIR" && sha256sum "$(basename "$OUT_PKG")") > "$OUT_PKG.sha256"

# Inspect actual packaged binary ELF ABI.
INSPECT_DIR="$WORKDIR/inspect"
mkdir -p "$INSPECT_DIR"
tar -xf "$OUT_PKG" -C "$INSPECT_DIR" usr/bin/search
PKG_BIN="$INSPECT_DIR/usr/bin/search"
BIN_SHA256=$(sha256sum "$PKG_BIN" | awk '{print $1}')
BIN_MAX_GLIBC=$(readelf --version-info "$PKG_BIN" 2>/dev/null \
  | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' | sed 's/^GLIBC_//' | sort -Vu | tail -n1 || echo "0.0")
BIN_REQUIRED_GLIBC=$(readelf -V "$PKG_BIN" 2>/dev/null \
  | awk '/Name: GLIBC_/ && $0 !~ /WEAK/ {for (i=1;i<=NF;i++) if ($i ~ /^GLIBC_/) {sub(/^GLIBC_/,"",$i); print $i}}' \
  | sort -Vu | tail -n1 || echo "$BIN_MAX_GLIBC")
HOST_GLIBC=$(ldd --version 2>&1 | awk 'NR==1{print $NF}' || echo "unknown")
BUILD_OS=$(awk -F= '/^PRETTY_NAME=/{gsub(/"/,"",$2); print $2}' /etc/os-release 2>/dev/null || uname -sr)
OUT_SHA256=$(awk '{print $1}' "$OUT_PKG.sha256")
NEEDED_JSON=$(readelf -d "$PKG_BIN" | awk -F'[][]' '/\(NEEDED\)/{printf "%s\"%s\"", (n++?", ":""), $2}')

cat > "$OUT_PKG.abi.json" <<EOF
{
  "format": "arch-pkg",
  "artifact": "$(basename "$OUT_PKG")",
  "package": "rill",
  "version": "${VERSION}-${PKGREL}",
  "arch": "x86_64",
  "target_family": "Arch/Artix rolling x86_64",
  "sha256": "$OUT_SHA256",
  "binary_sha256": "$BIN_SHA256",
  "build_os": "$BUILD_OS",
  "host_glibc": "$HOST_GLIBC",
  "binary_required_glibc": "$BIN_REQUIRED_GLIBC",
  "binary_max_glibc": "$BIN_MAX_GLIBC",
  "dt_needed": [$NEEDED_JSON]
}
EOF

echo "Built $OUT_PKG (SHA-256: $OUT_SHA256, host GLIBC: $HOST_GLIBC, target: Arch/Artix rolling)"
