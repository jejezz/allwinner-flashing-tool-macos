#!/usr/bin/env bash
#
# Build the macOS app icon set from the source artwork.
#
#   ./scripts/make-icon.sh [source.png]
#
# The source is a bare phoenix glyph on transparency. macOS since Big Sur
# expects an app icon to be a rounded square with the artwork inset — a bare
# glyph reads as a broken icon next to everything else in the Dock — so this
# composites it onto the app's own dark surface colour, on Apple's icon grid
# (an 824x824 rounded rect centred in a 1024 canvas, corner radius 185).

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="${1:-$root/assets/icon-source.png}"
out="$root/gui/macos/Runner/Assets.xcassets/AppIcon.appiconset"

command -v magick >/dev/null || { echo "needs ImageMagick (brew install imagemagick)" >&2; exit 1; }
[[ -f "$src" ]] || { echo "source not found: $src" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Surface gradient from lib/theme.dart (surfaceHi -> bg), so the icon belongs
# to the same family as the window.
magick -size 824x824 gradient:'#1F2A38-#0A0E14' \
    \( -size 824x824 xc:none -draw 'roundrectangle 0,0,823,823,185,185' -alpha extract \) \
    -alpha off -compose CopyOpacity -composite \
    "$work/plate.png"

# The glyph is orange on transparency; 600px inside 824 leaves the margin the
# rest of the Dock has.
magick "$work/plate.png" \
    \( "$src" -resize 600x600 \) -gravity center -compose over -composite \
    -background none -gravity center -extent 1024x1024 \
    "$work/icon_1024.png"

for size in 512 256 128 64 32 16; do
    magick "$work/icon_1024.png" -resize "${size}x${size}" "$work/icon_$size.png"
done

for size in 16 32 64 128 256 512 1024; do
    cp "$work/icon_$size.png" "$out/app_icon_$size.png"
done

echo "wrote $out/app_icon_{16,32,64,128,256,512,1024}.png"
