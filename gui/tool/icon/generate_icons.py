#!/usr/bin/env python3
"""Generate every app icon from one glyph (conventions/icons.md).

    python3 tool/icon/generate_icons.py            # from assets/icon/source_glyph.svg or .png
    python3 tool/icon/generate_icons.py path/to/glyph.svg

From jejezz/application-release-templates common/ @ conventions-v1 — merges
allwinner-phoenix's macOS/Windows/Linux scripts with saturn-mobile's
iOS/Android one. Needs Pillow (`pip3 install pillow`).

The glyph is transparent artwork — an Icons8 "Sticker" icon, SVG preferred
(PNG: 1024px). An SVG is first rendered to source_glyph.png at 1024px with
macOS's own renderer (render_svg.swift, next to this file). Every icon puts
the glyph on the same plate — a diagonal gradient #7C6CFF → #E961FF with a
soft gloss on the upper half (MacBroom's icon) — so all apps read as one
family in the Dock, taskbar and home screen. The glyph tells apps apart; the
plate never changes.

Writes, for each platform folder that exists:

  assets/icon/app_icon_1024.png   master (macOS-shaped), for docs and stores
  assets/icon/app_icon.png        256px copy used inside the app (About dialog)
  macOS    AppIcon.appiconset     Apple icon grid: 824px rounded plate
                                  (radius 185) with a drop shadow in a 1024
                                  canvas, glyph 440px — a sticker's white
                                  outline needs gradient around it to read
  Windows  app_icon.ico           full-bleed plate (12% radius), glyph 80% —
                                  a macOS-sized glyph reads as too small in
                                  the taskbar
  Linux    app_icon.png 512px     same as Windows
  iOS      AppIcon.appiconset     square plate, glyph 76%, NO alpha channel
                                  (App Store Connect rejects one)
  Android  legacy mipmaps + adaptive icon (gradient background layer, glyph
           foreground inside the 66/108 safe zone)
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
ICON_DIR = ROOT / 'assets/icon'

# Top-left → bottom-right. The app colours violet and pink (MacBroom).
PLATE_START = (0x7C, 0x6C, 0xFF)
PLATE_END = (0xE9, 0x61, 0xFF)
# White over the upper part of the plate, bounded below by a wide arc.
GLOSS_ALPHA = 28  # of 255, about 11%

MASTER = 1024

# macOS: Apple's icon grid.
MAC_PLATE = 824
MAC_RADIUS = 185
MAC_GLYPH = 440
MAC_SHADOW_BLUR = 14
MAC_SHADOW_OFFSET = 10
MAC_SHADOW_ALPHA = 90

# Windows / Linux: nearly edge to edge.
DESKTOP_RADIUS_FRACTION = 0.12
DESKTOP_GLYPH_FRACTION = 0.80

# iOS / Android legacy: the OS applies its own mask to a square plate.
MOBILE_GLYPH_FRACTION = 0.76
# Android adaptive: only the inner 66dp of 108dp is guaranteed visible.
# 0.54 keeps a round-ish glyph whole inside that circle; lower it for
# artwork whose corners stick out further.
ADAPTIVE_GLYPH_FRACTION = 0.54

ANDROID_LEGACY = {'mdpi': 48, 'hdpi': 72, 'xhdpi': 96, 'xxhdpi': 144, 'xxxhdpi': 192}
ANDROID_ADAPTIVE = {'mdpi': 108, 'hdpi': 162, 'xhdpi': 216, 'xxhdpi': 324, 'xxxhdpi': 432}


def gradient(size: int) -> Image.Image:
    """The square plate: diagonal gradient plus gloss. Android uses it alone
    as the adaptive icon background."""
    # t = (x + y) / 2 across the square; build it from a 2x-long ramp.
    ramp = Image.new('RGB', (2 * size - 1, 1))
    for i in range(2 * size - 1):
        t = i / max(1, 2 * size - 2)
        ramp.putpixel((i, 0), tuple(
            round(PLATE_START[c] + (PLATE_END[c] - PLATE_START[c]) * t) for c in range(3)))
    plate = Image.new('RGB', (size, size))
    for y in range(size):
        plate.paste(ramp.crop((y, 0, y + size, 1)), (0, y))

    gloss = Image.new('L', (size, size), 0)
    ImageDraw.Draw(gloss).ellipse(
        (-0.30 * size, -0.75 * size, 1.30 * size, 0.53 * size), fill=GLOSS_ALPHA)
    plate.paste(Image.new('RGB', (size, size), (255, 255, 255)), (0, 0), gloss)
    return plate


def rounded_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new('L', (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def rounded_plate(size: int, radius: int) -> Image.Image:
    plate = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    plate.paste(gradient(size), (0, 0), rounded_mask(size, radius))
    return plate


def fit(glyph: Image.Image, box: int) -> Image.Image:
    """Crop the glyph to its opaque pixels and scale its longest side to `box`."""
    art = glyph.crop(glyph.getbbox())
    scale = box / max(art.size)
    return art.resize((max(1, round(art.width * scale)), max(1, round(art.height * scale))),
                      Image.LANCZOS)


def centre(canvas: Image.Image, art: Image.Image) -> Image.Image:
    canvas.alpha_composite(art, ((canvas.width - art.width) // 2,
                                 (canvas.height - art.height) // 2))
    return canvas


def macos_icon(glyph: Image.Image) -> Image.Image:
    plate = centre(rounded_plate(MAC_PLATE, MAC_RADIUS), fit(glyph, MAC_GLYPH))
    # Big Sur-style drop shadow inside the grid's margin.
    offset = (MASTER - MAC_PLATE) // 2
    shadow_mask = Image.new('L', (MASTER, MASTER), 0)
    shadow_mask.paste(ImageChops.multiply(rounded_mask(MAC_PLATE, MAC_RADIUS),
                                          Image.new('L', (MAC_PLATE, MAC_PLATE), MAC_SHADOW_ALPHA)),
                      (offset, offset + MAC_SHADOW_OFFSET))
    canvas = Image.new('RGBA', (MASTER, MASTER), (0, 0, 0, 0))
    canvas.putalpha(shadow_mask.filter(ImageFilter.GaussianBlur(MAC_SHADOW_BLUR)))
    return centre(canvas, plate)


def desktop_icon(glyph: Image.Image) -> Image.Image:
    plate = rounded_plate(MASTER, round(MASTER * DESKTOP_RADIUS_FRACTION))
    return centre(plate, fit(glyph, round(MASTER * DESKTOP_GLYPH_FRACTION)))


def mobile_icon(glyph: Image.Image) -> Image.Image:
    plate = gradient(MASTER).convert('RGBA')
    return centre(plate, fit(glyph, round(MASTER * MOBILE_GLYPH_FRACTION)))


def write_macos(icon: Image.Image) -> None:
    out = ROOT / 'macos/Runner/Assets.xcassets/AppIcon.appiconset'
    if not out.parent.parent.exists():
        return
    out.mkdir(parents=True, exist_ok=True)
    for size in (16, 32, 64, 128, 256, 512, 1024):
        icon.resize((size, size), Image.LANCZOS).save(out / f'app_icon_{size}.png')
    print(f'macOS    {out.relative_to(ROOT)}/app_icon_{{16..1024}}.png')


def write_windows(icon: Image.Image) -> None:
    out = ROOT / 'windows/runner/resources/app_icon.ico'
    if not out.parent.exists():
        return
    sizes = [256, 128, 64, 48, 32, 16]
    icon.save(out, format='ICO', sizes=[(s, s) for s in sizes])
    print(f'Windows  {out.relative_to(ROOT)} ({"/".join(map(str, sizes))})')


def write_linux(icon: Image.Image) -> None:
    runner = ROOT / 'linux/runner'
    if not runner.exists():
        return
    out = runner / 'resources/app_icon.png'
    out.parent.mkdir(parents=True, exist_ok=True)
    icon.resize((512, 512), Image.LANCZOS).save(out)
    print(f'Linux    {out.relative_to(ROOT)} (512)')


def write_ios(icon: Image.Image) -> None:
    out = ROOT / 'ios/Runner/Assets.xcassets/AppIcon.appiconset'
    contents = out / 'Contents.json'
    if not contents.exists():
        return
    flat = icon.convert('RGB')  # no alpha channel
    written = 0
    for image in json.loads(contents.read_text())['images']:
        name = image.get('filename')
        if not name:
            continue
        points = float(image['size'].split('x')[0])
        pixels = round(points * int(image['scale'].rstrip('x')))
        flat.resize((pixels, pixels), Image.LANCZOS).save(out / name)
        written += 1
    print(f'iOS      {out.relative_to(ROOT)} ({written} files, no alpha)')


def write_android(icon: Image.Image, glyph: Image.Image) -> None:
    res = ROOT / 'android/app/src/main/res'
    if not res.exists():
        return
    for density, size in ANDROID_LEGACY.items():
        (res / f'mipmap-{density}').mkdir(exist_ok=True)
        icon.resize((size, size), Image.LANCZOS).save(res / f'mipmap-{density}/ic_launcher.png')

    for density, size in ANDROID_ADAPTIVE.items():
        foreground = centre(Image.new('RGBA', (size, size), (0, 0, 0, 0)),
                            fit(glyph, round(size * ADAPTIVE_GLYPH_FRACTION)))
        foreground.save(res / f'mipmap-{density}/ic_launcher_foreground.png')
        gradient(size).save(res / f'mipmap-{density}/ic_launcher_background.png')

    anydpi = res / 'mipmap-anydpi-v26'
    anydpi.mkdir(exist_ok=True)
    (anydpi / 'ic_launcher.xml').write_text(
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<!-- Generated by tool/icon/generate_icons.py. -->\n'
        '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
        '    <background android:drawable="@mipmap/ic_launcher_background" />\n'
        '    <foreground android:drawable="@mipmap/ic_launcher_foreground" />\n'
        '</adaptive-icon>\n',
        encoding='utf-8')
    print(f'Android  {res.relative_to(ROOT)}/mipmap-* (legacy + adaptive)')


def render_svg(svg: Path) -> Path:
    """SVG → assets/icon/source_glyph.png (1024px). The PNG is committed too, so
    other platforms and CI can regenerate without macOS."""
    out = ICON_DIR / 'source_glyph.png'
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    helper = Path(__file__).with_name('render_svg.swift')
    try:
        subprocess.run(['swift', str(helper), str(svg), str(out), str(MASTER)], check=True)
    except (OSError, subprocess.CalledProcessError) as e:
        raise SystemExit(f'could not render {svg} ({e}) — needs macOS with swift, '
                         'or export a 1024px PNG to assets/icon/source_glyph.png')
    print(f'glyph    {svg.name} → {out.relative_to(ROOT)} ({MASTER})')
    return out


def main() -> None:
    if len(sys.argv) > 1:
        source = Path(sys.argv[1])
    elif (ICON_DIR / 'source_glyph.svg').exists():
        source = ICON_DIR / 'source_glyph.svg'
    else:
        source = ICON_DIR / 'source_glyph.png'
    if not source.exists():
        raise SystemExit(f'source glyph not found: {source}')
    if source.suffix.lower() == '.svg':
        source = render_svg(source)
    glyph = Image.open(source).convert('RGBA')
    if glyph.getbbox() is None:
        raise SystemExit(f'source glyph is fully transparent: {source}')

    mac = macos_icon(glyph)
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    mac.save(ICON_DIR / 'app_icon_1024.png')
    mac.resize((256, 256), Image.LANCZOS).save(ICON_DIR / 'app_icon.png')
    print(f'master   {(ICON_DIR / "app_icon_1024.png").relative_to(ROOT)}, app_icon.png (256)')

    write_macos(mac)
    desktop = desktop_icon(glyph)
    write_windows(desktop)
    write_linux(desktop)
    mobile = mobile_icon(glyph)
    write_ios(mobile)
    write_android(mobile, glyph)


if __name__ == '__main__':
    main()
