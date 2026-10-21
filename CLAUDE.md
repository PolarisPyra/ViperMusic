# CLAUDE.md

## Releases

Triggers: any request to bump the version (for example "bump to 0.5.2", "bump release version", "bump and release", "cut a release"). Releases follow the steps below; never tag by hand.

Scope: this repo only (Viper Music). A bump or release request never covers the other Viper repo (vipermixer), even if the request says "both" or "all repos"; that repo gets its own request.

1. Check that `main` is clean and up to date with `origin/main`.
2. Set `version` in `Cargo.toml` to the new version, then run `cargo build --release` so `Cargo.lock` updates. Commit as `Bump version to X.Y.Z` (the `Bump` prefix keeps it out of the changelog) and push `main`.
3. Run `scripts/release.sh X.Y.Z`. It regenerates `CHANGELOG.md` with git-cliff, commits `Release vX.Y.Z`, creates the annotated tag, and pushes `main` and the tag. Show the release notes it prints before confirming.
4. Afterwards check `gh run list` and `gh release view vX.Y.Z`, and report what was built.

Notes:
- `git-cliff` lives in `~/.cargo/bin`, which this shell does not load. Prefix commands with `export PATH="$HOME/.cargo/bin:$PATH"`.
- Pushing a tag starts the Release AppImage workflow. Never move or force-push an existing tag. If the tag already exists, stop and ask.
- Only one Release AppImage run should exist per tag. If an older run for the same tag is still going, cancel it with `gh run cancel <id>` before the newer run creates the release.
- Do not edit `CHANGELOG.md` by hand; it is generated from commit subjects, so write subjects as "Add ...", "Fix ...", or "Change ...".
