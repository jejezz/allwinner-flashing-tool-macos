#!/usr/bin/env python3
"""Build the Windows and Linux app icons directly from the source glyph.

Earlier versions of this script reused macOS's already-composited icon
(`gui/macos/.../app_icon_1024.png`) and just cropped its outer margin. That
wasn't enough: `make-icon.sh` insets the glyph *twice* for macOS — first a
margin around the whole plate (so the Dock's own rounding has room), then
the glyph itself is resized down to ~73% of the plate for Apple's icon-grid
safety margin. Cropping only the outer margin still left the glyph looking
noticeably smaller than sibling app icons in the Windows taskbar. Worse, our
plate is a dark gradient that all but disappears against Windows' own dark
taskbar, so in practice only the glyph itself reads as "the icon" — and a
glyph occupying ~73% of an already-invisible plate reads as small.

This script instead composites directly from `assets/icon-source.png`, with
the glyph filling almost the entire canvas, independent from — and not
derived from — the macOS build. Needs Pillow (`pip install pillow`).

    python scripts/make-icon-win-linux.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageOps

root = Path(__file__).resolve().parent.parent
src = root / "assets/icon-source.png"

windows_out = root / "gui/windows/runner/resources/app_icon.ico"
linux_out = root / "gui/linux/runner/resources/app_icon.png"

if not src.exists():
    raise SystemExit(f"source not found: {src}")

CANVAS = 1024
# macOS goes 1024 -> 824 plate (80%) -> 600 glyph (73% of the plate, ~59% of
# the canvas). Windows/Linux icons conventionally go almost edge to edge, so
# this is deliberately much larger — most of the margin left is just enough
# to keep the glyph from looking clipped at small taskbar sizes.
GLYPH_FRACTION = 0.92
CORNER_RADIUS = round(CANVAS * 0.12)  # a light squircle, not macOS's ~22%

glyph = Image.open(src).convert("RGBA")
glyph = glyph.crop(glyph.split()[-1].getbbox())
glyph = ImageOps.contain(glyph, (round(CANVAS * GLYPH_FRACTION),) * 2, Image.LANCZOS)

# Same surface gradient as the app window (lib/theme.dart's surfaceHi -> bg),
# so the icon still belongs to the same family even filling the canvas.
top, bottom = (0x1F, 0x2A, 0x38), (0x0A, 0x0E, 0x14)
gradient_column = Image.new("RGB", (1, CANVAS))
for y in range(CANVAS):
    t = y / (CANVAS - 1)
    gradient_column.putpixel((0, y), tuple(round(top[c] + (bottom[c] - top[c]) * t) for c in range(3)))
gradient = gradient_column.resize((CANVAS, CANVAS))

mask = Image.new("L", (CANVAS, CANVAS), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, CANVAS - 1, CANVAS - 1), radius=CORNER_RADIUS, fill=255)

icon = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
icon.paste(gradient, (0, 0), mask)
icon.paste(glyph, ((CANVAS - glyph.width) // 2, (CANVAS - glyph.height) // 2), glyph)

windows_out.parent.mkdir(parents=True, exist_ok=True)
ico_sizes = [256, 128, 64, 48, 32, 16]
icon.save(windows_out, format="ICO", sizes=[(s, s) for s in ico_sizes])
print(f"wrote {windows_out} ({', '.join(str(s) for s in ico_sizes)})")

# Linux: GTK's gtk_window_set_icon_from_file() takes one file and scales it,
# so a single high-res PNG is enough.
linux_out.parent.mkdir(parents=True, exist_ok=True)
icon.resize((512, 512), Image.LANCZOS).save(linux_out, format="PNG")
print(f"wrote {linux_out} (512x512)")
