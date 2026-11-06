# State machines

Every explicit and implicit state machine in Viper Music, with its states, invariants, and
transitions, then the invalid states the code could reach and what became of each. Update the
tables when a transition changes, and move a finding to **Fixed** with its test when it is fixed.

References are `file:line` as of this document's last update; function names are given so they
can be found again after the lines move.

## Threads and wakes

The UI thread owns `ViperApp`. Work that has to keep going while the window is hidden or
minimized runs on its own thread:

| Thread | Owns | Talks to the UI by |
| --- | --- | --- |
| `playback-driver` | Nothing; holds a `Weak` to `Arc<Mutex<Playback>>` (`player.rs:153`) | The shared mutex |
| `viper-mpris` | Its own `MprisLibrary` copy of the tracks | The shared mutex, a 20 ms poll |
| `lastfm-scrobbler` | `Scrobbler` | The shared mutex, an event channel |
| `discord-presence` | Discord client state | The shared mutex |
| `music-library-scan`, `music-library-watch-scan`, artwork, `viper-cd-*`, lyrics, updater | One job each | A channel, plus `Repaint` to wake the UI |
| `instance::listen`, tray | Nothing | `TrayEvent` channel plus `Repaint` |

The UI thread runs `ViperApp::logic` only when its poll loop wakes (`runtime.rs:149`). The loop
waits on `RepaintReceiver::wait` (`shared/repaint.rs`), which wakes on `request_repaint`, or once
the earliest `request_repaint_after` deadline passes. Only the earliest deadline is kept, so every
`logic` run asks again for the wakes it still needs.

**Invariant W1.** A job that sends a result the UI polls for must call `request_repaint` after
sending, or the UI must already be asking for a delayed wake while the job runs. Otherwise the
result waits until something else wakes the app.

## 1. App lifecycle

### Window

Encoded by `ViperApp.window: Option<AnyWindowHandle>`, `tray: Option<Tray>`, and
`workbench.quit_requested`.

| State | Encoding |
| --- | --- |
| Open | `window = Some`, one GPUI window |
| Hidden | `window = None`, tray present, app and its threads still running |
| Reopening | `window = None` between `reopen_window` or `show_window` and the deferred `open_window` |
| Quitting | `quit_requested = true`, `cx.quit()` called |

Invariants:

- **L1.** Exactly one GPUI window exists while Open, and `window` is its handle.
- **L2.** The `ViperApp` entity lives until Quitting. The window's `Root` holds it only while the
  window is open, so the poll loop holds it too (`runtime.rs:149`, `start_polling`).
- **L3.** The poll loop runs from startup to Quitting, whether or not a window is open.

| From | Event | To | Where |
| --- | --- | --- | --- |
| (start) | `bootstrap::run` | Open | `bootstrap.rs:67`, `open_window` |
| Open | Window close, tray present | Hidden | `window_should_close` (`runtime.rs:95`) |
| Open | Window close, no tray, or `quit_requested` | Quitting | `window_should_close` |
| Hidden | `TrayEvent::Show` (tray, or a second launch) | Reopening, then Open | `show_window` (`runtime.rs:106`) |
| Open, not focused | `TrayEvent::Show` | Reopening, then Open (Wayland ignores focus requests) | `show_window`, `reopen_window` (`runtime.rs:119`) |
| Open | Logo change settles | Reopening, then Open | `logic`, `reopen_window` |
| Hidden | Display connection gone (logout) | Quitting | `logic` (`runtime.rs:325`) |
| Any | Tray Quit, File → Quit, restart to update | Quitting | `logic`, `quit` |

Repeated `Show` events in one drain are merged (`merge_shows`, `runtime.rs:37`), so a burst of
launches can't open more than one window.

### Instance

`platform/instance.rs`. The first copy is Primary and listens on
`$XDG_RUNTIME_DIR/viper-music.sock`. A later launch is Secondary: it connects, which makes the
primary send `TrayEvent::Show` and a repaint, and exits. If the primary doesn't answer within
3 s, the secondary takes over.

### Restart to update

`restart_requested` → `logic` → `Updater::restart()` sets the path to exec → `quit` → `main.rs`
execs it. Restart wins over a window reopen requested in the same tick.

## 2. Settings persistence

`platform/persistence/settings.rs`. Settings live in memory in `ViperApp.settings` and are written
by `save` (preferences) and `save_album_state` (per-album rows).

| State | Encoding |
| --- | --- |
| Loaded | `read_only = false`, saves write the database |
| Unreadable | `Settings::unreadable()` (`settings.rs:455`): defaults with `read_only = true`, saves do nothing |

- **S1.** Settings that were never read are never written over the stored ones
  (`settings.rs:590`, `settings.rs:672`).
- Volume, window size, and anything else not saved when it changed are written only by `quit`.

## 3. Library

### Full scan

`features/library/background.rs`, `state.rs`, `app/library_loader.rs`.

| From | Event | To |
| --- | --- | --- |
| Idle | `start_scan` (startup, folder chosen, SMB connected) | Scanning: `scan_receiver = Some`, `scanning = true`, playback stopped |
| Scanning | Result received in `poll_scan` | Idle with the new library; artwork scan starts |
| Scanning | Worker gone | Idle with an error toast |
| Scanning | `start_scan` again | Scanning; the old scan is cancelled and its result can't arrive |

While Scanning, `logic` asks for a wake every 100 ms. The scan thread doesn't wake the app itself,
so the first wake comes from `start_polling`.

### Artwork scan

`artwork_receiver = Some` while reading; each poll drains decoded covers and asks for a wake
80 ms later. It ends when the worker disconnects.

### Folder watcher

`background.rs:89`, `poll_file_watcher`. Encoded by `file_watcher`, `watcher_root`,
`pending_watch_paths`, `pending_removed_paths`, `watch_debounce_until`, `watch_scan_receiver`.

| From | Event | To |
| --- | --- | --- |
| Off | Local music folder set | Watching (`file_watcher = Some`, `watcher_root = folder`) |
| Off | Watcher can't start | Failed for this folder (`watcher_root = folder`, `file_watcher = None`), one toast |
| Watching | File changes | Debouncing (800 ms after the last change) |
| Debouncing | Debounce over, no scan running | Updating (`watch_scan_receiver = Some`) |
| Updating | Result | Watching; library updated, playback remapped |
| Watching | Watcher error | Off, then Watching again on the next poll (`background.rs:139`) |
| Any | Folder cleared or SMB share | Off |

- **F1.** Tracks the watcher adds get the same title and album fallbacks as a full scan
  (`scanner.rs:424`).
- **F2.** After a watcher update, every stored track index is remapped or dropped:
  `Playback::remap_tracks` (`player.rs:824`) and `last_observed_track` (`library_watcher.rs:21`).

### Artwork cache slot, selection, tag editor, lyrics

- **Artwork cache slot** (`shared/artwork/artwork_cache.rs`): absent → Loading → Ready or
  Failed. Ready slots are evicted least recently used first. `clear` bumps `texture_epoch`, and
  `poll` drops a decode from an older epoch, or one for a slot that is no longer Loading.
- **Selection:** `selected_album`, `selected_track`, and `highlighted_album` are indexes into the
  library. A watcher update clears `selected_album` when that album empties.
- **Tag editor job:** Idle → Saving → Idle. The result is matched to its `TrackDraft.index`.
- **Lyrics:** Per track key: Missing → Loading (one request at a time) → Cached `Ok` or `Err`.

## 4. Playback

`features/playback/player.rs`. Everything below is guarded by the one `Playback` mutex.

### Transport

| State | Encoding |
| --- | --- |
| Stopped | `current`, `player`, `queue_input`, `current_completion`, `current_position_ns`, `current_started` all `None` |
| Playing | All `Some`, player not paused |
| Paused | All `Some`, `Player::is_paused` |

- **P1.** The six transport fields are all `Some` or all `None`. `start_track_at` sets them
  together and `stop_current` (`player.rs:992`) clears them together.
- **P2.** Every queued, history, and context index is below `tracks.len()`.

| From | Event | To | Where |
| --- | --- | --- | --- |
| Any | `play`, `play_album`, `play_in_context`, `play_random_track` | Playing (or Stopped with an error) | `prepare_selection` (`player.rs:798`), then `start_track` |
| Playing | Track ends, nothing scheduled | Playing the next queued track, or Stopped | `advance_if_finished` (`player.rs:402`) |
| Playing | Track ends, gapless track scheduled | Playing it, with no gap | `advance_if_finished` |
| Playing | Track ends, Repeat One | Playing the same track | `advance_if_finished` |
| Playing ↔ Paused | `toggle_pause`, MPRIS, tray | | `toggle_pause` |
| Any | `skip_next`, `previous` | Playing another track | |
| Any | `stop`, scan start, current track removed by the watcher | Stopped, queues and history cleared | `stop`, `remap_tracks` |
| Any | Track can't open, or no audio output | Stopped, manual and automatic queues cleared, error set | `fail_to_start` (`player.rs:914`) |

### Gapless pipeline

Encoded by `gapless_preparation`, `scheduled_track`, `current_started`. It is only active while
gapless is on and repeat isn't One (`tick`, `player.rs:188`).

| From | Event | To |
| --- | --- | --- |
| Idle | Tick, current track started, not paused, something queued | Preparing (decoder opening on a worker) |
| Preparing | Decoder ready for the track still at the queue front | Scheduled (appended to rodio's queue) |
| Preparing | Result for a track no longer at the front | Idle, then prepares the new front |
| Preparing | Decoder fails | Idle; that queue entry is dropped and the error shown |
| Preparing, Scheduled | Queue edit, shuffle, repeat change, remap, gapless off | Idle (`clear_scheduled_pending`, `player.rs:555`) |
| Scheduled | Audio thread reaches it | Promoted-unobserved: audible, `current` not updated yet |
| Promoted-unobserved | Next driver tick | The scheduled track becomes `current` |

`clear_scheduled_pending` keeps a track the audio thread has already promoted, because clearing
rodio's queue returns 0 then.

### Queues, repeat, shuffle

- Queues: `return_queue` (from Previous), `manual_queue` (Play Next, Add to Queue), `auto_queue`
  (album or context order). They are played in that order. `history` is capped at 100.
- `RepeatMode`: Off, All, One. `toggle_repeat_all` goes Off ↔ All and One → All; a double click
  sets One; MPRIS sets any mode.
- **P3.** Repeat All refills `auto_queue` from `context_tracks` only once it is empty and a track
  has to start. Gapless never refills it ahead of time, so turning repeat off during the last
  track still stops there.
- **P4.** Starting an album queues every one of its tracks; with shuffle on, any of them can play
  first (`queue_context_from_start`, `player.rs:363`).

## 5. Player bar

`workbench/shell/player_bar.rs`.

| Machine | States | Notes |
| --- | --- | --- |
| Seek drag | `seek_preview: None` or `Some((track, fraction))` | Set while dragging (`player_bar.rs:756`), applied on release by `finish_drags` (`player_bar.rs:333`). It applies only to the track the drag began on (`seek_to`, `player_bar.rs:346`). |
| Seek clock | `anchor: None` (not playing) or `Some((position, instant))` | Eases toward the decoder's position, and jumps when the gap exceeds 300 ms |
| Volume drag | `volume_dragging: bool` | Saves settings on release |

## 6. Integrations

### MPRIS

`features/playback/mpris.rs`. Status is derived from `Playback` on each 20 ms poll and diffed
against `previous_snapshot` (`mpris.rs:140`). Commands lock the playback mutex and act on it
directly, using MPRIS's own copy of the tracks.

### Last.fm sign-in

`platform/presence/lastfm.rs`, `SignIn`:

| From | Event | To |
| --- | --- | --- |
| Any | Connect | RequestingToken |
| RequestingToken | Token received | AwaitingApproval(token), browser opened |
| AwaitingApproval | Finish | Exchanging |
| Exchanging | Session received | Idle, connected |
| Any | Disconnect | Idle; late replies are dropped |

Replies that arrive in a state that doesn't expect them are dropped.

### Last.fm scrobbler

Per track, identified by its file, not its index (`update_playback`, `lastfm.rs:234`):
New → NowPlaying sent → Scrobbled (at half the track or 4 minutes, if longer than 30 s). It goes
back to New when the file changes, or when the position jumps from the last 2 s to the first 2 s
(`playback_restarted`, `lastfm.rs:301`).

### Discord presence

Publishes when `(track index, title, artist, album, paused, options)` changes, and refreshes every
60 s. If Discord isn't running, it retries after a delay.

## 7. CD ripper

`features/ripper/state.rs`. `Phase` (`state.rs:40`) owns everything valid in that phase.

| From | Event | To |
| --- | --- | --- |
| Waiting | A disc is present | Reading (`read_disc`, `state.rs:288`) |
| Reading | TOC and lookup finish | Ready(Session), or Failed |
| Ready | Rip | Ripping |
| Ripping | Cancel | Cancelling (`cancel`, `state.rs:377`) |
| Ripping | Finished | Done |
| Cancelling | Finished | Ready |
| Done | Retry failed tracks | Ripping |
| Done | Edit tags and rip again | Ready |
| Done | Next disc | Waiting |
| Done, Failed | A new disc goes in | Reading |
| Ready, Failed | The disc comes out | Waiting |
| Failed | Try again | Reading |

- **R1.** A handler called in a phase it doesn't belong to leaves the phase as it is.
- **R2.** The read thread wakes the app when it finishes (`state.rs:288`), as W1 requires.
- `Probe`: Idle{due} → Running → Idle{due + 1.5 s}. It runs only in Waiting, Ready, Done and
  Failed, and only while the Rip page is shown (`watch_drives`, `state.rs:178`).
- `Lookup` (Searching, Matched, NotFound, Failed) and `Cover` (None, Loading, Ready, Unavailable)
  are polled only in Ready.

## 8. Updater

`platform/updates.rs`, `UpdateState`: Idle → Checking → UpToDate, Available, or Failed;
Available → Downloading → Ready, or Failed. Ready is final until restart. `busy` allows one check
or download at a time.

## Invalid states the code could reach

Ranked by impact. Each was confirmed by tracing the code; the lifecycle issues were also
reproduced in a sandboxed copy of the app.

### Fixed

**Issue 1: The app stopped working once its window closed.**

| # | Invalid state | Evidence and scenario | Fix | Test |
| --- | --- | --- | --- | --- |
| 1.1 | Hidden with no app (breaks L2, L3) | Closing the window from the taskbar dropped the window's `Root`, the only strong handle to `ViperApp`. `bootstrap` didn't keep one, and the poll loop held a weak handle and stopped. The playback, MPRIS, Discord, and Last.fm threads ended, so the music stopped. The process kept running, so a later launch handed off to it and nothing opened until the tray's Quit. Reproduced: after a taskbar close, all four threads were gone and a second launch opened no window. | The poll loop holds the app (`start_polling`). | Sandbox: close from the taskbar while playing; the position kept advancing and a second launch reopened the window. GPUI windows can't run under `cargo test`. |
| 1.2 | Poll loop never woken (breaks L3, W1) | Nothing requested the first wake, and the scan thread doesn't wake the app. Without a tray (for example GNOME without the AppIndicator extension), the library never finished loading and MPRIS Play did nothing. Reproduced: zero UI-thread wakes and an empty library with no tray. | `start_polling` requests the first wake. | Sandbox: with no tray, the library loads and MPRIS Play plays. |
| 1.3 | Two windows, one untracked (breaks L1) | Two `Show` events in one drain, such as a double-clicked launcher: each one deferred `open_window`. Closing the untracked window then set `window = None` while the other stayed open and stopped redrawing. | `merge_shows` | `app::runtime::tests::two_launches_at_once_open_one_window` |

Minimizing the window doesn't pause playback. In the sandbox, the position advanced at the normal
rate while the window was minimized. The pause users saw came from 1.1.

**Issue 2: A failed settings read was later saved over the real settings.**

| # | Invalid state | Evidence and scenario | Fix | Test |
| --- | --- | --- | --- | --- |
| 2.1 | Defaults treated as loaded (breaks S1) | `with_settings_result` used `Settings::default()` when `Settings::load` failed (a locked database, or a column a newer version wrote). The next `save` or `save_album_state`, or `quit`, wrote the defaults over the stored row and album state. Music folder, ratings, favorites, and play counts were gone. | `Settings::unreadable()`, whose saves do nothing, and a toast saying changes won't be saved. | `platform::persistence::settings::tests::only_settings_that_could_not_be_read_are_read_only`. The no-op save itself isn't exercised, because a broken guard would write to the developer's real settings. |

**Issue 3: Library watcher bookkeeping.**

| # | Invalid state | Evidence and scenario | Fix | Test |
| --- | --- | --- | --- | --- |
| 3.1 | Watcher-added tracks without fallbacks (breaks F1) | `scan_added_tracks` used `scan_track` without `fill_missing_display_metadata`, unlike the full scan. Untagged files copied in while the app ran showed with no title and no album until the next full scan. | Use `read_track` | `features::library::scanner::tests::untagged_files_the_watcher_finds_are_named_like_a_full_scan` |
| 3.2 | Watcher off for good | After a watcher error, `file_watcher = None` but `watcher_root` stayed set, so `poll_file_watcher` never made a new one. | Clear `watcher_root` on error | None: needs a watcher that fails on demand. |
| 3.3 | Stale indexes after a remap (breaks F2) | Deleting a file earlier in the library than the playing track renumbered it. `last_observed_track` wasn't remapped, so the album's play count went up again. The scrobbler compared indexes, so it re-sent Now Playing and scrobbled the track a second time. | Remap `last_observed_track`; the scrobbler compares files | `platform::presence::lastfm::tests::a_renumbered_track_is_still_the_same_play` |

**Issue 4: Playback queue.**

| # | Invalid state | Evidence and scenario | Fix | Test |
| --- | --- | --- | --- | --- |
| 4.1 | Shuffled album without its first track (breaks P4) | `play_album` with shuffle on removed the first track from the queue, then started a random one of the rest. Track 1 never played until Repeat All refilled the queue. | `queue_context_from_start` | `features::playback::player::tests::shuffled_album_can_start_on_its_first_track`, `album_in_order_starts_on_its_first_track` |
| 4.2 | Queue refilled under Repeat Off (breaks P3) | With gapless and Repeat All on, `update_gapless` refilled `auto_queue` during the last track. Turning repeat off then cleared only the scheduled track, so the album started over. Repeat All now leaves a short gap where the album wraps around. | Don't refill in `update_gapless` | `features::playback::player::tests::repeat_off_during_the_last_gapless_track_stops_there` |
| 4.3 | Seek applied to the wrong track | `seek_preview` kept only a fraction. If a track ended mid-drag, releasing seeked the next track to the old position. | `seek_preview` keeps its track | None: needs a drag in a GPUI window. |

**Issue 5: CD ripper.**

| # | Invalid state | Evidence and scenario | Fix | Test |
| --- | --- | --- | --- | --- |
| 5.1 | Reading with its result unpolled (breaks R2, W1) | `viper-cd-read` sent its result without waking the app, and `watch_drives` stops asking for wakes in Reading. With no music playing, "Reading the disc" stayed up whenever the TOC and MusicBrainz lookup took longer than 1.5 s. | Wake after sending | `features::ripper::state::tests::reading_a_disc_wakes_the_app_when_it_is_done` |
| 5.2 | Phase dropped (breaks R1) | `start_rip`, `retry_failed`, `edit_again`, and `cancel` used `mem::replace(&mut phase, Waiting)` and returned when the phase didn't match. A click from a stale frame, such as Cancel just as a rip finished, threw away Done or Failed. Rip while Ripping dropped the rip without cancelling it. | Put back a phase that doesn't match | `features::ripper::state::tests::actions_for_another_phase_leave_it_alone` |

### Open

Not fixed yet, most impactful first. "Plausible" means the code path exists but the timing is
narrow or depends on outside conditions.

| # | Invalid state or transition | Evidence | Confidence |
| --- | --- | --- | --- |
| O1 | Quitting or restarting to update during a rip kills it without a warning, leaving an incomplete album and an undrained `cdparanoia`. | `quit` and the restart path never check `ripper.busy()` | Confirmed |
| O2 | Gapless prepare failures drain the queue. Each tick drops one entry, and only the last error is shown, while the non-gapless path stops instead. | `update_gapless` (`player.rs:480`, `511`, `525`) vs `fail_to_start` | Confirmed |
| O3 | Pausing between a gapless promotion and the next tick leaves `current`, the seek clamp, MPRIS, Discord, and Last.fm on the previous track until it resumes. | `tick` calls `advance_if_finished` only while playing | Confirmed; window up to 100 ms |
| O4 | MPRIS volume changes playback only, so the slider shows the old value and the change isn't saved. | `mpris.rs:243` vs `settings.volume` | Confirmed |
| O5 | Queue panel drags and removals use positions from the last frame. When the driver consumes the queue front mid-drag, they act on the neighbouring track. | `queue_panel.rs`, `consume_scheduled` | Plausible |
| O6 | A scrobble that fails is never retried. Seeking past half scrobbles a track that wasn't heard, and replaying the same track doesn't scrobble it again. | `lastfm.rs:282`, `playback_restarted` | Confirmed |
| O7 | Cancel can't interrupt a stalled read, so a scratched disc can sit in Cancelling for minutes. | Cancel is checked between blocks; the read blocks on the `cdparanoia` pipe | Confirmed |
| O8 | Last.fm Connect is never disabled. Pressing it twice, or during Exchanging, makes Finish exchange the wrong token or drop the approved session. | `start_auth` (`lastfm.rs:139`) has no state guard | Confirmed |
| O9 | A user check for updates that fails offline replaces Available, hiding a found update. A check during a background check does nothing silently. | `updates.rs` | Confirmed |
| O10 | Clicking Rip while the cover is still loading rips without it. A late "Search again" result overwrites tags the user has typed. | `state.rs` `start`, `choose` | Confirmed |
| O11 | MPRIS metadata is republished only when the track index changes, so late artwork or a tag edit isn't shown. | `mpris.rs:150` | Confirmed |
| O12 | Discord's elapsed time ignores seeks and restarts until the 60 s refresh. | `PresenceKey` has no position | Confirmed |
| O13 | MPRIS commands use their own copy of the tracks. Between a remap or scan start and `sync_mpris`, Next can start the wrong file. | `mpris.rs`, `library_watcher.rs` | Plausible; milliseconds |
| O14 | A remap leaves an already-promoted scheduled track with its old index. | `remap_tracks` (`player.rs:824`) | Plausible |
| O15 | Moving or renaming a folder is a removal then an addition, so the playing track stops and the queue is lost. | Watcher events | Confirmed |
| O16 | A watcher removal doesn't remap the tag editor's `TrackDraft.index`, and a tag edit doesn't call `sync_mpris` or `record_new_albums`. | `tag_editor/state.rs` | Confirmed |
| O17 | A failed lyrics lookup is cached for the session, so a network blip hides lyrics until restart. | `lyrics/state.rs`, `request` | Confirmed |
| O18 | `connect_smb_share` sets `music_path` before a save that can fail and return early, and doesn't close album details. | `application.rs`, `connect_smb_share` | Confirmed |
| O19 | A Last.fm settings read that fails (keyring not ready at login) shows empty credentials, and saving from Preferences then deletes the stored keys. | `load_lastfm_settings`, `save_secret` | Confirmed; needs a user save |
| O20 | Volume and window size are saved only on Quit, so a logout or the tray's forced exit loses them. A window that fails to reopen quits without saving. | `quit`, `open_window` | Confirmed |
| O21 | The driver wakes every 16 ms while paused with a gapless preparation pending. `prepare_selection` pushes the current track to history even when it is selected again. `update_playback` holds the playback mutex across disk writes. | `tick`, `prepare_selection` (`player.rs:798`), `runtime.rs` `update_playback` | Confirmed; minor |

## Checking

- `cargo test` runs the regression tests named above.
- The lifecycle fixes in Issue 1 need a desktop session. Run a copy of the app with its own
  `HOME` and `XDG_RUNTIME_DIR` inside `unshare --map-current-user -n`, so it can't reach an
  installed copy's single-instance socket. Give it a private D-Bus session bus. To exercise the
  tray paths, add a stand-in `org.kde.StatusNotifierWatcher` on that bus. Then:
  1. Start playback through MPRIS on that bus.
  2. Close the window through KWin scripting (`closeWindow()`).
  3. Check that the MPRIS position still advances.
  4. Launch again inside the same namespace, and check that the window comes back.
