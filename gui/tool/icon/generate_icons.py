#!/usr/bin/env python3
"""Generate every app icon from one glyph (conventions/icons.md).

    python3 tool/icon/generate_icons.py            # from assets/icon/source_glyph.svg or .png
    python3 tool/icon/generate_icons.py path/to/glyph.svg
    python3 tool/icon/generate_icons.py path/to/cat.png --app-id portside   # cat mode
    python3 tool/icon/generate_icons.py path/to/cat.png --app-id x --prop-tile
    python3 tool/icon/generate_icons.py path/to/cat.png --app-id x --hue 148  # fixed hue
    python3 tool/icon/generate_icons.py path/to/cat.png   # settings from assets/icon/cat.json

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
  Windows  app_icon.ico           the glyph alone, no plate, one frame per size
                                  — on a taskbar full of plates in the same
                                  colour only the silhouette tells apps apart
  Linux    app_icon.png 512px     same as Windows
  iOS      AppIcon.appiconset     square plate, glyph 76%, NO alpha channel
                                  (App Store Connect rejects one)
  Android  legacy mipmaps + adaptive icon (gradient background layer, glyph
           foreground inside the 66/108 safe zone)

Cat mode (--app-id, conventions/proposals/cat-icon.md): the glyph is the same
cat for every app and the plate colour tells apps apart. The plate hue comes
from the SHA-256 of the app id (same id, same colour, nothing to record), at
fixed saturation/value, skipping the orange-yellow band the cat itself lives
in. The plate fades top to bottom with a soft halo behind the cat, and the cat
casts a shadow. The app's own glyph (assets/icon/source_glyph.png, its old
Sticker icon) rides along as a prop badge in the lower right, so apps stay
apart even where there is no plate (Windows). Platform shapes (macOS 824px plate, Windows glyph alone, ...)
are unchanged. The cat artwork is a paid Icons8 asset: pass its path, do not
commit it to a public repository.
"""
from __future__ import annotations

import colorsys
import hashlib
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

# Cat mode. APP_HUE (0..1) is None in the default sticker mode.
APP_HUE: float | None = None
PROP_COMPOSED = False  # the glyph is already a cat+prop composite
PROP_TILE = False      # always put the prop on a dark tile (--prop-tile)
CAT_HUE_BANNED = (0.03, 0.20)  # about 10°-72°: the cat's own oranges and yellows
CAT_PLATE_TOP = (0.55, 0.95)   # (saturation, value)
CAT_PLATE_BOTTOM = (0.85, 0.55)
CAT_HALO = (0.25, 1.0)
CAT_HALO_ALPHA = 110
CAT_SHADOW_ALPHA = 0.35
CAT_MAC_GLYPH_FRACTION = 0.80  # of the 824px plate
# Cat + prop composite (1024 canvas fractions): the cat sits up-left of centre,
# the prop badge in the lower right, pulled in from the corner.
CAT_PROP_CAT = 0.68
CAT_PROP_CAT_OFFSET = (-0.05, -0.06)
CAT_PROP_BADGE = 0.42
CAT_PROP_MARGIN = 0.06
CAT_PROP_ADAPTIVE_FRACTION = 0.46  # the badge corner must stay in the safe zone

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
# Windows icons are the glyph alone: the taskbar and Explorer lists show many
# small icons side by side, and a shared plate colour makes them look alike.
# Every size is rendered on its own — a plain downscale of the 1024px master
# turns the glyph's thin outline to mush at 16-48px — and the small ones get
# a light unsharp mask. 20/24/40 are the sizes Windows asks for at 125-150%
# scaling; without them it stretches a neighbour and blurs.
WINDOWS_ICON_SIZES = [256, 128, 64, 48, 40, 32, 24, 20, 16]
WINDOWS_GLYPH_FRACTION = 0.96
SHARPEN_MAX = 48

# iOS / Android legacy: the OS applies its own mask to a square plate.
MOBILE_GLYPH_FRACTION = 0.76
# Android adaptive: only the inner 66dp of 108dp is guaranteed visible.
# 0.54 keeps a round-ish glyph whole inside that circle; lower it for
# artwork whose corners stick out further.
ADAPTIVE_GLYPH_FRACTION = 0.54

ANDROID_LEGACY = {'mdpi': 48, 'hdpi': 72, 'xhdpi': 96, 'xxhdpi': 144, 'xxxhdpi': 192}
ANDROID_ADAPTIVE = {'mdpi': 108, 'hdpi': 162, 'xhdpi': 216, 'xxhdpi': 324, 'xxxhdpi': 432}


def hue_for(app_id: str) -> float:
    """Stable hue in 0..1 from the app id, outside CAT_HUE_BANNED."""
    h = int(hashlib.sha256(app_id.encode()).hexdigest(), 16) % 10000 / 10000
    lo, hi = CAT_HUE_BANNED
    v = h * (1 - (hi - lo))
    return v if v < lo else v + (hi - lo)


def _hsv(h: float, sv: tuple[float, float]) -> tuple[int, ...]:
    return tuple(round(c * 255) for c in colorsys.hsv_to_rgb(h, *sv))


def cat_gradient(size: int, hue: float) -> Image.Image:
    top, bottom = _hsv(hue, CAT_PLATE_TOP), _hsv(hue, CAT_PLATE_BOTTOM)
    column = Image.new('RGB', (1, size))
    for y in range(size):
        t = y / max(1, size - 1)
        column.putpixel((0, y), tuple(round(top[c] + (bottom[c] - top[c]) * t) for c in range(3)))
    plate = column.resize((size, size)).convert('RGBA')
    halo = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(halo).ellipse(
        (0.107 * size, 0.127 * size, 0.893 * size, 0.912 * size),
        fill=_hsv(hue, CAT_HALO) + (CAT_HALO_ALPHA,))
    plate.alpha_composite(halo.filter(ImageFilter.GaussianBlur(0.039 * size)))
    return plate.convert('RGB')


def with_shadow(art: Image.Image) -> Image.Image:
    """The glyph with a soft shadow below it (padded canvas)."""
    pad = round(max(art.size) * 0.08)
    canvas = Image.new('RGBA', (art.width + 2 * pad, art.height + 2 * pad), (0, 0, 0, 0))
    alpha = art.getchannel('A').point(lambda v: round(v * CAT_SHADOW_ALPHA))
    shadow = Image.new('RGBA', art.size, (0, 0, 0, 255))
    shadow.putalpha(alpha)
    canvas.alpha_composite(shadow, (pad, pad + round(art.height * 0.022)))
    canvas = canvas.filter(ImageFilter.GaussianBlur(art.height * 0.018))
    canvas.alpha_composite(art, (pad, pad))
    return canvas


def shadowed(art: Image.Image) -> Image.Image:
    """Cat mode: shadow under the glyph, unless the cat+prop composite
    already shadowed its cat (the prop sticker has its own shadow)."""
    return art if APP_HUE is None or PROP_COMPOSED else with_shadow(art)


def is_plain_light(prop: Image.Image) -> bool:
    """A white, outline-free glyph: invisible on light backgrounds and busy on
    the cat, so it needs a backing tile."""
    px = [p for p in prop.getdata() if p[3] > 200]
    return bool(px) and min(min(p[:3]) for p in px) > 200


def backed(prop: Image.Image) -> Image.Image:
    """Plain white glyph on a dark rounded tile with a white sticker border."""
    tile = Image.new('RGBA', (MASTER, MASTER), (0, 0, 0, 0))
    draw = ImageDraw.Draw(tile)
    draw.rounded_rectangle((60, 60, 964, 964), radius=210, fill=(0xF2, 0xF2, 0xF2, 255))
    draw.rounded_rectangle((120, 120, 904, 904), radius=160, fill=(0x2A, 0x2D, 0x45, 255))
    # A plain glyph floats in the tile; a sticker already has its own border.
    return centre(tile, fit(prop, 470 if is_plain_light(prop) else 600))


def cat_with_prop(cat: Image.Image, prop: Image.Image) -> Image.Image:
    """Cat (with shadow) up-left, prop sticker lower right, on a MASTER canvas."""
    canvas = Image.new('RGBA', (MASTER, MASTER), (0, 0, 0, 0))
    cat_art = with_shadow(fit(cat, round(MASTER * CAT_PROP_CAT)))
    canvas.alpha_composite(cat_art, (
        (MASTER - cat_art.width) // 2 + round(CAT_PROP_CAT_OFFSET[0] * MASTER),
        (MASTER - cat_art.height) // 2 + round(CAT_PROP_CAT_OFFSET[1] * MASTER)))
    if PROP_TILE or is_plain_light(prop):
        prop = backed(prop)
    badge = fit(prop, round(MASTER * CAT_PROP_BADGE))
    margin = round(MASTER * CAT_PROP_MARGIN)
    canvas.alpha_composite(badge, (MASTER - badge.width - margin, MASTER - badge.height - margin))
    return canvas


def gradient(size: int) -> Image.Image:
    """The square plate: diagonal gradient plus gloss. Android uses it alone
    as the adaptive icon background."""
    if APP_HUE is not None:
        return cat_gradient(size, APP_HUE)
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


def adaptive_fraction() -> float:
    return CAT_PROP_ADAPTIVE_FRACTION if PROP_COMPOSED else ADAPTIVE_GLYPH_FRACTION


def macos_icon(glyph: Image.Image) -> Image.Image:
    box = round(MAC_PLATE * CAT_MAC_GLYPH_FRACTION) if APP_HUE is not None else MAC_GLYPH
    plate = centre(rounded_plate(MAC_PLATE, MAC_RADIUS), shadowed(fit(glyph, box)))
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
    return centre(plate, shadowed(fit(glyph, round(MASTER * DESKTOP_GLYPH_FRACTION))))


def mobile_icon(glyph: Image.Image) -> Image.Image:
    plate = gradient(MASTER).convert('RGBA')
    return centre(plate, shadowed(fit(glyph, round(MASTER * MOBILE_GLYPH_FRACTION))))


def write_macos(icon: Image.Image) -> None:
    out = ROOT / 'macos/Runner/Assets.xcassets/AppIcon.appiconset'
    if not out.parent.parent.exists():
        return
    out.mkdir(parents=True, exist_ok=True)
    for size in (16, 32, 64, 128, 256, 512, 1024):
        icon.resize((size, size), Image.LANCZOS).save(out / f'app_icon_{size}.png')
    print(f'macOS    {out.relative_to(ROOT)}/app_icon_{{16..1024}}.png')


def windows_icon(glyph: Image.Image, size: int) -> Image.Image:
    art = fit(glyph, round(size * WINDOWS_GLYPH_FRACTION))
    if size <= SHARPEN_MAX:
        art = art.filter(ImageFilter.UnsharpMask(radius=0.6, percent=120, threshold=0))
    return centre(Image.new('RGBA', (size, size), (0, 0, 0, 0)), art)


def write_windows(glyph: Image.Image) -> None:
    out = ROOT / 'windows/runner/resources/app_icon.ico'
    if not out.parent.exists():
        return
    frames = [windows_icon(glyph, s) for s in WINDOWS_ICON_SIZES]
    frames[0].save(out, format='ICO', sizes=[(s, s) for s in WINDOWS_ICON_SIZES],
                   append_images=frames[1:])
    print(f'Windows  {out.relative_to(ROOT)} ({"/".join(map(str, WINDOWS_ICON_SIZES))}, no plate)')


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
                            shadowed(fit(glyph, round(size * adaptive_fraction()))))
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


def cat_options(args: list[str]) -> tuple[str | None, float | None, bool, list[str]]:
    """Cat-mode settings: assets/icon/cat.json first, command-line flags override.

    cat.json: {"app_id": "portside", "hue": 148, "prop_tile": true} — only
    app_id is required. It records what the flags would say, so a plain
    `generate_icons.py <cat image>` reproduces the committed icons."""
    app_id = hue = None
    tile = False
    config = ICON_DIR / 'cat.json'
    if config.exists():
        data = json.loads(config.read_text(encoding='utf-8'))
        app_id = data['app_id']
        hue = data.get('hue')
        tile = bool(data.get('prop_tile', False))
    args = list(args)
    for flag in ('--app-id', '--hue'):
        if flag in args:
            i = args.index(flag)
            if i + 1 >= len(args):
                raise SystemExit(f'{flag} needs a value')
            if flag == '--app-id':
                app_id = args[i + 1]
            else:
                hue = float(args[i + 1])
            del args[i:i + 2]
    if '--prop-tile' in args:
        tile = True
        args.remove('--prop-tile')
    return app_id, (None if hue is None else hue % 360 / 360), tile, args


def main() -> None:
    global APP_HUE, PROP_COMPOSED, PROP_TILE
    app_id, hue, PROP_TILE, args = cat_options(sys.argv[1:])
    if app_id is not None:
        APP_HUE = hue if hue is not None else hue_for(app_id)
        print(f'cat mode  app id {app_id!r} → hue {round(APP_HUE * 360)}° '
              f'({"fixed" if hue is not None else "hash"})')
    if args:
        source = Path(args[0])
    elif APP_HUE is not None:
        raise SystemExit('cat mode needs the cat image path as an argument')
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
    prop_path = ICON_DIR / 'source_glyph.png'
    if APP_HUE is not None and prop_path.exists():
        glyph = cat_with_prop(glyph, Image.open(prop_path).convert('RGBA'))
        PROP_COMPOSED = True
        print(f'prop      {prop_path.relative_to(ROOT)} (lower right badge)')

    mac = macos_icon(glyph)
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    mac.save(ICON_DIR / 'app_icon_1024.png')
    mac.resize((256, 256), Image.LANCZOS).save(ICON_DIR / 'app_icon.png')
    print(f'master   {(ICON_DIR / "app_icon_1024.png").relative_to(ROOT)}, app_icon.png (256)')

    write_macos(mac)
    desktop = desktop_icon(glyph)  # Linux
    write_windows(glyph)
    write_linux(desktop)
    mobile = mobile_icon(glyph)
    write_ios(mobile)
    write_android(mobile, glyph)


if __name__ == '__main__':
    main()
