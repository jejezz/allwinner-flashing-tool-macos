#!/usr/bin/env bash
#
# Bump the app version for a release (conventions/versioning.md §5).
#
#   scripts/bump-version.sh patch        1.4.2+37 -> 1.4.3+38
#   scripts/bump-version.sh minor        1.4.2+37 -> 1.5.0+38
#   scripts/bump-version.sh major        1.4.2+37 -> 2.0.0+38
#   scripts/bump-version.sh build        1.4.2+37 -> 1.4.2+38   (store resubmission)
#   scripts/bump-version.sh 1.5.0-rc.1   1.4.2+37 -> 1.5.0-rc.1+38
#
# The build number always goes up by exactly one and never resets. A
# Cargo.toml at the repository root is bumped to the same version. Commits as
# `chore(release): vX.Y.Z` but does NOT tag — tag the merge commit on main
# after the PR lands (conventions/tagging.md §2).
#
# From jejezz/application-release-templates common/ @ conventions-v1.

set -euo pipefail

usage() { sed -n '3,9p' "$0" | sed 's/^# \{0,1\}//'; exit 1; }
[ $# -eq 1 ] || usage

cd "$(git rev-parse --show-toplevel)"
# pubspec.yaml at the root, or one level down when the Flutter app lives in
# a subfolder next to other code (e.g. gui/ beside a Rust crate).
PUBSPEC=pubspec.yaml
if [ ! -f "$PUBSPEC" ]; then
  candidates=$(ls -1 */pubspec.yaml 2>/dev/null || true)
  if [ "$(printf '%s' "$candidates" | grep -c .)" = "1" ]; then
    PUBSPEC="$candidates"
  else
    echo "no pubspec.yaml at the repository root (or exactly one in a subfolder)" >&2
    exit 1
  fi
fi

# Only tracked files matter: the bump commit adds just the version files, so
# untracked local files (IDE settings like devtools_options.yaml, scratch
# notes) can't leak into it and shouldn't block a release.
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "tracked files have uncommitted changes — commit or stash first" >&2
  git status --short --untracked-files=no >&2
  exit 1
fi

CURRENT=$(grep -m1 '^version:' "$PUBSPEC" | sed -E 's/^version:[[:space:]]*//; s/[[:space:]]*$//')
if ! [[ "$CURRENT" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)(-[0-9A-Za-z.]+)?\+([0-9]+)$ ]]; then
  echo "pubspec.yaml version '$CURRENT' is not MAJOR.MINOR.PATCH[-pre]+BUILD" >&2
  exit 1
fi
MAJOR=${BASH_REMATCH[1]} MINOR=${BASH_REMATCH[2]} PATCH=${BASH_REMATCH[3]}
PRE=${BASH_REMATCH[4]} BUILD=${BASH_REMATCH[5]}

case "$1" in
  major) NAME="$((MAJOR + 1)).0.0" ;;
  minor) NAME="$MAJOR.$((MINOR + 1)).0" ;;
  # Leaving a pre-release: 1.5.0-rc.2 -> patch -> 1.5.0 (not 1.5.1).
  patch) if [ -n "$PRE" ]; then NAME="$MAJOR.$MINOR.$PATCH"; else NAME="$MAJOR.$MINOR.$((PATCH + 1))"; fi ;;
  build) NAME="$MAJOR.$MINOR.$PATCH$PRE" ;;
  *)
    [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || usage
    NAME="$1"
    ;;
esac
NEW="$NAME+$((BUILD + 1))"

perl -pi -e "s/^version:.*/version: $NEW/ if /^version:/ && !\$done++" "$PUBSPEC"
FILES=("$PUBSPEC")

if [ -f Cargo.toml ]; then
  # Only the [package] table's own version line (the first `version =`).
  perl -pi -e "s/^version\\s*=\\s*\"[^\"]*\"/version = \"$NAME\"/ if !\$done && /^version\\s*=/ && (\$done = 1)" Cargo.toml
  FILES+=(Cargo.toml)
  [ -f Cargo.lock ] && command -v cargo >/dev/null && cargo update -w --offline >/dev/null 2>&1 && FILES+=(Cargo.lock) || true
fi

git add "${FILES[@]}"
git commit -q -m "chore(release): v$NAME"

DISPLAY=$(grep -m1 "displayName = '" lib/app_identity.dart "$(dirname "$PUBSPEC")/lib/app_identity.dart" 2>/dev/null \
  | sed -E "s/.*displayName = '([^']+)'.*/\1/" | head -1 || true)
[ -n "$DISPLAY" ] || DISPLAY=$(grep -m1 '^PRODUCT_NAME' "$(dirname "$PUBSPEC")/macos/Runner/Configs/AppInfo.xcconfig" 2>/dev/null \
  | sed -E 's/^PRODUCT_NAME[[:space:]]*=[[:space:]]*//' || true)
[ -n "$DISPLAY" ] || DISPLAY="<Display Name>"

echo "$CURRENT -> $NEW  (committed: chore(release): v$NAME)"
echo
echo "Next: push, merge the PR, then tag the merge commit on main:"
echo "  git switch main && git pull"
echo "  git tag -a v$NAME -m \"$DISPLAY $NAME\" && git push origin v$NAME"
