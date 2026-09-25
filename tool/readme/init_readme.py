#!/usr/bin/env python3
"""Write README.md and README.ko.md from the templates (conventions/readme-guide.md).

    python3 tool/readme/init_readme.py            # refuses to overwrite
    python3 tool/readme/init_readme.py --force    # replace existing READMEs
    python3 tool/readme/init_readme.py --placeholders   # only (re)create missing images

flutter create's own README.md is replaced without --force.

Fills in everything the repository already knows:

  {{DISPLAY_NAME}}    lib/app_identity.dart displayName, else AppInfo.xcconfig PRODUCT_NAME
  {{FILE_NAME}}       display name without spaces (conventions/identity.md §2)
  {{REPO_SLUG}}       owner/repo from `git remote get-url origin`
  {{PLATFORM_*}}      from the platform folders that exist
  {{MIN_MACOS}}       macos/Podfile `platform :osx`, else the Xcode project's MACOSX_DEPLOYMENT_TARGET
  {{RUN_DEVICE}}      macos / windows / linux / a phone
  {{YEAR}}            AppIdentity.firstReleaseYear, else this year

and keeps only the <!-- if:… --> blocks for those platforms. What only you
know is left as {{TODO: …}} — tool/readme/check_readme.py fails until each
is filled in.

From jejezz/application-release-templates common/ @ conventions-v1.
"""
from __future__ import annotations

import datetime
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# allwinner-phoenix: the Flutter app lives in gui/ (next to the Rust crate),
# the README at the repository root.
APP = ROOT / 'gui' if (ROOT / 'gui/pubspec.yaml').exists() else ROOT
HERE = Path(__file__).resolve().parent
TARGETS = {'README.template.md': 'README.md', 'README.ko.template.md': 'README.ko.md'}


def read(path: str) -> str:
    p = APP / path
    return p.read_text(encoding='utf-8') if p.exists() else ''


def display_name() -> str:
    m = re.search(r"displayName\s*=\s*'([^']+)'", read('lib/app_identity.dart'))
    if m and not m.group(1).startswith('__'):
        return m.group(1)
    m = re.search(r'^PRODUCT_NAME\s*=\s*(.+)$', read('macos/Runner/Configs/AppInfo.xcconfig'), re.M)
    if m:
        return m.group(1).strip()
    raise SystemExit('display name not found: fill lib/app_identity.dart or AppInfo.xcconfig PRODUCT_NAME first')


def repo_slug() -> str:
    # The git remote, else AppIdentity.repositoryUrl (filled before the
    # GitHub repository exists).
    try:
        url = subprocess.run(['git', 'remote', 'get-url', 'origin'], cwd=ROOT,
                             capture_output=True, text=True, check=True).stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        url = ''
    m = (re.search(r'github\.com[:/](.+?)(?:\.git)?$', url)
         or re.search(r"repositoryUrl\s*=\s*'https://github\.com/([^'/]+/[^'/]+)'", read('lib/app_identity.dart')))
    return m.group(1) if m and '__' not in m.group(1) else '{{TODO: owner/repo}}'


def platforms() -> list[str]:
    return [p for p in ('macos', 'windows', 'linux', 'ios', 'android') if (APP / p).is_dir()]


def min_macos() -> str:
    # The Podfile only exists once a plugin needed CocoaPods; the Xcode
    # project always has the deployment target.
    m = (re.search(r"platform\s+:osx,\s*'([\d.]+)'", read('macos/Podfile'))
         or re.search(r'MACOSX_DEPLOYMENT_TARGET = ([\d.]+);', read('macos/Runner.xcodeproj/project.pbxproj')))
    return m.group(1) if m else '{{TODO: minimum macOS}}'


def first_year() -> str:
    m = re.search(r'firstReleaseYear\s*=\s*(\d{4})', read('lib/app_identity.dart'))
    return m.group(1) if m else str(datetime.date.today().year)


def keep_blocks(text: str, enabled: set[str]) -> str:
    """Resolve <!-- if:x --> … <!-- endif:x --> (nested, one per line)."""
    out, stack = [], []
    for line in text.splitlines(keepends=True):
        opening = re.fullmatch(r'\s*<!-- if:(\w+) -->\s*', line)
        closing = re.fullmatch(r'\s*<!-- endif(?::\w+)? -->\s*', line)
        if opening:
            stack.append(opening.group(1) in enabled)
        elif closing:
            stack.pop()
        elif all(stack):
            out.append(line)
    return re.sub(r'\n{3,}', '\n\n', ''.join(out))


PLACEHOLDER_TAG = 'readme-placeholder'


def placeholder_media(name: str) -> None:
    """Write "coming soon" images for every docs/screenshots/* the README
    references but that don't exist yet, so the README renders and the release
    check passes before the first capture. They carry a marker that
    check_readme.py reports as a warning until they're replaced."""
    try:
        from PIL import Image, ImageDraw, ImageFont, PngImagePlugin
    except ImportError:
        print('note     Pillow missing — no placeholder images (pip3 install pillow)')
        return
    icon_path = APP / 'assets/icon/app_icon.png'
    for readme in TARGETS.values():
        text = (ROOT / readme).read_text(encoding='utf-8') if (ROOT / readme).exists() else ''
        for rel in sorted(set(re.findall(r'src="(docs/screenshots/[^"{}]+)"', text))):
            out = ROOT / rel
            if out.exists():
                continue
            out.parent.mkdir(parents=True, exist_ok=True)
            w, h = (760, 480) if out.suffix == '.gif' else (1200, 760)
            img = Image.new('RGB', (w, h), (0x15, 0x1D, 0x27))
            draw = ImageDraw.Draw(img)
            size = 160 if out.suffix != '.gif' else 120
            if icon_path.exists():
                icon = Image.open(icon_path).convert('RGBA').resize((size, size))
                img.paste(icon, ((w - size) // 2, h // 2 - size + 10), icon)
            try:
                font = ImageFont.load_default(size=40 if out.suffix != '.gif' else 30)
            except TypeError:  # Pillow < 10.1
                font = ImageFont.load_default()
            for i, (line, color) in enumerate(((name, (0xF1, 0xF5, 0xF9)), ('Screenshot coming soon', (0xA9, 0xB4, 0xC4)))):
                box = draw.textbbox((0, 0), line, font=font)
                draw.text(((w - box[2]) // 2, h // 2 + 40 + i * (box[3] + 16)), line, fill=color, font=font)
            if out.suffix == '.gif':
                img.save(out, comment=PLACEHOLDER_TAG.encode())
            else:
                info = PngImagePlugin.PngInfo()
                info.add_text(PLACEHOLDER_TAG, '1')
                img.save(out, pnginfo=info)
            print(f'wrote    {rel} (placeholder — replace with tool/readme/capture.sh)')


def is_flutter_default(path: Path) -> bool:
    return path.exists() and 'A new Flutter project.' in path.read_text(encoding='utf-8')


def main() -> None:
    force = '--force' in sys.argv[1:]
    if '--placeholders' in sys.argv[1:]:
        # After renaming screenshots in the README: make placeholders for
        # whatever it now references that doesn't exist yet.
        placeholder_media(display_name())
        return
    plats = platforms()
    if not plats:
        raise SystemExit('no platform folders found — run from a Flutter app')
    desktop = [p for p in plats if p in ('macos', 'windows', 'linux')]
    mobile = [p for p in plats if p in ('ios', 'android')]
    enabled = set(plats) | ({'desktop'} if desktop else set()) | ({'mobile'} if mobile else set())

    names = {'macos': 'macOS', 'windows': 'Windows', 'linux': 'Linux', 'ios': 'iOS', 'android': 'Android'}
    name = display_name()
    values = {
        'DISPLAY_NAME': name,
        'FILE_NAME': re.sub(r'[^0-9A-Za-z]', '', name),
        'REPO_SLUG': repo_slug(),
        'PLATFORM_TEXT': ' · '.join(names[p] for p in plats),
        # shields.io: `-` separates fields, so spaces become %20 and the
        # separator is a URL-encoded middle dot.
        'PLATFORM_BADGE': '%20%C2%B7%20'.join(names[p] for p in plats),
        'MIN_MACOS': min_macos() if 'macos' in plats else '',
        'RUN_DEVICE': desktop[0] if desktop else '<device-id>',
        'YEAR': first_year(),
    }

    for template, target in TARGETS.items():
        out = ROOT / target
        # flutter create's README is replaced without asking.
        if out.exists() and not force and not is_flutter_default(out):
            print(f'skip     {target} (exists — use --force to replace)')
            continue
        text = keep_blocks((HERE / template).read_text(encoding='utf-8'), enabled)
        for key, value in values.items():
            text = text.replace('{{' + key + '}}', value)
        out.write_text(text, encoding='utf-8')
        todos = len(re.findall(r'\{\{TODO', text))
        print(f'wrote    {target} ({todos} TODOs to fill)')
    placeholder_media(name)


if __name__ == '__main__':
    main()
