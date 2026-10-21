# Viper

## Why Viper exists

Viper is a fast, album-centric native music player for Linux with a low memory footprint. It is designed to browse and play music from local folders or Samba shares, including libraries served by Navidrome.

## Security

Saved passwords, API keys, shared secrets, and session tokens are stored in the operating system's credential store, not in SQLite. Credentials from older SQLite settings are migrated to the credential store and cleared from the database when loaded. Application preferences, account names, and library state remain in SQLite at `~/.config/viper-music/viper.sqlite3`.

## Requirements

- Rust and Cargo

## Run

```sh
cargo run --release
```

The app opens to the Home view. To select or change the music folder, choose **Library → Choose Music Folder**. The selected folder is saved and scanned automatically on later launches. Scan progress appears beside the current page title.

## Features

- Browse the Home, Albums, and Songs views. Search matches album titles and artists, and the Songs view also matches track titles, so tracks without album tags stay reachable.
- Double-click an album or song to play it; single-click selects. Right-click for **Play next**, **Add to queue**, **Go to album**, and **Open containing folder**.
- Click the now-playing artwork or title to jump to its album. Drag on the seek bar to preview a position; playback seeks when you release.
- Edit **Up Next** in a side panel (`Q`) that stays open while you browse; drag songs to reorder them.
- Home shelves show up to 20 albums; **See all** opens the album browser with the matching sort.
- Keyboard: arrows move the selection, `Enter` opens or plays it, `Space` plays or pauses, `Ctrl ←/→` seeks, `Ctrl ↑/↓` changes volume, and `Ctrl +/−/0` zooms. See **Help → Keyboard Shortcuts** for the full list.
- Navigate with the File, Library, Playback, View, and Help menus, and choose a startup page from a sidebar item's context menu.
- Errors appear as dismissible notifications in the bottom-right corner.
- Album metadata falls back to filenames and folders when tags are missing. Albums are grouped by album artist when available, while track artists remain visible.
- Disc folders such as `CD1`, `Disc 2`, and `Disk 3` use the parent folder as the album name and artwork location.
- Load cover art from neighboring image files or embedded audio artwork; artwork is reduced to a 320 px thumbnail and cached.
- Read audio metadata and embedded artwork with Lofty and Symphonia.

Preferences let you choose your music folder and startup page. Preferences and album state are stored in SQLite at `~/.config/viper-music/viper.sqlite3`. On first launch, Viper moves the existing `~/.config/viper/` directory when the new location is absent. Databases from the older `~/.config/musicplayer/` location are copied when needed.

## Build an AppImage

Build a distributable AppImage for x86_64 Linux:

```sh
scripts/build.sh appimage
```

The output is `target/appimage/Viper-Music-x86_64.AppImage`, under the same name for every
version. The first run downloads linuxdeploy into `~/.cache/viper/appimage-tools/`; subsequent
runs reuse it. Build on the oldest Linux distribution you intend to support for the best
compatibility with older systems.

## Install locally

From a checkout of this repository, install Viper for your user with:

```sh
scripts/build.sh install
```

The command builds the release binary and installs it under `~/.local` (`bin/viper`, the hicolor
icon, and `share/applications/viper.desktop`). Set `PREFIX` to install somewhere else. Cargo and
the native build/runtime dependencies must already be installed. The script reports whether the
launcher is new, up to date, or stale and rewritten, and refreshes the desktop, icon, and KDE
caches so the menu picks it up. Settings and music files are not touched.

## Samba / SMB libraries

On Linux, open **Preferences → Connections → Samba / SMB share** and enter a share URL such as
`smb://nas.local/Music/Albums`. Username, password, and workgroup are optional for guest
shares; share settings are saved in Viper's local SQLite database, while the password is stored in the
operating system's credential store. Viper uses Pavão and the
system `libsmbclient` library. Install `libsmbclient-dev` to build and `libsmbclient` to run
the application (for Debian or Ubuntu, `sudo apt install libsmbclient-dev libsmbclient`).
SMB1-only servers require insecure legacy protocol support and are not enabled by default.

## Releases

Changelog entries and release notes come from the commit history with
[git-cliff](https://git-cliff.org/), configured in `cliff.toml`. Each commit is sorted into
a group by the start of its subject line, and commits that start with `Release`, `Bump`,
`Merge`, or `Chore` are left out. Words that don't match a group go under **Other**, so write
commit subjects with a leading verb, for example `Add ...`, `Fix ...`, or `Change ...`.

The tag sets the app version, so you never bump `Cargo.toml` by hand. The version in the repo
stays at `0.1.0` as a placeholder, which means a local build reports that version.

To cut a release, run the script from a clean `main` that is up to date with `origin/main`:

```sh
scripts/release.sh 0.2.0
```

The script checks the version and the repo state, runs Clippy, and prints the release notes for
the new version. After you confirm, it regenerates `CHANGELOG.md` with a full rewrite (not
`--prepend`, because the file already has an Unreleased section), commits it as `Release v0.2.0`,
creates the annotated tag, and pushes `main` and the tag. Nothing is written until you confirm.

Pushing the tag starts the **Release AppImage** workflow (`.github/workflows/release-appimage.yml`).
Before it builds, the workflow writes the tag's version (`0.2.0`) into `Cargo.toml`. It then
builds the AppImage on Ubuntu 22.04, runs `git cliff --latest` for the release notes, and creates
a GitHub release named `Viper v0.2.0` with the AppImage attached. Pushes to `main` do not build
anything.

To check the build without making a release, run the workflow by hand from the Actions tab
(**Run workflow**). It builds `Cargo.toml` as it is, so it uses the placeholder version, and
creates no release.

Don't edit `CHANGELOG.md` by hand, since the next regeneration overwrites it. Tags must start
with `v` followed by a digit, such as `v0.2.0`, because that is the pattern git-cliff and the
workflow both look for.

## Project layout

- `src/app/` creates the window (`bootstrap.rs`), owns application state (`application.rs`), runs each frame (`runtime.rs`), and handles keyboard shortcuts, scanning, and folder watching.
- `src/workbench/` lays out the shell: menu bar, sidebar, player bar, queue panel, and the active page.
- `src/features/` holds the Home, Albums, Songs, lyrics, and preferences views, plus the library model and scanner (`library/`) and Rodio playback with MPRIS controls (`playback/`).
- `src/platform/` stores settings in SQLite and credentials in the OS keyring (`persistence/`), and talks to Discord and Last.fm (`presence/`).
- `src/shared/` provides themes, reusable controls, notifications, and artwork loading and caching.
