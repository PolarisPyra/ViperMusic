# CLAUDE.md

## Releases

Triggers: any request to bump the version (for example "bump to 26.2.0", "bump release version", "bump and release", "cut a release"). Releases follow the steps below; never tag by hand.

Scope: this repo only (Viper Music). A bump or release request never covers the other Viper repo (vipermixer), even if the request says "both" or "all repos"; that repo gets its own request.

1. Check that `main` is clean and up to date with `origin/main`. Pick the bump level with the rules in AGENTS.md ("Pushing and releasing"): `release` (default) or `patch`. Check it with `scripts/release.sh next [release|patch]`.
2. Run `scripts/release.sh bump [release|patch]` (or `scripts/release.sh YY.N.P` when the user names a version). Don't edit `Cargo.toml` first; after confirmation the script writes the version with `scripts/set-version.sh`, updates `Cargo.lock`, regenerates `CHANGELOG.md` with git-cliff, commits `Release vYY.N.P`, creates the annotated tag, and pushes `main` and the tag. Show the release notes it prints before confirming.
3. Afterwards check `gh run list` and `gh release view vYY.N.P`, and report what was built.

Notes:
- `git-cliff` lives in `~/.cargo/bin`, which `~/.zshenv` puts on PATH for `/usr/bin/zsh`. The agent's Bash tool runs `/usr/bin/zsh` but its environment does not include that PATH, so `command -v git-cliff` fails there. Prefix commands in that tool with `export PATH="$HOME/.cargo/bin:$PATH"`.
- Pushing a tag starts the Release AppImage workflow. Never move or force-push an existing tag. If the tag already exists, stop and ask.
- Only one Release AppImage run should exist per tag. If an older run for the same tag is still going, cancel it with `gh run cancel <id>` before the newer run creates the release.
- Do not edit `CHANGELOG.md` by hand; it is generated from commit subjects, so write subjects as "Add ...", "Fix ...", or "Change ...".
