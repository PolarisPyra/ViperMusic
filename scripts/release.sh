#!/usr/bin/env bash
# Cuts a release: checks the repo and the build, writes the new version into Cargo.toml,
# regenerates CHANGELOG.md with git-cliff, commits both, tags the version, and pushes. Pushing the
# tag starts the Release AppImage workflow, which builds the AppImage and creates the GitHub release.
#
# Versions are CalVer, YY.N.P: the two-digit year, the release number within that year, and the
# patch. `bump` counts the release number up and starts the patch at 0 (26.1.3 -> 26.2.0), and a new
# year starts at 0.1 (26.3.0 -> 27.0.1); `bump patch` fixes the last release (26.2.0 -> 26.2.1).
#
# Usage:
#   scripts/release.sh bump [release|patch]   Release the next version, counted from the newest tag.
#   scripts/release.sh YY.N.P                 Release this version.
#   scripts/release.sh next [release|patch]   Print the version bump gives, and change nothing.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

die() {
  echo "$*" >&2
  exit 1
}

usage="Usage: scripts/release.sh bump [release|patch] | YY.N.P | next [release|patch]"

# The year is two digits, which tells a CalVer version apart from the old MAJOR.MINOR.PATCH scheme
# (`0.6.3` has a one-digit first part).
is_release_version() {
  [[ "$1" =~ ^[0-9]{2}\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]
}

# The version the given bump gives the newest tag. Until the first CalVer release exists, the newest
# tag is from the old scheme, so both levels give this year's first release.
next_version() {
  local level="${1:-release}" current year count patch
  case "$level" in
    release | patch) ;;
    *) die "Not a bump level: $level (expected release or patch)." ;;
  esac
  current=$(git tag -l 'v[0-9]*' --sort=-v:refname | head -n 1)
  [[ -n "$current" ]] || die "This repo has no release tag to count from."
  current="${current#v}"
  year=$(date +%y)
  if ! is_release_version "$current"; then
    echo "$year.0.1"
    return
  fi
  IFS=. read -r _ count patch <<< "$current"
  case "$level" in
    release)
      if ((10#${current%%.*} > 10#$year)); then
        die "The newest release, $current, is from a later year than this machine's clock ($year)."
      elif ((10#${current%%.*} < 10#$year)); then
        echo "$year.0.1"
      else
        echo "$year.$((count + 1)).0"
      fi
      ;;
    patch) echo "${current%.*}.$((patch + 1))" ;;
  esac
}

case "${1:-}" in
  next)
    next_version "${2:-release}"
    exit 0
    ;;
  bump) version=$(next_version "${2:-release}") ;;
  "") die "$usage" ;;
  *)
    version="${1#v}"
    is_release_version "$version" || die "Not a version: $version (expected YY.N.P, for example $(date +%y).0.1). $usage"
    ;;
esac
tag="v$version"

for tool in git git-cliff cargo; do
  command -v "$tool" > /dev/null || die "$tool is required."
done

if [[ "$(git branch --show-current)" != main ]]; then
  die "Release from main."
fi
if [[ -n "$(git status --porcelain)" ]]; then
  die "The working tree has uncommitted changes. Commit them before releasing."
fi

git fetch --quiet --tags origin
if git rev-parse --quiet --verify "refs/tags/$tag" > /dev/null; then
  die "$tag already exists."
fi
if ! git merge-base --is-ancestor origin/main HEAD; then
  die "main is behind or has diverged from origin/main. Pull or rebase first."
fi

echo "Checking the crate..."
cargo clippy --all-targets

echo
echo "Release notes for $tag:"
git cliff --config cliff.toml --unreleased --tag "$tag"
echo

read -r -p "Set the version to $version, commit it with CHANGELOG.md, tag $tag, and push main and the tag to origin? [y/N] " answer
if [[ "$answer" != y && "$answer" != Y ]]; then
  die "Release cancelled. Nothing was changed."
fi

scripts/set-version.sh "$version"
# Writes the crate's new version into Cargo.lock.
cargo update --workspace --quiet
git cliff --config cliff.toml --tag "$tag" --output CHANGELOG.md
git add CHANGELOG.md Cargo.toml Cargo.lock
git commit -m "Release $tag"
git tag -a "$tag" -m "Viper Music $version"
git push origin main "$tag"

echo "Pushed $tag. The Release AppImage workflow is building the AppImage and will create the"
echo "GitHub release. Follow it in the Actions tab."
