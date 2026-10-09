#!/usr/bin/env bash
# Build and package MarkupCraft for macOS on the host architecture:
#
#   $DIST/markupcraft-<version>-macos-<arch>.dmg   MarkupCraft.app on a drag-to-Applications disk image
#
# The bundle holds the app (Contents/MacOS/MarkupCraft), the CLI (Contents/MacOS/markupcraft-cli),
# Info.plist and the icon (built from assets/logo-512.png with sips + iconutil).
#
# Usage: packaging/macos/package.sh [--skip-build]
#
# Signing (env): MACOS_SIGN_IDENTITY, a codesign identity; default "-" (ad-hoc), which Apple
# silicon requires to run at all but which Gatekeeper does not trust on other Macs.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$(uname -m)" in
  arm64 | aarch64) ARCH=arm64 ;;
  x86_64) ARCH=x86_64 ;;
  *) echo "unsupported architecture $(uname -m)" >&2; exit 2 ;;
esac

# Keep in sync with LSMinimumSystemVersion in Info.plist.in.
export MACOSX_DEPLOYMENT_TARGET=11.0
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
SHORT_VERSION="${VERSION%%-*}"
WORK="$CARGO_TARGET_DIR/macos-package"
APP="$WORK/MarkupCraft.app"
DMG="$DIST/markupcraft-$VERSION-macos-$ARCH.dmg"

echo "==> MarkupCraft $VERSION for macOS $ARCH (signing identity: $IDENTITY)"
[ "$IDENTITY" = "-" ] && warn "macOS: ad-hoc signed; not notarized"

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p markupcraft -p markupcraft-cli)
fi
BIN="$CARGO_TARGET_DIR/release"

sign() {
  local ts=(--timestamp)
  [ "$IDENTITY" = "-" ] && ts=(--timestamp=none)
  codesign --force --sign "$IDENTITY" "${ts[@]}" "$@"
}

# ---- MarkupCraft.app ---------------------------------------------------------------------------
rm -rf "$WORK"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN/markupcraft" "$APP/Contents/MacOS/MarkupCraft"
cp "$BIN/markupcraft-cli" "$APP/Contents/MacOS/markupcraft-cli"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
  "$HERE/Info.plist.in" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"
copy_docs "$APP/Contents/Resources"

ICONSET="$WORK/MarkupCraft.iconset"
mkdir -p "$ICONSET"
SRC="$ROOT/assets/logo-512.png"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  if [ "$double" -le 512 ]; then
    sips -z "$double" "$double" "$SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
  fi
done
iconutil -c icns -o "$APP/Contents/Resources/MarkupCraft.icns" "$ICONSET"

# Sign inside-out: the nested executables first, then the bundle (which seals Info.plist).
sign --options runtime "$APP/Contents/MacOS/markupcraft-cli"
sign --options runtime "$APP/Contents/MacOS/MarkupCraft"
sign --options runtime "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"
smoke_cli "$APP/Contents/MacOS/markupcraft-cli"

# ---- DMG ---------------------------------------------------------------------------------------
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/MarkupCraft.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG" "$WORK/raw.dmg"
# makehybrid + convert builds the image without attaching a device, unlike `create -srcfolder`,
# which is flaky on CI runners ("Resource busy").
hdiutil makehybrid -hfs -hfs-volume-name "MarkupCraft $VERSION" -hfs-openfolder "$STAGE" -o "$WORK/raw.dmg" "$STAGE"
hdiutil convert "$WORK/raw.dmg" -format UDZO -imagekey zlib-level=9 -o "$DMG"
rm -f "$WORK/raw.dmg"
sign "$DMG"

echo "==> done"
ls -lh "$DMG"
