# Releasing

Everything about shipping Viper Music: versions, the changelog, the release script, and the
GitHub Actions workflows.

## Overview

```
CI workflow (GitHub Actions)            every push to main and every pull request: Clippy
        │ passed, on main
        ▼
Release workflow                        after each push to main that passed
  ├─ plan         does main carry a version with no tag yet, from a merged release pull request?
  ├─ pr           no: notes, version, CHANGELOG.md as one commit on the release branch → "Release v26.11.0" pull request
  │                 (you merge it with a merge commit; that push starts this workflow again)
  ├─ cut          yes: tag v26.11.0 on the release commit, push the tag
  │                 (or: scripts/release.sh bump on your machine, which pushes a tag and starts the step below)
  └─ appimage     build the AppImage, create the GitHub release with the tag's notes and the AppImage
        │
        ▼
Installed copies                        check the latest GitHub release every hour and update in place
```

| File | What it does |
| --- | --- |
| `scripts/release.sh` | Cuts a release, in CI or on your machine, and publishes it in CI |
| `scripts/set-version.sh` | Writes a version into `Cargo.toml` and `Cargo.lock` |
| `scripts/changelog.sh` | Checks commit messages, writes each release's notes from them, and reads and writes `CHANGELOG.md` |
| `.github/workflows/ci.yml` | Checks every push to `main` and every pull request, and runs by hand |
| `.github/workflows/release.yml` | Keeps the release pull request up to date, tags it once merged, builds the AppImage, and publishes the release |
| `.github/actions/setup` | The runner setup both workflows share: Linux packages and Rust |
| `docs/changelog-style.md` | The rules for changelog entries |

`scripts/release.sh --help` lists its commands.

## Versions

Versions are CalVer, `YY.M.N`: the two-digit calendar year, the month, and which release of that
month it is, counting from 0. Nobody works the next one out by hand: the Release workflow and
`scripts/release.sh bump` count from the newest tag and the UTC calendar.

- The next release in the same month counts `N` up: `26.11.4` → `26.11.5`.
- The first release in a new month starts at 0: `26.11.5` → `26.12.0`, and `26.12.3` → `27.1.0`.
- There are no levels to pick: a fix and a feature both take the next number.

`scripts/release.sh next` prints the version a bump gives and changes nothing. Each part is compared
as a whole number, so `26.11.10` sorts above `26.11.9`, and the updater compares tags with
`semver`, so every version has all three parts. The older versions, `0.x` and the one `YY.N.P`
release `26.0.1`, sort below every `26.11.x`, so copies on them update as usual.

The repo's version always matches the latest tag. A release writes it into `Cargo.toml` and
`Cargo.lock` with `scripts/set-version.sh`; to fix a mismatch, run
`scripts/release.sh version 26.11.0`.

## Changelog

`CHANGELOG.md` is for users, and each release's section is the text of its GitHub release. The
rules for entries are in [changelog-style.md](changelog-style.md).

**1. At commit.** Every commit that changes the app has a paragraph for users in its message,
starting with its group, or `Changelog: none` when users can't notice the change:

```
Redraw the seek bar only when it moves

Changelog: Changed: The seek bar now redraws only when it moves, so Viper Music uses less CPU
while a song plays.
```

The `commit-msg` hook in `scripts/hooks/` turns down a commit that changes the app without one, one
without a group, and one that breaks the style rules. A commit that only touches `.github/`,
`scripts/`, `docs/`, or `*.md` is internal and needs none. Turn the hook on once per clone:

```sh
git config core.hooksPath scripts/hooks
```

**2. At release.** `scripts/release.sh` writes the release's section from the `Changelog:`
paragraphs of the app commits since the last tag, under their groups, each entry once. A release
with only internal changes says so in one line. A tag whose build failed never reached users; the
next release merges its section into its own, so the notes cover everything since the last version
users have.

On your machine, the notes wait in `.release-notes/<tag>.md` (ignored by git) until the release is
pushed. To change them before releasing, edit the file and run the release again.

**Fixing notes after a release.** Edit that release's section of `CHANGELOG.md`, commit it, and run
`scripts/release.sh notes 26.11.0` to publish it to the GitHub release.

## Cut a release

**With the release pull request.** Every push to `main` runs the CI workflow. When it passes, the
Release workflow looks at the commits since the last tag. If any has a `Changelog:` paragraph other
than `none`, it (`scripts/release.sh ci-pr`):

1. Writes and checks the release notes, writes the next version, adds the notes to the top of
   `CHANGELOG.md`, and commits them as `Release v26.11.0` on top of the checked commit.
2. Force-pushes that one commit to the `release` branch, and opens a **Release v26.11.0** pull
   request into `main`, or updates its title and notes.

A push with only internal commits, or `Changelog: none`, changes nothing, and when nothing for
users is waiting the pull request is closed.

**To release**, merge the pull request with **Create a merge commit**, or run
`gh pr merge release --merge`. Squash and rebase would also release commits pushed after the pull
request was last updated, without their notes. The merge is a push to `main`, so CI runs on it, and
when it passes the Release workflow sees `main` carries a version with no tag yet (`ci-plan`) and:

1. Tags the commit that set the version and pushes the tag (`scripts/release.sh ci-tag`).
2. Builds the AppImage and creates the GitHub release `Viper Music v26.11.0` with the notes and
   `Viper-Music-x86_64.AppImage` (`scripts/release.sh ci-release`).
3. Opens a new release pull request for anything pushed before the merge that the release left out.

The repo has to let the workflow open pull requests (Settings → Actions → General → **Allow GitHub
Actions to create and approve pull requests**), and should allow only merge commits (Settings →
General → Pull Requests).

**From your machine**, on a clean `main` up to date with `origin/main`, a release skips the pull
request:

```sh
scripts/release.sh bump        # the next version, 26.11.4 -> 26.11.5
scripts/release.sh 26.11.7     # exactly this version
```

It needs `git`, `cargo`, and `gh` (signed in). The script checks the version and the repo, runs
`cargo clippy --all-targets`, pushes commits not on `origin` yet, writes the notes and the version,
commits `Release v26.11.0`, creates the annotated tag, and pushes `main` and the tag. It asks no
questions, prints the link to the Release workflow's run, and ends. To follow the build, run
`scripts/release.sh watch 26.11.0`.

To check the build without a release, run the Release workflow by hand from the Actions tab
(**Run workflow**). It builds the AppImage as a workflow artifact and publishes nothing.

## Rules

- Releases add their notes to `CHANGELOG.md`; edit it only to fix published notes, then run
  `scripts/release.sh notes <version>`.
- Don't change version numbers by hand.
- Tags start with `v` followed by a digit, such as `v26.11.0`.
- Never reuse, move, or delete a pushed tag. A broken release is followed by the next one.
