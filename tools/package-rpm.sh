#!/usr/bin/env bash
# Build a native x86_64 RPM package for Rill using rpmbuild.
#
# Refuses to fabricate an RPM when rpmbuild is absent or when the build host's
# glibc exceeds MAX_GLIBC (default 2.40 for Fedora 40+ / openSUSE / modern RPM
# distros; set MAX_GLIBC=2.36 if building from a Debian 12 binary on a verified
# RPM host).
#
# Usage:
#   tools/package-rpm.sh [--skip-cargo-build] [--out-dir DIR]
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
      echo "  RPM_RELEASE      RPM release number (default: 1)"
      echo "  SEARCH_BINARY    pre-built release binary path (only with --skip-cargo-build)"
      echo "  MAX_GLIBC        maximum permitted GLIBC_* floor (default: 2.40)"
      echo "  ALLOW_HOST_ABI=1 bypass MAX_GLIBC guard"
      exit 0
      ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ $(uname -m) == x86_64 ]] || { echo "This script builds x86_64 only." >&2; exit 2; }

for cmd in rpmbuild readelf sha256sum tar awk; do
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "Missing required tool '$cmd'. Build .rpm inside a Fedora/openSUSE/RPM host with rpm-build." >&2
    exit 3
  }
done

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
VERSION="${RILL_VERSION:-$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$ROOT/Cargo.toml")}"
RELEASE="${RPM_RELEASE:-1}"
MAX_GLIBC="${MAX_GLIBC:-2.40}"
OUT_DIR="${OUT_DIR:-$ROOT/build/packages}"
case "$OUT_DIR" in /*) ;; *) OUT_DIR="$PWD/$OUT_DIR" ;; esac
BINARY="${SEARCH_BINARY:-$ROOT/target/release/search}"
case "$BINARY" in /*) ;; *) BINARY="$PWD/$BINARY" ;; esac

ver_le() {
  [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" == "$2" ]]
}

HOST_GLIBC=$(ldd --version 2>&1 | awk 'NR==1{print $NF}' || echo "unknown")
if [[ "${ALLOW_HOST_ABI:-0}" != "1" && "$HOST_GLIBC" != "unknown" ]] && ! ver_le "$HOST_GLIBC" "$MAX_GLIBC"; then
  echo "FATAL (ABI floor): host glibc is $HOST_GLIBC (> MAX_GLIBC=$MAX_GLIBC)." >&2
  echo "Build .rpm inside a supported RPM distribution (e.g. Fedora 40/41 or openSUSE Leap 15.6)." >&2
  exit 7
fi

TOPDIR=$(mktemp -d "${TMPDIR:-/tmp}/rill-rpmbuild.XXXXXX")
trap 'rm -rf "$TOPDIR"' EXIT
mkdir -p "$TOPDIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# Create clean source archive matching Source0: rill-<version>.tar.gz.
tar --exclude='.git' --exclude='target' --exclude='build' --exclude='council' \
  --transform="s|^\.|rill-${VERSION}|" \
  -czf "$TOPDIR/SOURCES/rill-${VERSION}.tar.gz" -C "$ROOT" .

cp "$ROOT/packaging/rpm/rill.spec" "$TOPDIR/SPECS/rill.spec"

RPM_ARGS=(
  --define "_topdir $TOPDIR"
  --define "rill_version $VERSION"
  --define "rill_release $RELEASE"
)
if [[ "$SKIP_CARGO_BUILD" == "1" ]]; then
  [[ -x "$BINARY" ]] || { echo "Missing pre-built binary: $BINARY" >&2; exit 3; }
  BIN_REQUIRED_GLIBC=$(readelf -V "$BINARY" 2>/dev/null \
    | awk '/Name: GLIBC_/ && $0 !~ /WEAK/ {for (i=1;i<=NF;i++) if ($i ~ /^GLIBC_/) {sub(/^GLIBC_/,"",$i); print $i}}' \
    | sort -Vu | tail -n1 || echo "0.0")
  if [[ "${ALLOW_HOST_ABI:-0}" != "1" ]] && ! ver_le "$BIN_REQUIRED_GLIBC" "$MAX_GLIBC"; then
    echo "FATAL (ABI floor): $BINARY requires GLIBC_$BIN_REQUIRED_GLIBC (> MAX_GLIBC=$MAX_GLIBC)." >&2
    exit 7
  fi
  RPM_ARGS+=(--define "prebuilt_binary $(readlink -f "$BINARY")")
fi

rpmbuild -bb "${RPM_ARGS[@]}" "$TOPDIR/SPECS/rill.spec"

BUILT_RPM=$(find "$TOPDIR/RPMS/x86_64" -maxdepth 1 -name "rill-${VERSION}-*.x86_64.rpm" | head -n1)
[[ -n "$BUILT_RPM" && -f "$BUILT_RPM" ]] || { echo "rpmbuild did not produce an x86_64 RPM." >&2; exit 1; }

mkdir -p "$OUT_DIR"
OUT_RPM="$OUT_DIR/$(basename "$BUILT_RPM")"
cp -f "$BUILT_RPM" "$OUT_RPM"
(cd "$OUT_DIR" && sha256sum "$(basename "$OUT_RPM")") > "$OUT_RPM.sha256"

OUT_SHA256=$(awk '{print $1}' "$OUT_RPM.sha256")
BUILD_OS=$(awk -F= '/^PRETTY_NAME=/{gsub(/"/,"",$2); print $2}' /etc/os-release 2>/dev/null || uname -sr)
RPM_REQUIRES=$(rpm -qpR "$OUT_RPM" 2>/dev/null | sed 's/"/\\"/g' | awk '{printf "%s\"%s\"", (n++?", ":""), $0}')

cat > "$OUT_RPM.abi.json" <<EOF
{
  "format": "rpm",
  "artifact": "$(basename "$OUT_RPM")",
  "package": "rill",
  "version": "${VERSION}-${RELEASE}",
  "arch": "x86_64",
  "sha256": "$OUT_SHA256",
  "build_os": "$BUILD_OS",
  "host_glibc": "$HOST_GLIBC",
  "rpm_requires": [$RPM_REQUIRES]
}
EOF

echo "Built $OUT_RPM (SHA-256: $OUT_SHA256, host GLIBC: $HOST_GLIBC)"
