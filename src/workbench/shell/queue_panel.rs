use crate::{
    app::ViperApp,
    shared::ui::{
        controls::{
            action_button, close_button, describe_button, label, listen, sz, tooltip, DragPreview,
        },
        icons::{icon, IconName},
        theme::{self, Palette},
    },
};
use gpui::{div, prelude::*, px, AnyElement, Context, DragMoveEvent, FontWeight, SharedString};

pub(in crate::workbench) const PANEL_WIDTH: f32 = 340.0;

/// Marks a queued song being dragged, by its queue position.
struct QueueDrag(usize);

/// Draw the playback queue as a side panel that stays open while browsing.
///
/// Rows can be dragged to reorder them or removed individually.
///
/// # Arguments
/// * `app` - Application holding the queue and whether the panel is open.
/// * `cx` - Application entity context.
///
/// # Returns
/// The panel, or `None` while it is closed.
pub(in crate::workbench) fn render(
    app: &mut ViperApp,
    cx: &mut Context<ViperApp>,
) -> Option<AnyElement> {
    if !app.workbench.show_queue {
        app.ui.queue_drop = None;
        return None;
    }
    if !cx.has_active_drag() {
        app.ui.queue_drop = None;
    }
    let colors = theme::colors(cx);
    let playback = app.playback();
    let queue = playback.queue().iter().copied().collect::<Vec<_>>();
    let current = playback
        .current
        .and_then(|index| app.library.model.tracks.get(index))
        .map(|track| (track.title.clone(), track.artist.clone()));
    drop(playback);

    let count = queue.len();
    let header = div()
        .flex()
        .items_center()
        .gap(sz(8.0))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(sz(20.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(colors.text)
                        .child("Up Next"),
                )
                .child(
                    div()
                        .text_size(sz(12.0))
                        .text_color(colors.muted)
                        .child(format!(
                            "{count} queued song{}",
                            if count == 1 { "" } else { "s" }
                        )),
                ),
        )
        .when(count > 0, |header| {
            header.child(
                action_button(
                    "clear-queue",
                    "Clear",
                    true,
                    colors,
                    cx.listener(|app, _, _, cx| {
                        app.playback().clear_queue();
                        cx.notify();
                    }),
                )
                .tooltip(tooltip("Remove every queued song")),
            )
        })
        .child(close_button(
            "close-queue",
            "Close queue (Q)",
            colors,
            cx.listener(|app, _, _, cx| {
                app.workbench.show_queue = false;
                cx.notify();
            }),
        ));

    let now_playing = current.map(|(title, artist)| {
        div()
            .flex()
            .flex_col()
            .bg(colors.surface)
            .rounded(sz(8.0))
            .px(sz(12.0))
            .py(sz(10.0))
            .child(
                div()
                    .text_size(sz(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(colors.accent)
                    .child("NOW PLAYING"),
            )
            .child(truncated(title, 13.0, colors.text).font_weight(FontWeight::BOLD))
            .child(truncated(artist, 11.0, colors.muted))
    });

    let body = if queue.is_empty() {
        div()
            .flex()
            .flex_col()
            .items_center()
            .pt(sz(24.0))
            .child(
                div()
                    .text_size(sz(15.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(colors.text)
                    .child("The queue is empty"),
            )
            .child(
                div()
                    .text_size(sz(12.0))
                    .text_color(colors.muted)
                    .child("Right-click a song or album to add it."),
            )
            .into_any_element()
    } else {
        let rows = queue
            .iter()
            .enumerate()
            .map(|(index, &track)| queue_row(app, index, track, colors, cx))
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(sz(6.0))
            .child(label("Drag songs to reorder", 11.0, colors.subtle))
            .child(
                div()
                    .id("queue-panel-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(sz(6.0))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    };

    Some(
        div()
            .id("queue-panel")
            .flex()
            .flex_col()
            .flex_none()
            .gap(sz(12.0))
            .w(sz(PANEL_WIDTH))
            .h_full()
            .p(sz(16.0))
            .bg(colors.panel)
            .border_l_1()
            .border_color(colors.border)
            .child(header)
            .children(now_playing)
            .child(body)
            .into_any_element(),
    )
}

/// Draw one queued song, which can be dragged elsewhere in the queue or removed.
fn queue_row(
    app: &ViperApp,
    index: usize,
    track_index: usize,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let (title, artist) = app.library.model.tracks.get(track_index).map_or_else(
        || ("Unavailable track".to_owned(), String::new()),
        |track| (track.title.clone(), track.artist.clone()),
    );
    let preview = SharedString::from(title.clone());
    let drop_line = |at: usize| {
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(2.0))
            .bg(colors.accent)
            .when(at == index, |line| line.top(sz(-4.0)))
            .when(at != index, |line| line.bottom(sz(-4.0)))
    };
    let drop = app
        .ui
        .queue_drop
        .filter(|&at| at == index || at == index + 1);
    div()
        .id(("queue-item", index))
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(sz(8.0))
        .px(sz(8.0))
        .py(sz(6.0))
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border)
        .rounded(sz(8.0))
        .cursor_grab()
        .on_drag(QueueDrag(index), move |_, _, _, cx| {
            cx.new(|_| DragPreview(Some(preview.clone())))
        })
        .on_drag_move(listen(
            cx,
            move |app, event: &DragMoveEvent<QueueDrag>, _, _| {
                let position = event.event.position;
                if event.bounds.contains(&position) {
                    let below = position.y > event.bounds.center().y;
                    app.ui.queue_drop = Some(if below { index + 1 } else { index });
                }
            },
        ))
        .on_drop(listen(cx, |app, drag: &QueueDrag, _, _| {
            if let Some(target) = app.ui.queue_drop.take() {
                // Removing the dragged row shifts later targets up by one.
                let to = if drag.0 < target { target - 1 } else { target };
                app.playback().move_queued(drag.0, to);
            }
        }))
        .child(icon(IconName::GripVertical, 16.0, colors.subtle))
        .child(label(format!("{:02}", index + 1), 11.0, colors.subtle).flex_none())
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(truncated(title, 12.0, colors.text).font_weight(FontWeight::BOLD))
                .when(!artist.is_empty(), |column| {
                    column.child(truncated(artist, 10.0, colors.muted))
                }),
        )
        .child(
            describe_button(
                div().id(("remove-queued", index)),
                format!("Remove queued song {}", index + 1),
            )
            .flex_none()
            .p(sz(2.0))
            .rounded(sz(4.0))
            .cursor_pointer()
            .hover(|style| style.bg(colors.hover))
            .tooltip(tooltip("Remove from queue"))
            .on_click(cx.listener(move |app, _, _, cx| {
                app.playback().remove_queued(index);
                cx.notify();
            }))
            .child(icon(IconName::X, 16.0, colors.muted)),
        )
        .children(drop.map(drop_line))
        .into_any_element()
}

/// One line of text that is cut short with an ellipsis when it does not fit.
fn truncated(text: String, size: f32, color: gpui::Hsla) -> gpui::Div {
    label(text, size, color).w_full().truncate()
}
