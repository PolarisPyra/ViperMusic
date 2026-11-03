#!/usr/bin/env bash
# Writes one version into Cargo.toml and Cargo.lock, which the app and the AppImage use.
# scripts/release.sh runs it when it commits a release, and the Release workflow runs it again from
# the tag before building.
#
# Usage: scripts/set-version.sh 26.11.0
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${1:-}"
version="${version#v}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Usage: scripts/set-version.sh YY.M.N (for example 26.11.0)" >&2
  exit 1
fi

# Perl rather than sed: it replaces only the package's own version (the first match), and in
# Cargo.lock only the viper package's, so no cargo or network is needed.
perl -0pi -e "s/^version = .*/version = \"$version\"/m" Cargo.toml
perl -0pi -e "s/^(name = \"viper\"\nversion = )\".*\"/\${1}\"$version\"/m" Cargo.lock

echo "Set the version to $version."
