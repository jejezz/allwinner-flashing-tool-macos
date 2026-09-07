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
cat > "$notes" <<EOF
Apple Silicon (arm64) 전용입니다.

### 설치

\`Allwinner-Flasher-$tag-macos-arm64.zip\`을 내려받아 압축을 풀고
\`/Applications\`로 옮긴 뒤, **첫 실행 전에 한 번** 격리 속성을 제거하세요.

\`\`\`bash
xattr -dr com.apple.quarantine "/Applications/Allwinner Flasher.app"
\`\`\`

이 앱은 Apple Developer ID로 서명·공증되지 않았습니다(ad-hoc 서명).
그래서 내려받은 상태로는 Gatekeeper가 실행을 막습니다. 위 명령은 그
표시를 지웁니다. 공증된 빌드가 필요하면 Apple Developer Program이
있어야 합니다.

CLI만 쓰려면 \`aw-tool-$tag-macos-arm64.tar.gz\`를 받으세요. libusb가
정적 링크되어 있어 별도 설치가 필요 없습니다.

### 무결성

\`SHA256SUMS\`로 확인할 수 있습니다.
EOF

echo "==> creating the GitHub release"
args=(--title "$tag" --notes-file "$notes")
[[ "$publish" == "--publish" ]] || args+=(--draft)

git -C "$root" tag -a "$tag" -m "$tag" 2>/dev/null || echo "    tag $tag already exists"
git -C "$root" push origin "$tag"

gh release create "$tag" "${args[@]}" \
    "$dist/Allwinner-Flasher-$tag-macos-arm64.zip" \
    "$dist/aw-tool-$tag-macos-arm64.tar.gz" \
    "$dist/SHA256SUMS"

echo
if [[ "$publish" == "--publish" ]]; then
    echo "완료: $tag 공개됨"
else
    echo "완료: $tag 초안으로 생성됨 — 확인 후 gh release edit $tag --draft=false"
fi
