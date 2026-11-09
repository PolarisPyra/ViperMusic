use crate::{
    app::ViperApp,
    shared::ui::{
        controls::{backdrop, click, close_button, menu_item, modal_frame, sz},
        icons::{icon, IconName},
        theme::{self, Palette},
    },
    workbench::Page,
};
use gpui::{div, prelude::*, px, AnyElement, Context, MouseButton, WeakEntity};
use gpui_component::{
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenu},
    Sizable,
};

/// Height of the menu bar at 100% zoom.
pub(in crate::workbench) const HEIGHT: f32 = 34.0;
/// Width of every menu popup.
const MENU_WIDTH: f32 = 220.0;

/// Shortcuts listed in the Keyboard Shortcuts dialog.
const SHORTCUTS: [(&str, &str); 10] = [
    ("F", "Focus search"),
    ("Arrows", "Move the selection"),
    ("Enter", "Open album or play the selected song"),
    ("Space", "Play or pause"),
    ("Ctrl ← / →", "Seek 5 seconds"),
    ("Ctrl ↑ / ↓", "Change volume"),
    ("Ctrl + / − / 0", "Zoom in, out, or reset"),
    ("F5", "Play a random song"),
    ("Q", "Show or hide queue"),
    ("Esc", "Close panels and album details"),
];

/// Draw the menu bar across the top of the window.
///
/// # Arguments
/// * `cx` - Application entity context.
///
/// # Returns
/// The menu bar element.
pub(in crate::workbench) fn render(cx: &mut Context<ViperApp>) -> AnyElement {
    let colors = theme::colors(cx);
    let weak = cx.weak_entity();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(sz(4.0))
        .h(sz(HEIGHT))
        .px(sz(8.0))
        .py(sz(2.0))
        .bg(colors.canvas)
        .border_b_1()
        .border_color(colors.border)
        .child(icon(IconName::Disc, 18.0, colors.muted).mr(sz(8.0)))
        .child(menu("file-menu", "File", &weak, |menu, weak, _, _| {
            menu.item(menu_item("Preferences", weak, |app, _, _| {
                app.workbench.show_preferences = true;
            }))
            .separator()
            .item(menu_item("Quit", weak, |app, _, cx| {
                app.request_quit(true, cx)
            }))
        }))
        .child(menu(
            "library-menu",
            "Library",
            &weak,
            |menu, weak, window, cx| {
                let weak = weak.clone();
                menu.submenu("Add Library", window, cx, move |menu, _, _| {
                    menu.min_w(px(MENU_WIDTH)).item(menu_item(
                        "Choose Music Folder",
                        &weak,
                        |_, _, cx| ViperApp::choose_folder(cx),
                    ))
                })
            },
        ))
        .child(menu("playback-menu", "Playback", &weak, playback_menu))
        .child(menu("view-menu", "View", &weak, |mut menu, weak, _, cx| {
            let current = weak.upgrade().map(|app| app.read(cx).workbench.page);
            for page in Page::ALL {
                menu = menu.item(
                    menu_item(page.label(), weak, move |app, _, _| {
                        app.workbench.page = page
                    })
                    .checked(current == Some(page)),
                );
            }
            menu
        }))
        .child(menu("help-menu", "Help", &weak, |menu, weak, _, _| {
            menu.item(menu_item("Keyboard Shortcuts", weak, |app, _, _| {
                app.workbench.show_shortcuts = true;
            }))
        }))
        .child(div().flex_1())
        .child(version_chip(colors))
        .into_any_element()
}

/// A quiet menu-bar trigger that opens a menu built by `build` each time it is opened.
fn menu(
    id: &'static str,
    title: &'static str,
    weak: &WeakEntity<ViperApp>,
    build: impl Fn(
            PopupMenu,
            &WeakEntity<ViperApp>,
            &mut gpui::Window,
            &mut Context<PopupMenu>,
        ) -> PopupMenu
        + 'static,
) -> impl IntoElement {
    let weak = weak.clone();
    Button::new(id)
        .label(title)
        .ghost()
        .small()
        .dropdown_menu(move |menu, window, cx| build(menu.min_w(px(MENU_WIDTH)), &weak, window, cx))
}

/// The Playback menu, with entries enabled by what can currently play.
fn playback_menu(
    menu: PopupMenu,
    weak: &WeakEntity<ViperApp>,
    _: &mut gpui::Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let (has_current, can_play, paused, has_history, has_next) = weak
        .upgrade()
        .map(|app| {
            let app = app.read(cx);
            let playback = app.playback();
            let has_current = playback.current.is_some();
            let can_play = has_current
                || app.library.view.selected_album.is_some()
                || playback.has_next()
                || (playback.shuffle_enabled && !app.library.model.tracks.is_empty());
            (
                has_current,
                can_play,
                playback.is_paused(),
                playback.has_history(),
                playback.has_next(),
            )
        })
        .unwrap_or_default();
    let play_label = if has_current && paused {
        "Resume"
    } else if has_current {
        "Pause"
    } else {
        "Play Selected Album"
    };
    menu.item(menu_item("Show Queue", weak, |app, _, _| {
        app.workbench.show_queue = true;
    }))
    .item(
        menu_item(play_label, weak, |app, _, _| {
            app.playback().play_pause(&app.library);
        })
        .disabled(!can_play),
    )
    .separator()
    .item(
        menu_item("Previous Track", weak, |app, _, _| {
            app.playback().previous(&app.library.model.tracks);
        })
        .disabled(!has_current && !has_history),
    )
    .item(
        menu_item("Next Track", weak, |app, _, _| {
            app.playback().skip_next(&app.library.model.tracks);
        })
        .disabled(!has_next),
    )
    .item(menu_item("Stop", weak, |app, _, _| app.playback().stop()).disabled(!has_current))
}

/// The running build's version as a small chip at the right end of the menu bar.
fn version_chip(colors: Palette) -> impl IntoElement {
    div()
        .px(sz(8.0))
        .py(sz(2.0))
        .bg(colors.text.opacity(0.06))
        .border_1()
        .border_color(colors.border)
        .rounded(sz(4.0))
        .text_size(sz(11.0))
        .text_color(colors.muted)
        .child(env!("CARGO_PKG_VERSION"))
}

/// Draw the Keyboard Shortcuts dialog while it is open.
///
/// # Arguments
/// * `app` - Application holding whether the dialog is open.
/// * `cx` - Application entity context.
///
/// # Returns
/// The dialog, or `None` while it is closed.
pub(in crate::workbench) fn render_shortcuts(
    app: &ViperApp,
    cx: &mut Context<ViperApp>,
) -> Option<AnyElement> {
    if !app.workbench.show_shortcuts {
        return None;
    }
    let colors = theme::colors(cx);
    let close =
        |cx: &Context<ViperApp>| click(cx, |app, _, _| app.workbench.show_shortcuts = false);
    let rows = SHORTCUTS.iter().map(|&(shortcut, description)| {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(sz(16.0))
            .child(div().text_color(colors.text).child(description))
            .child(
                div()
                    .px(sz(7.0))
                    .py(sz(3.0))
                    .bg(colors.canvas)
                    .rounded(sz(4.0))
                    .text_size(sz(11.0))
                    .text_color(colors.muted)
                    .child(shortcut),
            )
    });
    Some(
        backdrop("keyboard-shortcuts-backdrop", colors)
            .on_click(close(cx))
            .child(
                modal_frame(colors)
                    .min_w(sz(360.0))
                    .max_w(sz(420.0))
                    .gap(sz(5.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(sz(22.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(colors.text)
                                    .child("Keyboard Shortcuts"),
                            )
                            .child(close_button(
                                "close-shortcuts",
                                "Close keyboard shortcuts",
                                colors,
                                close(cx),
                            )),
                    )
                    .child(div().my(sz(8.0)).h(px(1.0)).bg(colors.border))
                    .children(rows),
            )
            .into_any_element(),
    )
}
