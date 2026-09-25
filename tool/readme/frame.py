#!/usr/bin/env python3
"""Frame raw window screenshots for the README (conventions/readme-guide.md §4).

    python3 tool/readme/frame.py docs/screenshots/raw/home.png [more.png …]
    python3 tool/readme/frame.py --split raw/home-light.png raw/home-dark.png -o home.png

Every output is 1200px wide (the README shows it at 360 or 720, so it stays
sharp on Retina) with rounded corners and a soft shadow on a transparent
margin, so the same file reads well on GitHub's light and dark themes.

--split puts the light and dark captures of the same screen side by side,
cut on a diagonal — one image that shows both themes (conventions/theming.md §6).
Capture both at the same window size.

Outputs go to docs/screenshots/<name>.png. Needs Pillow.

From jejezz/application-release-templates common/ @ conventions-v1.
"""
from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'docs/screenshots'

WIDTH = 1200          # content width
RADIUS = 20           # macOS window corner at this scale
MARGIN = 48           # transparent space for the shadow
SHADOW_OFFSET = 14
SHADOW_BLUR = 22
SHADOW_ALPHA = 110


def rounded(img: Image.Image, radius: int) -> Image.Image:
    mask = Image.new('L', img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, img.width - 1, img.height - 1), radius, fill=255)
    # Keep any transparency the capture already has (macOS window corners).
    mask = ImageChops.multiply(mask, img.getchannel('A'))
    out = img.copy()
    out.putalpha(mask)
    return out


def scaled(img: Image.Image) -> Image.Image:
    img = img.convert('RGBA')
    height = round(img.height * WIDTH / img.width)
    return img.resize((WIDTH, height), Image.LANCZOS)


def framed(content: Image.Image) -> Image.Image:
    w, h = content.size
    canvas = Image.new('RGBA', (w + MARGIN * 2, h + MARGIN * 2), (0, 0, 0, 0))
    shadow = Image.new('RGBA', canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (MARGIN, MARGIN + SHADOW_OFFSET, MARGIN + w, MARGIN + h + SHADOW_OFFSET),
        RADIUS, fill=(0, 0, 0, SHADOW_ALPHA))
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(SHADOW_BLUR)))
    canvas.alpha_composite(content, (MARGIN, MARGIN))
    return canvas


def split(light: Image.Image, dark: Image.Image) -> Image.Image:
    if light.size != dark.size:
        raise SystemExit(f'--split needs same-size captures: {light.size} vs {dark.size}')
    w, h = light.size
    # Diagonal from 60% along the top to 40% along the bottom.
    mask = Image.new('L', (w, h), 0)
    ImageDraw.Draw(mask).polygon([(round(w * 0.6), 0), (w, 0), (w, h), (round(w * 0.4), h)], fill=255)
    out = light.copy()
    out.paste(dark, (0, 0), mask)
    # Keep the corners of both captures transparent.
    out.putalpha(ImageChops.multiply(light.getchannel('A'), dark.getchannel('A')))
    return out


def save(img: Image.Image, name: str) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / name
    img.save(path, optimize=True)
    print(f'wrote {path.relative_to(ROOT)} {img.size[0]}x{img.size[1]} ({path.stat().st_size // 1024} KB)')


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('images', nargs='+', type=Path)
    ap.add_argument('--split', action='store_true', help='two images: light then dark')
    ap.add_argument('-o', '--output', help='output file name (with --split)')
    args = ap.parse_args()

    if args.split:
        if len(args.images) != 2:
            raise SystemExit('--split takes exactly two images: light, dark')
        light, dark = (scaled(Image.open(p)) for p in args.images)
        name = args.output or args.images[0].name.replace('-light', '')
        save(framed(rounded(split(light, dark), RADIUS)), name)
        return

    for p in args.images:
        save(framed(rounded(scaled(Image.open(p)), RADIUS)), p.name)


if __name__ == '__main__':
    main()
