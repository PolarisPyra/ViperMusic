# Changelog

All notable changes to Viper Music are documented here, for the people who use it.
Each release adds its notes at the top; see docs/release.md.

## [26.11.5] - 2026-11-05

### Fixed

- Songs you play while the window is minimized are now scrobbled to Last.fm and show as now playing there.

## [26.11.4] - 2026-11-05

### Fixed

- Discord now keeps up with the song you're playing while the window is minimized, instead of staying stuck on the song that was playing when you minimized it.

## [26.11.3] - 2026-11-04

### Added

- You can now edit an album's tags from its right-click menu, by hand or by looking the album up on MusicBrainz and picking the matching release.
- You can hide pages from the sidebar by right-clicking them, or choose which pages it shows in Preferences → View; hidden pages stay in the View menu. The sidebar's right-click menu now matches the album right-click menu.
- Album covers in the album grid and tracks in an album's track list now light up when you hover over them.

### Changed

- Discord Rich Presence works without setting up your own Discord application, and Preferences → Services lets you choose what it shows: the song, artist, album, time left, and whether it stays up while paused.

## [26.11.2] - 2026-11-04

### Fixed

- Choosing Quit from the system tray menu now quits Viper Music even while its window is minimized.

## [26.11.1] - 2026-11-04

### Added

- Rip CD now finds and rips from CD drives on Windows and macOS, not only Linux, without needing cdparanoia.

## [26.11.0] - 2026-11-03

### Added

- A new Rip CD page in the sidebar copies an audio CD into your library as FLAC files, named and tagged with the album, tracks and cover from MusicBrainz, which you can edit before ripping. A bar under Rip CD in the sidebar shows how far a rip has got while you use the rest of Viper Music.

### Fixed

- With gapless playback on, turning on shuffle or repeat or adding songs to the queue while nothing is playing no longer starts playback on its own, and a song that can't be opened stops playback instead of being tried again and again.
- Clicking Disconnect for Last.fm in Preferences → Connections while sign-in is still finishing no longer signs you back in a moment later.
- Quitting from the tray or the File menu no longer sometimes reopens the window instead, which made Viper Music quit the next time you only closed the window to the tray.

## [26.0.1] - 2026-10-31

### Changed

- Change version numbers to CalVer YY.N.P
- Change the seek bar to redraw only when it moves, using less CPU

### Fixed

- Fix playback stopping after a song while the window is minimized

## [0.6.3] - 2026-10-25

### Changed

- Change the version chip to a rounded rectangle

## [0.6.2] - 2026-10-25

### Changed

- Change the update card to ease in more slowly

### Fixed

- Fix Preferences saying up to date before the first update check

## [0.6.1] - 2026-10-25

### Changed

- Change the About page to drop the links and the data notes

## [0.6.0] - 2026-10-25

### Added

- Add automatic updates from GitHub releases

### Changed

- Change the local install to the self-updating AppImage

## [0.5.9] - 2026-10-24

### Fixed

- Fix the taskbar and title bar icon not following the chosen logo

## [0.5.8] - 2026-10-24

### Added

- Add a choice of cassette logo color ways

### Fixed

- Fix stale icon sizes lingering after install

## [0.5.7] - 2026-10-24

### Changed

- Change the app logo to the green and pink cassette

### Fixed

- Fix the first launch after login doing nothing when Viper Music was hidden to the tray

## [0.5.6] - 2026-10-24

### Changed

- Change the app logo to the new cassette design

## [0.5.5] - 2026-10-24

### Added

- Add a version chip to the menu bar

## [0.5.4] - 2026-10-24

### Changed

- Change releases to set the version from the release script

### Fixed

- Fix reopening Viper from the app menu while it runs in the tray

## [0.5.3] - 2026-10-24

### Changed

- Change the app logo to the simplified petal icon

## [0.5.2] - 2026-10-24

### Added

- Add CLAUDE.md with the release procedure

### Changed

- Change the CLAUDE.md PATH note for the Bash tool
- Change the app logo to the new design

## [0.5.1] - 2026-10-21

### Added

- Add Play and Pause tray items and open the tray menu on left click

## [0.5.0] - 2026-10-21

### Added

- Add git-cliff changelog config and generated CHANGELOG.md
- Add build and release scripts

### Changed

- Rename release title to Viper Music

### Fixed

- Fix release notes output in AppImage workflow

### Other

- Generate release notes with git-cliff
- Document how to cut a release with git-cliff
- Set release version from the tag
- Close to the system tray and upgrade eframe to 0.36

## [0.1.0] - 2026-10-20

### Added

- Add album sorting controls
- Add Discord Rich Presence and rebrand as Viper
- Add paginated album list view
- Add Samba share support and refine connection settings
- Add AppImage packaging workflow
- Add Arch install and uninstall scripts
- Add compact sidebar and Catppuccin themes with shared styling
- Add Rosé Pine themes
- Add automatic lyrics panel
- Add playback modes and queue management
- Add keyboard shortcuts and queue access
- Add Rust PipeWire MIDI mixer
- Add random track control and space player buttons evenly
- Add resizable album list columns and shuffle shortcut
- Add album context menus to grid and list
- Add album queue action to context menu
- Add Last.fm scrobbling and fix repeat navigation
- Add Linux MPRIS support
- Add Songs view, notifications, keyboard navigation, and accessibility
- Add resizable song list columns and fix divider text padding
- Add GitHub workflow that builds the Linux AppImage on version tags

### Changed

- Update README and current app UI
- Improve album views and settings UI
- Polish album favorite and rating controls
- Polish preferences and playback audio details
- Refactor source into domain modules
- Use Inter as the default UI font
- Use Phosphor icons across the UI
- Refactor album views and replace FFmpeg with Symphonia
- Move album card into album grid view
- Improve scan notices and read WMA metadata
- Polish menu bar dropdowns
- Refactor app into workbench and feature modules
- Refactor feature modules and document public APIs
- Make event handling matches exhaustive
- Use typed optional album references
- Improve lyrics panel layout and resizing
- Refactor artwork handling and use logo asset
- Update application logo
- Rename Viper app to Viper Music
- Use mixer logo for Viper Music
- Update Viper Music logo
- Update Viper icon and transport bar, remove bundled mixer
- Rename Arch package manager script
- Move music config to viper-music
- Make album format badges optional
- Use static placeholders for missing album art
- Improve gapless track preparation
- Use Rodio queue for gapless playback
- Improve playback queue popup layout
- Use Rodio track position and pause state
- Update app icon to native 512px asset
- Use CD icon for Albums navigation
- Refactor for performance and module structure

### Fixed

- Fix sidebar resize behavior
- Fix icon spacing to be equal
- Fix album highlight deselection
- Stop focus shortcut from typing the F key into the search field

### Other

- First commit
- Ui update
- Refine navigation and selection highlights
- Cache album artwork and refine playback controls
- Organize modules by responsibility
- Consolidation of album view
- Update
- Clarify application startup and library scanning modules
- Emphasize album art browsing in README
- Clarify saved Samba connection details
- Speed up SMB library scanning
- Warn while keeping tracks without metadata
- Skip and report empty audio files
- Continue scans after track tag errors
- Set album pagination to 30 items
- Register Viper as an audio player
- Speed up SMB scans with metadata caching and buffered reads
- Keep sidebar resizable in compact mode
- Apply project rules to API docs and secret storage
- Store SMB passwords in SQLite again
- Refine albums toolbar layout
- Style Home action buttons
- Adjust album toolbar and grid spacing
- Widen player bar track info area
- Document Viper purpose and credential storage
- Add date to album track panel
- Persist lyrics panel visibility
- Smooth lyrics auto-follow scrolling
- Bound gapless playback preloading
- Load album artwork asynchronously
- Smooth album artwork loading
- Smooth album artwork loading transition
- Style lyrics resize separator
- Preserve album artwork when closing details
- Even album toolbar button spacing
- Enforce single music player instance
- Revert to a31b039 - remove incoming queue move changes
- Refine playback transport controls
- Swap queue and random transport buttons
- Updated icon
- Cool ass jacket labels
- Avoid playback pause when enqueueing
- Parallelize SMB metadata scanning
- Prioritize queued tracks during album playback
- Preserve active track when queueing during gapless playback
- Insert queued tracks before album continuation
- Preempt gapless album track on queue insertion
- Avoid restarting playback when queueing during gapless
- Match album menu to file dropdown styling
- Clear album selection on blank space click
- Keep album track sidebar visible without selection
- Restore double-click album playback
- Keep Discord settings in Services preferences
- Store Discord application ID in credential store
- Avoid playback restart on queue edits
- Keep album and shared UI changes on main
- Store SMB passwords in OS credential store
- Keep lossless and lossy copies of an album as separate albums
- Keep album header controls inside the side panel

### Removed

- Remove audio device requirement from README
- Remove hyphens from README description
