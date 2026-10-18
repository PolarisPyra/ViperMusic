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

- Browse the Home and Albums views, search the album library, and open album track lists.
- Play an album or start from a selected track; use previous, play/pause, next, stop, seek, and volume controls.
- Navigate with the File, Library, Playback, and View menu dropdowns.
- Choose **Set Home/Albums as startup view** from the corresponding sidebar item's context menu.
- Album metadata falls back to filenames and folders when tags are missing. Albums are grouped by album artist when available, while track artists remain visible.
- Disc folders such as `CD1`, `Disc 2`, and `Disk 3` use the parent folder as the album name and artwork location.
- Load cover art from neighboring image files or embedded audio artwork; artwork is reduced to a 320 px thumbnail and cached.
- Read audio metadata and embedded artwork with Lofty and Symphonia.

Preferences let you choose your music folder and startup page. Preferences and album state are stored in SQLite at `~/.config/viper-music/viper.sqlite3`. On first launch, Viper moves the existing `~/.config/viper/` directory when the new location is absent. Databases from the older `~/.config/musicplayer/` location are copied when needed.

## Build an AppImage

Build a distributable AppImage for x86_64 Linux:

```sh
./packaging/build-appimage.sh
```

The output is written to `target/appimage/`. The first run downloads linuxdeploy into
`~/.cache/viper/appimage-tools/`; subsequent runs reuse it. Build on the oldest Linux
distribution you intend to support for the best compatibility with older systems.

## Install on Arch Linux

From a checkout of this repository, install Viper with:

```sh
./packaging/packenger.sh install
```

The command builds the release binary and installs Viper under `/usr/bin`, with its
application menu entry and icon in the standard hicolor theme. Cargo and the native
build/runtime dependencies must already be installed. It uses `sudo` when needed; you
can also run it as root.

To remove the installed application files later, run:

```sh
./packaging/packenger.sh uninstall
```

## Samba / SMB libraries

On Linux, open **Preferences → Connections → Samba / SMB share** and enter a share URL such as
`smb://nas.local/Music/Albums`. Username, password, and workgroup are optional for guest
shares; share settings are saved in Viper's local SQLite database, while the password is stored in the
operating system's credential store. Viper uses Pavão and the
system `libsmbclient` library. Install `libsmbclient-dev` to build and `libsmbclient` to run
the application (for Debian or Ubuntu, `sudo apt install libsmbclient-dev libsmbclient`).
SMB1-only servers require insecure legacy protocol support and are not enabled by default.

## Project layout

- `src/main.rs` starts the application; `src/app/initialization.rs` configures fonts and the native window, then creates `ViperApp`.
- `src/app/` coordinates application state and initial library loading in `library_loader.rs` and filesystem updates in `library_watcher.rs`.
- `src/views/home.rs` and `src/views/album_grid.rs` render the Home and Albums pages; album details and track rows live under `src/components/`.
- `src/library/` owns models, `sorting`, `scanner`, `modifications`, audio-file helpers, tag reading, metadata caching, and folder watching.
- `src/artwork/` scans for and prepares cover art in `scanner.rs`, and manages decoded artwork textures in `cache.rs`.
- `src/playback/player.rs` handles Rodio playback with Symphonia decoding; `src/storage/` stores settings and metadata cache records in SQLite.
