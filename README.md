<a href="https://github.com/PolarisPyra/ViperMusic/releases/latest"><img align="right" src="https://img.shields.io/github/v/release/PolarisPyra/ViperMusic?label=version" alt="Latest version"></a>

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

Discord Rich Presence needs the app's Discord application ID at build time. Put it in a `.env` file at the repository root, which git ignores, or set it in the environment:

```sh
DISCORD_APPLICATION_ID=<application id>
```

Release builds get it from the `DISCORD_APPLICATION_ID` repository secret. Builds without it work, but leave Rich Presence out.

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

The command builds the AppImage and installs it under `~/.local` (`bin/viper`, the hicolor icon,
and `share/applications/viper.desktop`). Installed this way, or run as a downloaded release
AppImage, Viper Music checks GitHub for a new release every hour, installs it in place, and offers
a restart. Set `PREFIX` to install somewhere else. Cargo and
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

Releases follow [docs/release.md](docs/release.md). In short:

- Versions are CalVer, `YY.M.N`: the two-digit year, the month, and which release of that month it
  is, from 0 (`26.11.4` → `26.11.5`, then `26.12.0` in a new month). `scripts/release.sh next`
  prints the next one; never set a version by hand.
- Every commit that changes the app ends its message with a `Changelog:` paragraph for users, such
  as `Changelog: Fixed: …`, or `Changelog: none`. The rules are in
  [docs/changelog-style.md](docs/changelog-style.md). Turn on the hook that checks it once per
  clone with `git config core.hooksPath scripts/hooks`.
- After each push to `main` that passes CI, the Release workflow keeps a **Release vYY.M.N** pull
  request up to date with the next version and its notes. Merging it with a merge commit
  (`gh pr merge release --merge`) tags the release, builds the AppImage, and creates the GitHub
  release with the notes from `CHANGELOG.md`.
- To release right away from a clean, up-to-date `main`, run `scripts/release.sh bump`.

## Project layout

- `src/app/` creates the window (`bootstrap.rs`), owns application state (`application.rs`), runs each frame (`runtime.rs`), and handles keyboard shortcuts, scanning, and folder watching.
- `src/workbench/` lays out the shell: menu bar, sidebar, player bar, queue panel, and the active page.
- `src/features/` holds the Home, Albums, Songs, lyrics, and preferences views, plus the library model and scanner (`library/`) and Rodio playback with MPRIS controls (`playback/`).
- `src/platform/` stores settings in SQLite and credentials in the OS keyring (`persistence/`), and talks to Discord and Last.fm (`presence/`).
- `src/shared/` provides themes, reusable controls, notifications, and artwork loading and caching.
