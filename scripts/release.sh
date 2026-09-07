#!/usr/bin/env bash
#
# Package a release and publish it to GitHub.
#
#   ./scripts/release.sh v0.1.0            # build, package, create a draft release
#   ./scripts/release.sh v0.1.0 --publish  # ...and publish it immediately
#
# Produces, under dist/:
#   Allwinner-Flasher-<tag>-macos-arm64.zip   the .app
#   aw-tool-<tag>-macos-arm64.tar.gz          the CLI on its own
#   SHA256SUMS
#
# Apple Silicon only — see ARCHS in gui/macos/Runner/Configs/Release.xcconfig.

set -euo pipefail

tag="${1:-}"
publish="${2:-}"

if [[ -z "$tag" ]]; then
    echo "usage: $0 <tag> [--publish]" >&2
    exit 2
fi
if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+ ]]; then
    echo "tag should look like v0.1.0 (got '$tag')" >&2
    exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist="$root/dist"
app="$root/gui/build/macos/Build/Products/Release/Allwinner Flasher.app"

# A release should be reproducible from what is on the branch, not from
# whatever happens to be in the working tree.
if [[ -n "$(git -C "$root" status --porcelain)" ]]; then
    echo "working tree is dirty — commit or stash first" >&2
    exit 1
fi

echo "==> building"
"$root/scripts/build-app.sh"

arch="$(lipo -archs "$app/Contents/Resources/aw-tool")"
if [[ "$arch" != "arm64" ]]; then
    echo "unexpected helper architecture: $arch" >&2
    exit 1
fi

rm -rf "$dist"
mkdir -p "$dist"

# ditto, not zip: it preserves the bundle's symlinks and extended attributes,
# and a zip(1) archive of a .app arrives with its code signature broken.
echo "==> packaging"
ditto -c -k --sequesterRsrc --keepParent \
    "$app" "$dist/Allwinner-Flasher-$tag-macos-arm64.zip"

tar -C "$root/target/release" -czf \
    "$dist/aw-tool-$tag-macos-arm64.tar.gz" aw-tool

(cd "$dist" && shasum -a 256 ./* > SHA256SUMS)

echo "==> verifying the packaged app still validates"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
ditto -x -k "$dist/Allwinner-Flasher-$tag-macos-arm64.zip" "$work"
codesign --verify --strict "$work/Allwinner Flasher.app"

notes="$dist/notes.md"
# Bilingual, English first: the repository is public, but the team reading it
# is Korean. The quarantine step leads because without it the app simply does
# not open, and macOS's own wording ("damaged", "cannot be verified") suggests
# a corrupt download rather than a missing signature.
cat > "$notes" <<EOF
**Apple Silicon (arm64) only. / Apple Silicon(arm64) 전용입니다.**

## ⚠️ Required before first launch / 첫 실행 전 필수

Unzip, move **Allwinner Flasher.app** to \`/Applications\`, then run this **once**:

\`\`\`bash
xattr -dr com.apple.quarantine "/Applications/Allwinner Flasher.app"
\`\`\`

압축을 풀어 **Allwinner Flasher.app**을 \`/Applications\`로 옮긴 뒤, 위 명령을 **한 번** 실행하세요.

### Why / 왜 필요한가

This app is ad-hoc signed — it is **not** signed or notarised with an Apple
Developer ID. macOS therefore refuses to open it after download, and the
warning it shows (about the app being damaged, or the developer not being
verified) reads like a corrupt download. It is not: the file is fine, it simply
carries no Apple-issued signature. The command above clears the quarantine flag
macOS attaches to downloaded files.

이 앱은 ad-hoc 서명이며 Apple Developer ID로 서명·공증되지 **않았습니다**.
그래서 내려받은 상태로는 macOS가 실행을 막고, "손상되었다"거나 "개발자를
확인할 수 없다"는 경고를 띄웁니다. 파일이 깨진 것이 아니라 Apple이 발급한
서명이 없을 뿐입니다. 위 명령은 macOS가 다운로드 파일에 붙이는 격리 표시를
지웁니다.

Without the command you can also allow it in **System Settings → Privacy &
Security**, after the first blocked attempt.
명령 대신 한 번 차단된 뒤 **시스템 설정 → 개인정보 보호 및 보안**에서 허용해도 됩니다.

## Downloads / 내려받기

| File | |
|---|---|
| \`Allwinner-Flasher-$tag-macos-arm64.zip\` | The app / 앱 |
| \`aw-tool-$tag-macos-arm64.tar.gz\` | CLI only — libusb is linked statically, nothing else to install / CLI 단독 — libusb가 정적 링크되어 별도 설치 불필요 |
| \`SHA256SUMS\` | Checksums / 체크섬 |

The CLI in the tarball is not quarantined the same way; if macOS blocks it, the
same \`xattr -dr com.apple.quarantine ./aw-tool\` applies.
tarball의 CLI도 막히는 경우 같은 명령을 적용하면 됩니다.

## What it does / 용도

Flashes Allwinner T507/T527 boards from macOS over the FEL/EFEX USB protocol —
a replacement for the Windows-only PhoenixSuit. Put a board in FEL mode, pick an
image, and it carries it from bootstrap through the full fusing pass to reboot.

Windows 전용인 PhoenixSuit을 대체해, Allwinner T507/T527 보드를 macOS에서
FEL/EFEX USB 프로토콜로 플래싱합니다. 보드를 FEL 모드로 연결하고 이미지를
고르면 부트스트랩부터 전체 퓨징, 재부팅까지 진행합니다.

See the [README](https://github.com/jejezz/allwinner-flashing-tool-macos#readme)
([한국어](https://github.com/jejezz/allwinner-flashing-tool-macos/blob/main/README.md))
for what is and is not verified on hardware.
EOF

echo "==> publishing to GitHub"
git -C "$root" tag -a "$tag" -m "$tag" 2>/dev/null || echo "    tag $tag already exists"
git -C "$root" push origin "$tag" 2>/dev/null || echo "    tag $tag already pushed"

assets=(
    "$dist/Allwinner-Flasher-$tag-macos-arm64.zip"
    "$dist/aw-tool-$tag-macos-arm64.tar.gz"
    "$dist/SHA256SUMS"
)

# Re-runnable: refresh an existing release rather than failing on it, so a
# rebuild (new icon, corrected notes) does not mean deleting and re-pushing a
# tag that other clones may already have.
if gh release view "$tag" >/dev/null 2>&1; then
    echo "    release $tag exists — updating notes and assets"
    gh release edit "$tag" --notes-file "$notes"
    gh release upload "$tag" "${assets[@]}" --clobber
    [[ "$publish" == "--publish" ]] && gh release edit "$tag" --draft=false
else
    args=(--title "$tag" --notes-file "$notes")
    [[ "$publish" == "--publish" ]] || args+=(--draft)
    gh release create "$tag" "${args[@]}" "${assets[@]}"
fi

echo
if [[ "$publish" == "--publish" ]]; then
    echo "완료: $tag 공개됨"
else
    echo "완료: $tag 초안으로 생성됨 — 확인 후 gh release edit $tag --draft=false"
fi
