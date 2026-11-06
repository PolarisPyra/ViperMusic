use crate::{
    app::ViperApp,
    features::{
        library::{AlbumIndex, Track},
        playback::RepeatMode,
    },
    shared::ui::{
        controls::{
            describe_button, horizontal_fraction, listen, measure, sz, tooltip, unzoomed,
            DragPreview,
        },
        icons::{icon, inherited_icon, IconName},
        theme::{self, Palette},
    },
};
use gpui::{
    div, img, prelude::*, px, relative, AnyElement, ClickEvent, Context, Div, DragMoveEvent,
    FontWeight, Role, Stateful, Window,
};
use std::time::{Duration, Instant};

/// Height of the player bar at 100% zoom.
pub(in crate::workbench) const PANEL_HEIGHT: f32 = 112.0;
const ROW_HEIGHT: f32 = 72.0;
/// Horizontal padding on each side of the bar.
const PANEL_PADDING: f32 = 24.0;
const TRACK_ART_SIZE: f32 = 68.0;
/// Width of the time labels on either side of the seek bar.
const TIME_LABEL_WIDTH: f32 = 38.0;
/// Shortest wait between repaints while playing, about 60 fps.
const MIN_PLAYING_REPAINT: Duration = Duration::from_millis(16);
/// Steps the seek knob is redrawn in, per physical pixel.
const KNOB_STEPS_PER_PIXEL: f64 = 4.0;
/// Furthest the seek clock may drift from the decoder before it jumps to it, as after a seek.
const CLOCK_RESYNC: Duration = Duration::from_millis(300);
/// Time over which the seek clock catches up with the decoder's position.
const CLOCK_CORRECTION: Duration = Duration::from_secs(1);

/// The playback position the seek bar draws. The decoder reports its position a whole audio
/// buffer at a time, so between reports the bar runs on the clock and eases towards each report.
#[derive(Default)]
pub(crate) struct SeekClock {
    /// The position drawn last, and when.
    anchor: Option<(Duration, Instant)>,
}

impl SeekClock {
    /// Advance the clock and return the position to draw.
    ///
    /// # Arguments
    /// * `reported` - The decoder's position.
    /// * `playing` - Whether playback is running; a paused clock shows `reported` as it is.
    ///
    /// # Returns
    /// The position to draw.
    fn position(&mut self, reported: Duration, playing: bool) -> Duration {
        let now = Instant::now();
        let Some((drawn, at)) = self.anchor.filter(|_| playing) else {
            self.anchor = playing.then_some((reported, now));
            return reported;
        };
        let elapsed = now.duration_since(at);
        let estimate = drawn + elapsed;
        let error = reported.as_secs_f64() - estimate.as_secs_f64();
        let position = if error.abs() > CLOCK_RESYNC.as_secs_f64() {
            reported
        } else {
            let weight = (elapsed.as_secs_f64() / CLOCK_CORRECTION.as_secs_f64()).min(1.0);
            Duration::from_secs_f64((estimate.as_secs_f64() + error * weight).max(0.0))
        };
        self.anchor = Some((position, now));
        position
    }
}

/// Marks a drag of the seek bar's knob.
struct SeekDrag;
/// Marks a drag of the volume slider.
struct VolumeDrag;

/// What the transport needs to know about playback for one frame.
struct Transport {
    current: Option<usize>,
    playing: bool,
    shuffle: bool,
    repeat: RepeatMode,
    previous_enabled: bool,
    next_enabled: bool,
    play_enabled: bool,
    position: Duration,
}

/// Draw the playback bar along the bottom of the window.
///
/// # Arguments
/// * `app` - Application holding playback and the library.
/// * `window` - Window being drawn, whose width lays out the bar.
/// * `cx` - Application entity context.
///
/// # Returns
/// The playback bar element.
pub(in crate::workbench) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    finish_drags(app, cx);
    let colors = theme::colors(cx);
    let mut transport = {
        let playback = app.playback();
        let library = &app.library;
        let current = playback.current;
        let can_start = library
            .view
            .selected_album
            .and_then(|index| library.model.albums.get(index))
            .is_some_and(|album| !album.tracks.is_empty())
            || playback.has_next()
            || (playback.shuffle_enabled && !library.model.tracks.is_empty());
        Transport {
            current,
            playing: playback.is_playing(),
            shuffle: playback.shuffle_enabled,
            repeat: playback.repeat_mode,
            previous_enabled: current.is_some() || playback.has_history(),
            next_enabled: playback.has_next(),
            play_enabled: current.is_some() || can_start,
            position: playback.position(),
        }
    };
    transport.position = app
        .ui
        .seek_clock
        .position(transport.position, transport.playing);
    let track = transport
        .current
        .and_then(|index| app.library.model.tracks.get(index))
        .cloned();
    let duration = track
        .as_ref()
        .and_then(|track| track.duration_ms)
        .map(Duration::from_millis);

    let row_width = (unzoomed(window.viewport_size().width, window) - PANEL_PADDING * 2.0).max(0.0);
    let volume_width = if row_width < 862.0 { 150.0 } else { 176.0 };
    let spacing = ((row_width * 0.5 - volume_width - 50.0 - 17.0 - 8.0) / 2.0).clamp(54.0, 68.0);
    let controls_width = spacing * 4.0 + 34.0;
    let max_info_width = ((row_width - controls_width) * 0.5 - 16.0).max(0.0);
    let info_width = (row_width * 0.4).min(440.0).min(max_info_width);
    let center = row_width * 0.5;
    let volume_left = row_width - volume_width - 90.0;

    let mut row = div()
        .relative()
        .flex_none()
        .w_full()
        .h(sz(ROW_HEIGHT))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(sz(info_width))
                .children(track_info(app, transport.current, cx)),
        );

    let shuffle_tooltip = if transport.shuffle {
        "Shuffle: On"
    } else {
        "Shuffle: Off"
    };
    row = row.child(
        place(
            transport_button(
                "player-shuffle",
                IconName::Shuffle,
                20.0,
                shuffle_tooltip,
                transport.shuffle,
                true,
                colors,
            ),
            center - spacing * 2.0,
            (34.0, 38.0),
        )
        .on_click(cx.listener(|app, _, _, cx| {
            let tracks = std::sync::Arc::clone(&app.library.model.tracks);
            app.playback().toggle_shuffle(&tracks);
            cx.notify();
        })),
    );
    row = row.child(
        place(
            transport_button(
                "player-previous",
                IconName::SkipBack,
                28.0,
                "Previous track",
                false,
                transport.previous_enabled,
                colors,
            ),
            center - spacing,
            (40.0, 44.0),
        )
        .when(transport.previous_enabled, |button| {
            button.on_click(cx.listener(|app, _, _, cx| {
                app.playback().previous(&app.library.model.tracks);
                cx.notify();
            }))
        }),
    );
    row = row.child(play_button(&transport, center, colors, cx));
    row = row.child(
        place(
            transport_button(
                "player-next",
                IconName::SkipForward,
                28.0,
                "Next track",
                false,
                transport.next_enabled,
                colors,
            ),
            center + spacing,
            (40.0, 44.0),
        )
        .when(transport.next_enabled, |button| {
            button.on_click(cx.listener(|app, _, _, cx| {
                app.playback().skip_next(&app.library.model.tracks);
                cx.notify();
            }))
        }),
    );
    let (repeat_icon, repeat_tooltip) = match transport.repeat {
        RepeatMode::Off => (IconName::Repeat, "Repeat: Off"),
        RepeatMode::All => (IconName::Repeat, "Repeat: Queue"),
        RepeatMode::One => (IconName::Repeat1, "Repeat: Track"),
    };
    row = row.child(
        place(
            transport_button(
                "player-repeat",
                repeat_icon,
                20.0,
                repeat_tooltip,
                transport.repeat != RepeatMode::Off,
                true,
                colors,
            ),
            center + spacing * 2.0,
            (34.0, 38.0),
        )
        .on_click(cx.listener(|app, event: &ClickEvent, _, cx| {
            let mut playback = app.playback();
            if event.click_count() >= 2 {
                playback.set_repeat_mode(RepeatMode::One);
            } else {
                playback.toggle_repeat_all();
            }
            drop(playback);
            cx.notify();
        })),
    );

    if let Some(track) = &track {
        let left = center + controls_width * 0.5 + 14.0;
        let right = volume_left - 10.0;
        row = row.children(audio_chip(track, left, right, row_width, colors));
    }
    row = row.child(volume_control(app, volume_left, volume_width, colors, cx));
    let has_tracks = !app.library.model.tracks.is_empty();
    row = row.child(
        place(
            transport_button(
                "player-queue",
                IconName::ListMusic,
                20.0,
                "Queue",
                app.workbench.show_queue,
                true,
                colors,
            ),
            row_width - 66.0,
            (34.0, 38.0),
        )
        .on_click(cx.listener(|app, _, _, cx| {
            app.workbench.show_queue = !app.workbench.show_queue;
            cx.notify();
        })),
    );
    row = row.child(
        place(
            transport_button(
                "player-random-track",
                IconName::Dice5,
                20.0,
                "Play random song",
                false,
                has_tracks,
                colors,
            ),
            row_width - 24.0,
            (34.0, 38.0),
        )
        .when(has_tracks, |button| {
            button.on_click(cx.listener(|app, _, _, cx| {
                app.playback().play_random_track(&app.library.model.tracks);
                cx.notify();
            }))
        }),
    );

    let seek = seek_row(app, &transport, duration, window.scale_factor(), colors, cx);
    div()
        .id("scrubber-controls")
        .flex()
        .flex_col()
        .flex_none()
        .h(sz(PANEL_HEIGHT))
        .px(sz(PANEL_PADDING))
        .py(sz(8.0))
        .bg(colors.panel)
        .border_t_1()
        .border_color(colors.border)
        .child(row)
        .child(seek)
        .into_any_element()
}

/// Apply a seek or volume drag that has just been let go.
fn finish_drags(app: &mut ViperApp, cx: &Context<ViperApp>) {
    if cx.has_active_drag() {
        return;
    }
    if let Some((track, fraction)) = app.ui.seek_preview.take() {
        seek_to(app, track, fraction);
    }
    if std::mem::take(&mut app.ui.volume_dragging) {
        app.save_settings();
    }
}

/// Seek `track` to `fraction` of its length, unless another track has started since the drag began.
fn seek_to(app: &mut ViperApp, track: usize, fraction: f32) {
    let mut playback = app.playback();
    let Some(duration) = playback
        .current
        .filter(|&current| current == track)
        .and_then(|index| app.library.model.tracks.get(index))
        .and_then(|track| track.duration_ms)
        .map(Duration::from_millis)
    else {
        return;
    };
    playback.seek(&app.library.model.tracks, duration.mul_f32(fraction));
}

/// Set the volume from a position along the slider.
fn set_volume(app: &mut ViperApp, fraction: f32) {
    app.settings.volume = (fraction * 100.0).round() as u8;
    app.playback().set_volume(app.settings.volume);
}

/// Position `element` of `size` so it is centered at `x` across the row.
fn place(element: Stateful<Div>, x: f32, size: (f32, f32)) -> Stateful<Div> {
    element
        .absolute()
        .left(sz(x - size.0 * 0.5))
        .top(sz((ROW_HEIGHT - size.1) * 0.5))
        .w(sz(size.0))
        .h(sz(size.1))
}

/// A transport icon that lights up while its mode is on.
fn transport_button(
    id: &'static str,
    name: IconName,
    size: f32,
    description: &'static str,
    active: bool,
    enabled: bool,
    colors: Palette,
) -> Stateful<Div> {
    let tint = if active { colors.accent } else { colors.muted };
    let hovered = if active { colors.accent } else { colors.text };
    describe_button(div().id(id), description)
        .flex()
        .items_center()
        .justify_center()
        .tooltip(tooltip(description))
        .text_color(tint)
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.text_color(hovered))
        })
        .child(inherited_icon(name, size))
}

/// The round play and pause button in the middle of the bar.
fn play_button(
    transport: &Transport,
    center: f32,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> Stateful<Div> {
    let has_current = transport.current.is_some();
    let hover_text = if !has_current && !transport.play_enabled {
        "Select an album or turn on Shuffle to play"
    } else if transport.playing {
        "Pause"
    } else if has_current {
        "Resume"
    } else {
        "Play"
    };
    let description = if transport.playing { "Pause" } else { "Play" };
    let button = describe_button(div().id("player-play-pause"), description)
        .flex()
        .items_center()
        .justify_center()
        .tooltip(tooltip(hover_text))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(sz(46.0))
                .rounded_full()
                .bg(if has_current {
                    colors.accent
                } else {
                    colors.surface
                })
                .child(icon(
                    if transport.playing {
                        IconName::Pause
                    } else {
                        IconName::Play
                    },
                    24.0,
                    if has_current {
                        colors.on_accent
                    } else {
                        colors.muted
                    },
                )),
        );
    place(button, center, (50.0, 50.0)).when(transport.play_enabled, |button| {
        button
            .cursor_pointer()
            .on_click(cx.listener(|app, _, _, cx| {
                app.playback().play_pause(&app.library);
                cx.notify();
            }))
    })
}

/// The current track's artwork, title and artist, which open its album when clicked.
fn track_info(
    app: &mut ViperApp,
    current: Option<usize>,
    cx: &mut Context<ViperApp>,
) -> Option<AnyElement> {
    let colors = theme::colors(cx);
    let index = current?;
    let track = app.library.model.tracks.get(index)?;
    let (title, artist) = (track.title.clone(), track.artist.clone());
    let album_index = app
        .library
        .model
        .track_album
        .get(index)
        .copied()
        .flatten()
        .map(AlbumIndex::get);
    let texture = album_index.and_then(|album_index| {
        let bytes = app.library.model.albums.get(album_index)?.art.clone()?;
        app.artwork_cache.texture(&app.repaint, album_index, &bytes)
    });
    let cover = div()
        .flex_none()
        .size(sz(TRACK_ART_SIZE))
        .children(texture.map(|texture| img(texture).size_full()));
    let text = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_size(sz(15.0))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.text)
                .truncate()
                .child(title),
        )
        .child(
            div()
                .text_size(sz(13.0))
                .text_color(colors.muted)
                .truncate()
                .child(artist),
        );
    let info = div()
        .id("now-playing-album")
        .flex()
        .items_center()
        .gap(sz(9.0))
        .h_full()
        .max_w_full()
        .child(cover)
        .child(text);
    Some(match album_index {
        Some(album_index) => describe_button(info, "Go to playing album")
            .cursor_pointer()
            .tooltip(tooltip("Go to album"))
            .on_click(cx.listener(move |app, _, _, cx| {
                app.workbench.open_album(&mut app.library.view, album_index);
                cx.notify();
            }))
            .into_any_element(),
        None => info.into_any_element(),
    })
}

/// The volume icon, slider and percentage, which save the volume once it is set.
fn volume_control(
    app: &ViperApp,
    left: f32,
    width: f32,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let volume = app.settings.volume;
    let volume_icon = match volume {
        0 => IconName::VolumeX,
        1..=35 => IconName::Volume1,
        36..=u8::MAX => IconName::Volume2,
    };
    let fraction = f32::from(volume) / 100.0;
    let bounds = std::rc::Rc::clone(&app.ui.volume_bounds);
    let slider = div()
        .id("player-volume-slider")
        .role(Role::Slider)
        .aria_label("Volume")
        .relative()
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .cursor_pointer()
        .child(measure(&app.ui.volume_bounds))
        .child(
            div()
                .relative()
                .w_full()
                .h(px(3.0))
                .rounded(px(2.0))
                .bg(colors.surface)
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .rounded(px(2.0))
                        .bg(colors.accent),
                )
                .child(knob(fraction, colors)),
        )
        .on_click(cx.listener(move |app, event: &ClickEvent, _, cx| {
            set_volume(app, horizontal_fraction(bounds.get(), event.position()));
            app.save_settings();
            cx.notify();
        }))
        .on_drag(VolumeDrag, |_, _, _, cx| cx.new(|_| DragPreview(None)))
        .on_drag_move(listen(
            cx,
            |app, event: &DragMoveEvent<VolumeDrag>, _, _| {
                set_volume(app, horizontal_fraction(event.bounds, event.event.position));
                app.ui.volume_dragging = true;
            },
        ));
    div()
        .absolute()
        .left(sz(left))
        .top(sz(ROW_HEIGHT * 0.5 - 15.0))
        .w(sz(width))
        .h(sz(30.0))
        .flex()
        .items_center()
        .child(
            div()
                .flex_none()
                .w(sz(30.0))
                .child(icon(volume_icon, 22.0, colors.muted)),
        )
        .child(slider)
        .child(
            div()
                .flex_none()
                .w(sz(46.0))
                .flex()
                .justify_center()
                .text_size(sz(11.0))
                .text_color(colors.muted)
                .child(format!("{volume}%")),
        )
        .into_any_element()
}

/// The slider knob at `fraction` along its track.
fn knob(fraction: f32, colors: Palette) -> Div {
    div()
        .absolute()
        .top(px(-4.5))
        .left(relative(fraction))
        .ml(px(-3.0))
        .w(px(6.0))
        .h(px(12.0))
        .rounded(px(3.0))
        .bg(colors.text)
}

/// The seek bar with the elapsed time and the track length on either side.
fn seek_row(
    app: &mut ViperApp,
    transport: &Transport,
    duration: Option<Duration>,
    scale_factor: f32,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let max_seconds = duration.map_or(0.0, |duration| duration.as_secs_f64());
    let can_seek = max_seconds > 0.0;
    let current = transport.current;
    // Dragging previews the target time; the decoder seeks once on release.
    let shown = app
        .ui
        .seek_preview
        .filter(|&(track, _)| can_seek && transport.current == Some(track))
        .map_or(transport.position, |(_, fraction)| {
            Duration::from_secs_f64(max_seconds * f64::from(fraction))
        });
    let fraction = if can_seek {
        (shown.as_secs_f64() / max_seconds).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    if transport.playing {
        // Repaint when the time label's second changes or the knob next moves a fraction of a
        // pixel, not at a fixed rate: a long track moves the knob a pixel only every second or so.
        let position = shown.as_secs_f64();
        let mut next_change = 1.0 - position.fract();
        let pixels = f64::from(f32::from(app.ui.seek_bounds.get().size.width) * scale_factor);
        if can_seek && pixels > 0.0 {
            let seconds_per_step = max_seconds / (pixels * KNOB_STEPS_PER_PIXEL);
            next_change = next_change.min(seconds_per_step - position % seconds_per_step);
        }
        app.repaint
            .request_repaint_after(Duration::from_secs_f64(next_change).max(MIN_PLAYING_REPAINT));
    }
    let hover = app.ui.seek_hover.filter(|_| can_seek);
    let bounds = std::rc::Rc::clone(&app.ui.seek_bounds);
    let hover_bounds = std::rc::Rc::clone(&app.ui.seek_bounds);
    let time_label = |text: String| {
        div()
            .flex_none()
            .w(sz(TIME_LABEL_WIDTH))
            .text_size(sz(11.0))
            .text_color(colors.subtle)
            .child(text)
    };
    let bar = div()
        .id("player-seek-bar")
        .role(Role::Slider)
        .aria_label("Playback position")
        .relative()
        .flex_1()
        .h(sz(14.0))
        .flex()
        .items_center()
        .child(measure(&app.ui.seek_bounds))
        .child(
            div()
                .relative()
                .w_full()
                .h(px(3.0))
                .rounded(px(2.0))
                .bg(colors.surface)
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .rounded(px(2.0))
                        .bg(colors.accent),
                )
                .when(transport.current.is_some() && duration.is_some(), |track| {
                    track.child(knob(fraction, colors))
                }),
        )
        .children(hover.map(|at| {
            div()
                .absolute()
                .bottom(sz(18.0))
                .left(relative(at))
                .ml(sz(-20.0))
                .w(sz(40.0))
                .flex()
                .justify_center()
                .child(
                    div()
                        .px(sz(6.0))
                        .py(sz(2.0))
                        .bg(colors.surface)
                        .border_1()
                        .border_color(colors.border)
                        .rounded(sz(4.0))
                        .text_size(sz(11.0))
                        .text_color(colors.text)
                        .child(format_time(Duration::from_secs_f64(
                            max_seconds * f64::from(at),
                        ))),
                )
        }))
        .when(can_seek, |bar| {
            bar.cursor_pointer()
                .on_click(cx.listener(move |app, event: &ClickEvent, _, cx| {
                    if let Some(track) = current {
                        seek_to(
                            app,
                            track,
                            horizontal_fraction(bounds.get(), event.position()),
                        );
                    }
                    cx.notify();
                }))
                .on_mouse_move(
                    cx.listener(move |app, event: &gpui::MouseMoveEvent, _, cx| {
                        app.ui.seek_hover =
                            Some(horizontal_fraction(hover_bounds.get(), event.position));
                        cx.notify();
                    }),
                )
                .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                    if !hovered {
                        app.ui.seek_hover = None;
                        cx.notify();
                    }
                }))
                .on_drag(SeekDrag, |_, _, _, cx| cx.new(|_| DragPreview(None)))
                .on_drag_move(listen(
                    cx,
                    move |app, event: &DragMoveEvent<SeekDrag>, _, _| {
                        // The drag stays with the track it began on.
                        let track = app
                            .ui
                            .seek_preview
                            .map_or(current, |(track, _)| Some(track));
                        app.ui.seek_preview = track.map(|track| {
                            (
                                track,
                                horizontal_fraction(event.bounds, event.event.position),
                            )
                        });
                    },
                ))
        });
    div()
        .flex()
        .items_center()
        .gap(sz(8.0))
        .w_full()
        .child(time_label(format_time(shown)))
        .child(bar)
        .child(time_label(duration.map(format_time).unwrap_or_default()))
        .into_any_element()
}

/// File type, bitrate and resolution of the playing track between the controls and the volume.
fn audio_chip(
    track: &Track,
    left: f32,
    right: f32,
    row_width: f32,
    colors: Palette,
) -> Option<AnyElement> {
    if right <= left {
        return None;
    }
    let file_type = track
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("AUDIO")
        .to_ascii_uppercase();
    let audio = track.audio;

    let mut details = vec![file_type.clone()];
    let mut tooltip_details = vec![format!("File type: {file_type}")];
    if let Some(bitrate) = audio.bitrate_kbps {
        details.push(format!("{bitrate} kb/s"));
        tooltip_details.push(format!("Bitrate: {bitrate} kb/s"));
    }
    if let (Some(bit_depth), Some(sample_rate)) = (audio.bit_depth, audio.sample_rate_hz) {
        let sample_rate_khz = sample_rate as f32 / 1000.0;
        let sample_rate_label = if sample_rate % 1000 == 0 {
            format!("{}", sample_rate / 1000)
        } else {
            format!("{sample_rate_khz:.1}")
        };
        details.push(format!("{bit_depth}/{sample_rate_label}"));
        tooltip_details.push(format!(
            "{bit_depth}-bit, {sample_rate_khz:.1} kHz sample rate"
        ));
    } else if let Some(sample_rate) = audio.sample_rate_hz {
        let label = format!("{:.1} kHz", sample_rate as f32 / 1000.0);
        details.push(label.clone());
        tooltip_details.push(format!("Sample rate: {label}"));
    } else if let Some(bit_depth) = audio.bit_depth {
        details.push(format!("{bit_depth}-bit"));
        tooltip_details.push(format!("Bit depth: {bit_depth}-bit"));
    }

    let label = details.join("  •  ");
    // Text is not measured before layout, so a generous average glyph width decides whether the
    // chip fits; the chip itself is as wide as its text.
    let chip_width = label.chars().count() as f32 * 5.6 + 16.0;
    if chip_width > right - left {
        return None;
    }
    Some(
        div()
            .id("audio-info-chip")
            .absolute()
            .right(sz(row_width - right))
            .top(sz(ROW_HEIGHT * 0.5 - 11.0))
            .h(sz(22.0))
            .px(sz(8.0))
            .flex()
            .items_center()
            .bg(colors.panel)
            .border_1()
            .border_color(colors.border)
            .rounded(sz(3.0))
            .text_size(sz(10.0))
            .text_color(colors.muted)
            .whitespace_nowrap()
            .tooltip(tooltip(tooltip_details.join("\n")))
            .child(label)
            .into_any_element(),
    )
}

fn format_time(time: Duration) -> String {
    let total = time.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}
