#!/usr/bin/env python3
"""Check README.md / README.ko.md against conventions/readme-guide.md.

    python3 tool/readme/check_readme.py                   # release: everything
    python3 tool/readme/check_readme.py --stage bootstrap # before features exist

--stage bootstrap checks only the structure a README can have before the app
does anything: both files, the H1, the language switch, the badges, and the
Install / Development / License sections. Features, "How it works", the demo
GIF and screenshots are written once there is something to show, so TODOs and
missing images are not reported. The release workflow uses it for pre-release
tags (vX.Y.Z-rc.N), the full check for real releases.

Errors (exit 1):
  - a {{TODO…}} / {{PLACEHOLDER}} left from the template
  - a local link or image that doesn't exist
  - an <img> without alt text
  - a required section, the language switch, or the release/license badges missing
  - the H1 isn't the app's display name
  - an image over the size limit (PNG/JPG 1.5 MB, GIF 8 MB)

Warnings:
  - a "coming soon" placeholder image from init_readme.py not yet replaced
    by a real capture (fine for the first releases, not for long)
  - a GIF over 5 MB (slow first paint on GitHub)
  - a versioned file name like Foo-1.2.3.dmg (goes stale — write <version>
    and link releases/latest instead)

From jejezz/application-release-templates common/ @ conventions-v1.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# allwinner-phoenix: the Flutter app lives in gui/ (next to the Rust crate),
# the README at the repository root.
APP = ROOT / 'gui' if (ROOT / 'gui/pubspec.yaml').exists() else ROOT

REQUIRED = {
    'README.md': ['Features', 'Install', 'Development', 'License'],
    'README.ko.md': ['기능', '설치', '개발', '라이선스'],
}
# Sections that describe what the app does — not required before it does anything.
CONTENT_SECTIONS = {'Features', '기능'}
SWITCH = {'README.md': 'README.ko.md', 'README.ko.md': 'README.md'}
MAX_BYTES = {'.png': 1_500_000, '.jpg': 1_500_000, '.jpeg': 1_500_000, '.gif': 8_000_000}
GIF_WARN = 5_000_000
PLACEHOLDER_TAG = 'readme-placeholder'


def is_placeholder(path: Path) -> bool:
    try:
        from PIL import Image
        with Image.open(path) as img:
            return PLACEHOLDER_TAG in img.info or img.info.get('comment', b'') == PLACEHOLDER_TAG.encode()
    except Exception:
        return False


def display_name() -> str | None:
    for path, pattern in (('lib/app_identity.dart', r"displayName\s*=\s*'([^']+)'"),
                          ('macos/Runner/Configs/AppInfo.xcconfig', r'^PRODUCT_NAME\s*=\s*(.+)$')):
        p = APP / path
        if p.exists():
            m = re.search(pattern, p.read_text(encoding='utf-8'), re.M)
            if m:
                return m.group(1).strip()
    return None


def check(name: str, errors: list[str], warnings: list[str], bootstrap: bool = False) -> None:
    path = ROOT / name
    if not path.exists():
        errors.append(f'{name}: missing (python3 tool/readme/init_readme.py)')
        return
    text = path.read_text(encoding='utf-8')
    body = re.sub(r'<!--.*?-->', '', text, flags=re.S)  # ignore HTML comments

    for m in re.finditer(r'\{\{[^}]*\}\}', body):
        line = body.count('\n', 0, m.start()) + 1
        # Before features exist, content TODOs are expected; names/URLs aren't.
        if bootstrap and m.group(0).startswith('{{TODO'):
            continue
        errors.append(f'{name}:{line}: template placeholder left: {m.group(0)[:60]}')

    h1 = re.search(r'<h1[^>]*>(.*?)</h1>|^# (.+)$', body, re.M)
    expected = display_name()
    if not h1:
        errors.append(f'{name}: no H1 title')
    elif expected and (h1.group(1) or h1.group(2)).strip() != expected:
        errors.append(f'{name}: H1 is "{(h1.group(1) or h1.group(2)).strip()}", expected display name "{expected}"')

    headings = re.findall(r'^## (.+)$', body, re.M)
    for section in REQUIRED[name]:
        if bootstrap and section in CONTENT_SECTIONS:
            continue
        if section not in [h.strip() for h in headings]:
            errors.append(f'{name}: missing section "## {section}"')

    if f'href="{SWITCH[name]}"' not in body and f']({SWITCH[name]})' not in body:
        errors.append(f'{name}: no language switch link to {SWITCH[name]}')

    if 'releases/latest' not in body:
        errors.append(f'{name}: no link to releases/latest (badge or Install)')
    if 'img.shields.io/github/license' not in body:
        errors.append(f'{name}: no license badge')

    # Before features exist, links inside a TODO are part of the instructions
    # ("link docs/ …"), not the README.
    scan = re.sub(r'\{\{TODO[^}]*\}\}', '', body) if bootstrap else body
    targets = re.findall(r'\]\(([^)\s]+)\)', scan) + re.findall(r'(?:src|href)="([^"]+)"', scan)
    for target in targets:
        if re.match(r'(https?:|mailto:|#)', target):
            continue
        local = ROOT / target.split('#')[0]
        if bootstrap and target.startswith('docs/screenshots/'):
            continue  # captured once there is something to show
        if not local.exists():
            errors.append(f'{name}: broken link {target}')
            continue
        if local.suffix.lower() in MAX_BYTES and is_placeholder(local):
            warnings.append(f'{name}: {target} is still a placeholder — capture it with tool/readme/capture.sh')
        limit = MAX_BYTES.get(local.suffix.lower())
        size = local.stat().st_size
        if limit and size > limit:
            errors.append(f'{name}: {target} is {size / 1e6:.1f} MB (limit {limit / 1e6:.1f} MB)')
        elif local.suffix.lower() == '.gif' and size > GIF_WARN:
            warnings.append(f'{name}: {target} is {size / 1e6:.1f} MB — aim for ≤ 5 MB')

    for tag in re.findall(r'<img\b[^>]*>', body):
        alt = re.search(r'alt="([^"]*)"', tag)
        if not alt or not alt.group(1).strip():
            errors.append(f'{name}: <img> without alt text: {tag[:70]}')
    for m in re.finditer(r'!\[([^\]]*)\]\(', body):
        if not m.group(1).strip():
            errors.append(f'{name}: markdown image without alt text')

    for m in re.finditer(r'\b[\w.-]+-v?\d+\.\d+\.\d+[\w.-]*\.(dmg|exe|zip|tar\.gz|msi|AppImage|deb|apk)\b', body):
        warnings.append(f'{name}: versioned file name "{m.group(0)}" — write <version> instead')


def main() -> None:
    bootstrap = '--stage' in sys.argv and sys.argv[sys.argv.index('--stage') + 1:][:1] == ['bootstrap']
    errors: list[str] = []
    warnings: list[str] = []
    for name in REQUIRED:
        check(name, errors, warnings, bootstrap)
    for w in warnings:
        print(f'warning  {w}')
    for e in errors:
        print(f'error    {e}')
    if errors:
        sys.exit(1)
    stage = ' — structure only (bootstrap)' if bootstrap else ''
    print(f'ok       README.md, README.ko.md{stage} ({len(warnings)} warnings)')


if __name__ == '__main__':
    main()
