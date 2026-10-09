# shellcheck shell=bash
# Shared setup for the macOS and Linux packaging scripts. Source it:
#   . "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
#
# Exports:
#   ROOT              workspace root
#   VERSION           [workspace.package] version from Cargo.toml (override: MARKUPCRAFT_VERSION)
#   DIST              output directory for the packages (default: $ROOT/dist)
#   CARGO_TARGET_DIR  cargo's target directory (default: $ROOT/target), made absolute

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT

# The version lives in one place: `[workspace.package] version` in the root Cargo.toml.
# (`cargo xtask version` prints the same; awk avoids compiling xtask here.)
workspace_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]"); next }
    in_pkg && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }
  ' "$ROOT/Cargo.toml"
}

VERSION="${MARKUPCRAFT_VERSION:-$(workspace_version)}"
if [ -z "$VERSION" ]; then
  echo "error: could not read [workspace.package] version from $ROOT/Cargo.toml" >&2
  exit 1
fi
export VERSION

absolute() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s\n' "$ROOT/$1" ;;
  esac
}

DIST="$(absolute "${DIST:-dist}")"
mkdir -p "$DIST"
export DIST
CARGO_TARGET_DIR="$(absolute "${CARGO_TARGET_DIR:-target}")"
export CARGO_TARGET_DIR

# A GitHub Actions warning annotation (plain stderr elsewhere).
warn() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
}

# Copy the readme and licence files into a package directory.
copy_docs() {
  local dest="$1" f
  for f in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    if [ -f "$ROOT/$f" ]; then cp "$ROOT/$f" "$dest/"; fi
  done
}

# Start the CLI without arguments: it prints its usage and exits 2. Anything above 2 (a crash,
# a missing library) fails the package.
smoke_cli() {
  local status=0
  "$1" >/dev/null 2>&1 || status=$?
  if [ "$status" -gt 2 ]; then
    echo "error: $1 did not start (exit $status)" >&2
    exit 1
  fi
  echo "ok: $(basename "$1") starts (exit $status)"
}
