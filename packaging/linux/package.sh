#!/usr/bin/env bash
# Build and package MarkupCraft for Linux on the host architecture (x86_64 or aarch64):
#
#   $DIST/markupcraft-<version>-linux-<arch>.AppImage   one file, runs on distros with glibc >= the build host's
#   $DIST/markupcraft-<version>-linux-<arch>.tar.gz     FHS-style tree: bin/, share/applications, share/icons
#
# Usage: packaging/linux/package.sh [--skip-build] [--formats "appimage tar"]
#
# Needs: cargo; appimagetool for the AppImage (APPIMAGETOOL, or on PATH, or downloaded into
# $CARGO_TARGET_DIR). Optional: desktop-file-validate. The app loads GTK 3 (file dialogs),
# xkbcommon, Wayland/X11 and Vulkan/GL from the system at run time, as every desktop has them.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"

SKIP_BUILD=0
FORMATS="appimage tar"
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --formats) FORMATS="$2"; shift 2 ;;
    -h | --help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
BASENAME="markupcraft-$VERSION-linux-$ARCH"
has() { case " $FORMATS " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

echo "==> MarkupCraft $VERSION for Linux $ARCH ($FORMATS)"

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p markupcraft -p markupcraft-cli)
fi
BIN="$CARGO_TARGET_DIR/release"
WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/root"
rm -rf "$WORK"

# ---- stage an FHS tree (shared by both formats) -------------------------------------------------
install -Dm755 "$BIN/markupcraft" "$STAGE/usr/bin/markupcraft"
install -Dm755 "$BIN/markupcraft-cli" "$STAGE/usr/bin/markupcraft-cli"
strip "$STAGE/usr/bin/markupcraft" "$STAGE/usr/bin/markupcraft-cli" 2>/dev/null || true
install -Dm644 "$HERE/markupcraft.desktop" "$STAGE/usr/share/applications/markupcraft.desktop"
install -Dm644 "$ROOT/assets/logo-256.png" "$STAGE/usr/share/icons/hicolor/256x256/apps/markupcraft.png"
install -Dm644 "$ROOT/assets/logo-512.png" "$STAGE/usr/share/icons/hicolor/512x512/apps/markupcraft.png"
install -Dm644 "$ROOT/assets/logo.svg" "$STAGE/usr/share/icons/hicolor/scalable/apps/markupcraft.svg"
mkdir -p "$STAGE/usr/share/doc/markupcraft"
copy_docs "$STAGE/usr/share/doc/markupcraft"

if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$STAGE/usr/share/applications/markupcraft.desktop"
fi
smoke_cli "$STAGE/usr/bin/markupcraft-cli"

# ---- .tar.gz ------------------------------------------------------------------------------------
if has tar; then
  mkdir -p "$WORK/tar"
  cp -R "$STAGE/usr" "$WORK/tar/$BASENAME"
  tar -C "$WORK/tar" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
  echo "wrote $DIST/$BASENAME.tar.gz"
fi

# ---- AppImage -----------------------------------------------------------------------------------
if has appimage; then
  APPDIR="$WORK/MarkupCraft.AppDir"
  cp -R "$STAGE" "$APPDIR"
  ln -s usr/bin/markupcraft "$APPDIR/AppRun"
  cp "$HERE/markupcraft.desktop" "$APPDIR/markupcraft.desktop"
  cp "$ROOT/assets/logo-256.png" "$APPDIR/markupcraft.png"
  ln -s markupcraft.png "$APPDIR/.DirIcon"

  TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
  if [ -z "$TOOL" ]; then
    TOOL="$CARGO_TARGET_DIR/appimagetool-$ARCH.AppImage"
    if [ ! -x "$TOOL" ]; then
      curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
      chmod +x "$TOOL"
    fi
  fi
  OUT="$DIST/$BASENAME.AppImage"
  rm -f "$OUT"
  # Extract-and-run: works without FUSE (containers, CI runners).
  ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT"
  chmod +x "$OUT"
  echo "wrote $OUT"
fi

echo "==> done"
ls -lh "$DIST"
