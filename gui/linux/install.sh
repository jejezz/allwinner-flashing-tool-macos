#!/usr/bin/env bash
#
# Installs this app for the current user, so it shows up in the desktop's
# app launcher with its icon:
#
#   ./install.sh            install (or upgrade)
#   ./install.sh --remove   uninstall
#
# Copied into the release tarball by .github/workflows/release.yml, which
# fills in @FILE_NAME@ and @APP_ID@. Nothing is written outside ~/.local.

set -euo pipefail

FILE_NAME="@FILE_NAME@"
APP_ID="@APP_ID@"

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
INSTALL_DIR="$DATA_HOME/$FILE_NAME"
DESKTOP_FILE="$DATA_HOME/applications/$APP_ID.desktop"
ICON_FILE="$DATA_HOME/icons/hicolor/512x512/apps/$APP_ID.png"

refresh() {
  command -v update-desktop-database >/dev/null && update-desktop-database "$DATA_HOME/applications" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$DATA_HOME/icons/hicolor" || true
}

if [ "${1:-}" = "--remove" ]; then
  rm -rf "$INSTALL_DIR" "$DESKTOP_FILE" "$ICON_FILE"
  refresh
  echo "Removed $FILE_NAME."
  exit 0
fi

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

rm -rf "$INSTALL_DIR"
mkdir -p "$INSTALL_DIR" "$(dirname "$DESKTOP_FILE")" "$(dirname "$ICON_FILE")"
cp -r "$SRC_DIR"/. "$INSTALL_DIR"/
rm -rf "$INSTALL_DIR/share" "$INSTALL_DIR/install.sh"

cp "$SRC_DIR/share/icons/hicolor/512x512/apps/$APP_ID.png" "$ICON_FILE"
sed "s|@INSTALL_DIR@|$INSTALL_DIR|g" "$SRC_DIR/share/applications/$APP_ID.desktop" > "$DESKTOP_FILE"

refresh
echo "Installed $FILE_NAME to $INSTALL_DIR"
