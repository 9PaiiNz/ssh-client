#!/usr/bin/env bash
#
# Remove bundled libwayland-* from Tauri AppImages.
#
# linuxdeploy packs the build host's libwayland-{client,cursor,egl,server} into
# the AppImage. On user machines with a newer Mesa/Wayland stack, WebKit's EGL
# init then fails with:
#
#   Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...
#
# leaving a blank grey window. Deferring to the system libwayland (same as
# libGL/libEGL) fixes Wayland without needing LD_PRELOAD.
#
# Usage: scripts/appimage-unbundle-wayland.sh [bundle-dir]
#
set -euo pipefail

BUNDLE_DIR="${1:-src-tauri/target/release/bundle/appimage}"
ARCH="${ARCH:-$(uname -m)}"

if [[ ! -d "$BUNDLE_DIR" ]]; then
  echo "No AppImage bundle directory at $BUNDLE_DIR — nothing to do."
  exit 0
fi

shopt -s nullglob
APPIMAGES=("$BUNDLE_DIR"/*.AppImage)
if [[ ${#APPIMAGES[@]} -eq 0 ]]; then
  echo "No AppImage found in $BUNDLE_DIR — nothing to do."
  exit 0
fi

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

APPIMAGETOOL="$WORK_DIR/appimagetool"
echo "Fetching appimagetool for $ARCH"
curl -fsSL -o "$APPIMAGETOOL" \
  "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
chmod +x "$APPIMAGETOOL"

for APP in "${APPIMAGES[@]}"; do
  echo "==> Repacking $(basename "$APP") without bundled libwayland"

  EXTRACT_DIR="$WORK_DIR/extract"
  rm -rf "$EXTRACT_DIR"
  mkdir -p "$EXTRACT_DIR"
  APP_ABS="$(realpath "$APP")"
  (cd "$EXTRACT_DIR" && "$APP_ABS" --appimage-extract >/dev/null)

  REMOVED="$(
    find "$EXTRACT_DIR/squashfs-root" -type f \( \
      -name 'libwayland-client.so*' -o \
      -name 'libwayland-cursor.so*' -o \
      -name 'libwayland-egl.so*' -o \
      -name 'libwayland-server.so*' \) -print -delete || true
  )"

  if [[ -z "$REMOVED" ]]; then
    echo "    no bundled libwayland libraries found"
  else
    echo "$REMOVED" | sed 's/^/    removed /'
  fi

  ARCH="$ARCH" "$APPIMAGETOOL" --appimage-extract-and-run \
    "$EXTRACT_DIR/squashfs-root" "$APP.new" >/dev/null
  mv -f "$APP.new" "$APP"
  chmod +x "$APP"

  # Drop stale updater signature if present (we don't ship signed updates yet).
  rm -f "$APP.sig"

  echo "    done — $(basename "$APP") will use the host libwayland"
done
