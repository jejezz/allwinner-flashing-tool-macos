#!/usr/bin/env bash
#
# README screenshots and demo GIF (conventions/readme-guide.md §4). macOS only.
#
#   tool/readme/capture.sh shot <name>          window → docs/screenshots/raw/<name>.png
#   tool/readme/capture.sh record <seconds>     window area → docs/screenshots/raw/demo.mov
#   tool/readme/capture.sh gif [in.mov]         raw/demo.mov → docs/screenshots/demo.gif
#
# Run the app first (flutter run -d macos, release mode looks best) and
# size its window with the app's default size. `shot` and `record` find the
# window by the app's display name (AppInfo.xcconfig PRODUCT_NAME), so the
# app doesn't need to be in front. Then frame the raw shots:
#
#   python3 tool/readme/frame.py docs/screenshots/raw/home.png
#   python3 tool/readme/frame.py --split docs/screenshots/raw/home-light.png docs/screenshots/raw/home-dark.png -o home.png
#
# The first `shot`/`record` asks for the Screen Recording permission for the
# terminal app — grant it in System Settings → Privacy & Security, then rerun.
#
# From jejezz/application-release-templates common/ @ conventions-v1.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$root/tool/readme"
raw="$root/docs/screenshots/raw"
out="$root/docs/screenshots"
# allwinner-phoenix: the Flutter app lives in gui/.
app="$root/gui"

GIF_WIDTH="${GIF_WIDTH:-760}"   # MacBroom's demo.gif: 760px, 15 fps
GIF_FPS="${GIF_FPS:-15}"

app_name() {
  grep '^PRODUCT_NAME' "$app/macos/Runner/Configs/AppInfo.xcconfig" | head -1 \
    | sed -E 's/^PRODUCT_NAME[[:space:]]*=[[:space:]]*//'
}

window() {
  swift "$here/find_window.swift" "${APP_NAME:-$(app_name)}"
}

cmd="${1:-}"
case "$cmd" in
  shot)
    name="${2:?usage: capture.sh shot <name>}"
    read -r id _ <<<"$(window)"
    mkdir -p "$raw"
    # -o: no window shadow (frame.py draws a consistent one); -x: no sound.
    screencapture -x -o -l"$id" "$raw/$name.png"
    echo "wrote $raw/$name.png"
    ;;

  record)
    seconds="${2:?usage: capture.sh record <seconds>}"
    read -r _ x y w h <<<"$(window)"
    mkdir -p "$raw"
    echo "recording ${seconds}s of the window area in 3s — keep the window where it is"
    sleep 3
    screencapture -x -v -V"$seconds" -R"$x,$y,$w,$h" "$raw/demo.mov"
    echo "wrote $raw/demo.mov — now: tool/readme/capture.sh gif"
    ;;

  gif)
    in="${2:-$raw/demo.mov}"
    [ -f "$in" ] || { echo "not found: $in" >&2; exit 1; }
    command -v ffmpeg >/dev/null || { echo "needs ffmpeg (brew install ffmpeg)" >&2; exit 1; }
    mkdir -p "$out"
    # Two-pass palette: GIF has 256 colours, so build them from this video
    # instead of a generic palette (much less banding on gradients).
    filters="fps=$GIF_FPS,scale=$GIF_WIDTH:-1:flags=lanczos"
    ffmpeg -loglevel error -y -i "$in" \
      -vf "$filters,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle" \
      "$out/demo.gif"
    size=$(stat -f%z "$out/demo.gif")
    echo "wrote $out/demo.gif ($((size / 1024)) KB)"
    if [ "$size" -gt 5000000 ]; then
      echo "warning: over 5 MB — record fewer seconds, or GIF_FPS=12 / GIF_WIDTH=680" >&2
    fi
    ;;

  *)
    sed -n '3,8p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
    ;;
esac
