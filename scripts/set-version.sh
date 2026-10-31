#!/usr/bin/env bash
# Writes one version into Cargo.toml, which the app and the AppImage use. scripts/release.sh runs
# it before tagging, and the Release AppImage workflow runs it again from the tag before building.
#
# Usage: scripts/set-version.sh 26.1.0
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${1:-}"
version="${version#v}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Usage: scripts/set-version.sh YY.N.P (for example 26.1.0)" >&2
  exit 1
fi

# Perl rather than sed: it replaces only the package's own version (the first match).
perl -0pi -e "s/^version = .*/version = \"$version\"/m" Cargo.toml

echo "Set the version to $version."
