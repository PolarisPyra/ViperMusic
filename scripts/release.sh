#!/usr/bin/env bash
# Everything that releases Viper Music, on your machine and in the Release workflow
# (.github/workflows/release.yml). docs/release.md explains how it all fits together.
#
# After each push to main that passes the checks, the Release workflow keeps a "Release vYY.M.N"
# pull request up to date with what users would get; merging it releases. These are for releasing
# from your machine instead.
#
# On your machine:
#   scripts/release.sh bump             Cut the next release now, with no questions asked: check,
#                                       write the notes from the commits' Changelog: paragraphs,
#                                       set the version, add the notes to CHANGELOG.md, commit,
#                                       tag, and push, then print the build's link and end. The
#                                       version is CalVer, YY.M.N, counted from the newest tag:
#                                       the next N this month (26.11.4 -> 26.11.5), or N 0 in a
#                                       new month (26.11.5 -> 26.12.0).
#   scripts/release.sh YY.M.N           The same, for exactly that version.
#   scripts/release.sh next             Print the version that bump gives, and change nothing.
#   scripts/release.sh watch YY.M.N     Follow YY.M.N's build to the end, then check that its
#                                       GitHub release has the AppImage.
#   scripts/release.sh version YY.M.N   Write a version into Cargo.toml and Cargo.lock. `next`
#                                       works here too.
#   scripts/release.sh notes YY.M.N     Publish YY.M.N's section of CHANGELOG.md to its GitHub
#                                       release again, after a fix.
#
# In the Release workflow, where $RELEASE_TAG names the tag being published:
#   scripts/release.sh ci-plan          Write the version main carries to the step's `version`
#                                       output when it has no tag yet, because a release pull
#                                       request was merged, and nothing when it has one.
#   scripts/release.sh ci-pr            When a commit since the last tag has a Changelog:
#                                       paragraph for users, write the notes, set the next version,
#                                       add the notes to CHANGELOG.md, commit that on top of the
#                                       checked commit, force-push it to the release branch, and
#                                       open or update its pull request. Close it when none has.
#   scripts/release.sh ci-tag YY.M.N    Tag the commit that set YY.M.N, push the tag, and write it
#                                       to the step's `tag` output.
#   scripts/release.sh ci-release       Create the GitHub release for $RELEASE_TAG with its section
#                                       of CHANGELOG.md and the AppImage scripts/build.sh built.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

WORKFLOW=release.yml
# The branch the Release workflow keeps the next release on, for its pull request. Only it writes
# there.
RELEASE_BRANCH=release
APPIMAGE=target/appimage/Viper-Music-x86_64.AppImage

die() {
  echo "$*" >&2
  exit 1
}
step() {
  echo
  echo "== $*"
}
usage() {
  sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'
}
# A version is CalVer, YY.M.N: the two-digit year, the month, and which release of that month it
# is, from 0. The year is two digits, which tells a CalVer version apart from the old
# MAJOR.MINOR.PATCH scheme (`0.6.3` has a one-digit first part). The one YY.N.P release, 26.0.1,
# reads as month 0 of 2026, so it still sorts below every new one.
is_release_version() {
  [[ "$1" =~ ^[0-9]{2}\.([1-9]|1[0-2])\.(0|[1-9][0-9]*)$ ]]
}

check_version() {
  is_release_version "$1" \
    || die "Not a version: $1 (expected YY.M.N, for example $(date -u +%y.%-m).0)."
}

# The newest release, from the tags. A release follows the latest tag, so the next version comes
# from there rather than from Cargo.toml, which a release writes to match it.
latest_version() {
  local tag
  tag=$(git tag -l 'v[0-9]*' --sort=-v:refname | head -n 1)
  [[ -n "$tag" ]] || die "This repo has no release tag to count from."
  echo "${tag#v}"
}

# Prints the version after the latest release: the next N in the same month, or N 0 in a new one.
# The month is UTC's, so a release cut here and one cut by the workflow count the same way.
next_version() {
  local current year month count
  current=$(latest_version)
  read -r year month <<< "$(date -u '+%y %-m')"
  # The newest tag from an old scheme has no month to count from.
  if ! is_release_version "$current"; then
    echo "$year.$month.0"
    return
  fi
  IFS=. read -r _ _ count <<< "$current"
  local was=$((10#${current%%.*} * 100 + $(cut -d. -f2 <<< "$current")))
  local now=$((10#$year * 100 + month))
  if ((was > now)); then
    die "The newest release, $current, is from a later month than this machine's clock ($year.$month)."
  elif ((was < now)); then
    echo "$year.$month.0"
  else
    echo "$year.$month.$((count + 1))"
  fi
}

# The version to release: `next`, or `bump`, counted from the latest tag, or the YY.M.N given. A
# leading `v` is accepted, as in the tags themselves.
resolve_version() {
  local version="${1:-}"
  [[ -n "$version" ]] || die "Usage: scripts/release.sh bump | YY.M.N"
  version="${version#v}"
  case "$version" in
    next | bump) next_version ;;
    *)
      check_version "$version"
      echo "$version"
      ;;
  esac
}

# The version Cargo.toml carries.
cargo_version() {
  sed -n 's/^version = "\(.*\)"$/\1/p' "${1:-Cargo.toml}" | head -n 1
}

# Writes and checks the release notes, sets the version, adds the notes to CHANGELOG.md, and
# commits, all on the current commit. Pushing is up to the caller.
commit_release() {
  local version="$1" tag="v$1"
  # Kept until the release is pushed, so an edit to it, or a second run, is used as it is.
  local notes=".release-notes/$tag.md"
  step "Release notes for $tag"
  scripts/changelog.sh draft "$tag" "$notes"
  echo
  cat "$notes"
  echo
  scripts/changelog.sh check "$notes" "$version"

  step "Committing $tag"
  scripts/set-version.sh "$version"
  scripts/changelog.sh insert "$notes" "$version"
  git add CHANGELOG.md Cargo.toml Cargo.lock
  git commit -m "Release $tag"
}

cut_release() {
  local asked="${1:-bump}" version previous
  version=$(resolve_version "$asked")
  check_version "$version"
  previous=$(latest_version)
  [[ "$version" != "$previous" ]] || die "v$version is already the latest release; nothing to cut."
  local tag="v$version"
  echo "Cutting $tag, up from v$previous."

  step "Checking the repo"
  local tool
  for tool in git cargo gh; do
    command -v "$tool" > /dev/null || die "$tool is required."
  done
  gh auth status > /dev/null 2>&1 || die "gh is not signed in. Run gh auth login."
  [[ "$(git branch --show-current)" == main ]] || die "Release from main."
  [[ -z "$(git status --porcelain)" ]] || die "The working tree has uncommitted changes. Commit them before releasing."
  git fetch --quiet --tags origin
  ! git rev-parse --quiet --verify "refs/tags/$tag" > /dev/null || die "$tag already exists."
  git merge-base --is-ancestor origin/main HEAD \
    || die "main is behind or has diverged from origin/main. Pull or rebase first."
  echo "main is clean and up to date."

  step "Checking the crate"
  # The same check as the CI workflow.
  cargo clippy --all-targets
  [[ -z "$(git status --porcelain)" ]] || die "cargo clippy changed files. Commit them, then release again."

  # Commits not on origin yet go up on their own first, so the tag's run is the only release build.
  if [[ -n "$(git rev-list origin/main..HEAD)" ]]; then
    git push origin main
  fi
  commit_release "$version"
  git tag -a "$tag" -m "Viper Music $version"
  git push origin main "$tag"
  rm -f ".release-notes/$tag.md"

  # The build runs on GitHub from here; `scripts/release.sh watch` follows it.
  local run=""
  for _ in $(seq 30); do
    run=$(gh run list --workflow "$WORKFLOW" --branch "$tag" --limit 1 --json databaseId -q '.[0].databaseId // empty')
    [[ -n "$run" ]] && break
    sleep 2
  done
  [[ -n "$run" ]] || die "No Release run started for $tag. Check the Actions tab."
  echo "Released $tag. The build: $(gh run view "$run" --json url -q .url)"
}

# Follows a release's build to the end, then checks that its GitHub release has the AppImage.
# Ctrl-C stops only the watching, not the build.
watch_release() {
  local version="${1#v}"
  check_version "$version"
  local tag="v$version" run url
  run=$(gh run list --workflow "$WORKFLOW" --branch "$tag" --limit 1 --json databaseId -q '.[0].databaseId // empty')
  [[ -n "$run" ]] || die "No Release run for $tag. Check the Actions tab."
  url=$(gh run view "$run" --json url -q .url)
  echo "$url"
  if ! gh run watch "$run" --exit-status --interval 30 > /dev/null; then
    gh run view "$run" --json jobs -q '.jobs[] | "  \(.name): \(.conclusion)"'
    gh run view "$run" --log-failed 2> /dev/null | grep '##\[error\]' | head -n 10 || true
    die "The Release workflow failed: $url"
  fi
  gh run view "$run" --json jobs -q '.jobs[] | "  \(.name): \(.conclusion)"'

  step "Checking the GitHub release $tag"
  gh release view "$tag" --json assets -q '.assets[].name' | grep -q '\.AppImage$' \
    || die "The GitHub release $tag has no AppImage."
  echo "Installed copies will update to $version. Download:"
  gh release view "$tag" --json assets -q '.assets[] | "  \(.url)"'
}

# A release run's first step, after a push to main passed the checks. When main carries a version
# with no tag yet, a release pull request was merged, and this run tags and publishes it; otherwise
# it updates the release pull request.
ci_plan() {
  local version
  version=$(cargo_version)
  if git rev-parse --quiet --verify "refs/tags/v$version" > /dev/null; then
    echo "v$version is released already, so this run updates the release pull request."
    version=""
  else
    echo "main carries v$version, which has no tag yet, so this run releases it."
  fi
  echo "version=$version" >> "${GITHUB_OUTPUT:-/dev/stdout}"
}

# Puts the next release on the release branch, one commit on top of the checked commit, and opens
# or updates its pull request, so it always shows what merging would release. A commit pushed while
# it is open joins it once its own checks pass. Merged with a merge commit, one that had not yet
# keeps its notes for the next release.
ci_pr() {
  local pr version body
  pr=$(gh pr list --head "$RELEASE_BRANCH" --base main --state open --json number -q '.[0].number // empty')
  if ! scripts/changelog.sh pending; then
    echo "No commit since $(latest_version) has notes for users, so there is nothing to release."
    if [[ -n "$pr" ]]; then
      gh pr close "$pr" --delete-branch --comment "Nothing for users is waiting any more."
    fi
    return
  fi
  version=$(next_version)
  commit_release "$version"
  git push --force origin "HEAD:refs/heads/$RELEASE_BRANCH"
  body="$(cat ".release-notes/v$version.md")

---

Merging this releases v$version: these notes and the version, on top of $(git rev-parse --short HEAD^), which passed the checks. Merge it with **Create a merge commit**, so commits pushed since keep their notes for the next release."
  if [[ -n "$pr" ]]; then
    gh pr edit "$pr" --title "Release v$version" --body "$body"
  else
    gh pr create --base main --head "$RELEASE_BRANCH" --title "Release v$version" --body "$body"
  fi
}

# Tags the commit that set VERSION: the release pull request's own commit when it was merged with
# a merge commit, else the one the merge made. A tag pushed with the workflow's token starts no
# second run, so this run builds it.
ci_tag() {
  local version="${1#v}" tag commit set
  check_version "$version"
  tag="v$version"
  ! git rev-parse --quiet --verify "refs/tags/$tag" > /dev/null || die "$tag already exists."
  commit=$(git log -1 --format=%H -G '^version = ' "v$(latest_version)..HEAD" -- Cargo.toml)
  [[ -n "$commit" ]] || die "No commit since v$(latest_version) changes the version."
  set=$(git show "$commit:Cargo.toml" | cargo_version /dev/stdin)
  [[ "$set" == "$version" ]] || die "The newest commit that changes the version sets $set, not $version."
  git tag -a "$tag" -m "Viper Music $version" "$commit"
  git push origin "refs/tags/$tag"
  echo "Tagged $tag on $(git log -1 --format='%h %s' "$commit")."
  echo "tag=$tag" >> "${GITHUB_OUTPUT:-/dev/stdout}"
}

# Creates the GitHub release with the tag's section of CHANGELOG.md, which release.sh committed
# before tagging, and the AppImage. The AppImage keeps the same name in every release, so GitHub's
# /releases/latest/download/Viper-Music-x86_64.AppImage always points at the newest one.
ci_release() {
  local tag="${RELEASE_TAG:?Run ci-release from the Release workflow.}"
  [[ -f "$APPIMAGE" ]] || die "Expected $APPIMAGE after the build."
  scripts/changelog.sh section "$tag" > RELEASE_NOTES.md
  [[ -s RELEASE_NOTES.md ]] || echo "CHANGELOG.md has no section for $tag, so the release has no notes." >&2
  gh release create "$tag" --title "Viper Music $tag" --notes-file RELEASE_NOTES.md "$APPIMAGE"
}

# Publishes a release's notes again from CHANGELOG.md, for a fix made after the release.
republish_notes() {
  local version="${1#v}"
  check_version "$version"
  local tag="v$version"
  local notes=".release-notes/$tag.published.md"
  mkdir -p .release-notes
  scripts/changelog.sh section "$tag" > "$notes"
  [[ -s "$notes" ]] || die "CHANGELOG.md has no section for $version."
  scripts/changelog.sh check "$notes" "$version"
  gh release edit "$tag" --notes-file "$notes" > /dev/null
  echo "Updated the notes of the GitHub release $tag."
  rm -f "$notes"
}

case "${1:-}" in
  bump) cut_release bump ;;
  next) next_version ;;
  version)
    requested=$(resolve_version "${2:-}") || exit 1
    scripts/set-version.sh "$requested"
    ;;
  notes) republish_notes "${2:-}" ;;
  watch) watch_release "${2:-}" ;;
  ci-plan) ci_plan ;;
  ci-pr) ci_pr ;;
  ci-tag) ci_tag "${2:-}" ;;
  ci-release) ci_release ;;
  -h | --help) usage ;;
  "") usage >&2 && exit 1 ;;
  # A YY.M.N.
  *) cut_release "$1" ;;
esac
