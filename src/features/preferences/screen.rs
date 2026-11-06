use super::state::PreferencesState;
use crate::{
    app::ViperApp,
    platform::{
        persistence::settings::{Settings, StartupView},
        presence::discord::{self, PresenceStatus},
        updates::{UpdateState, RELEASES_URL},
    },
    shared::{
        action::AppAction,
        artwork::render_image,
        logo::LogoColorway,
        ui::{
            controls::{
                action_button, backdrop, click, close_button, horizontal_fraction, icon_button,
                label, listen, measure, modal_frame, sz, text_field, tooltip, unzoomed,
                DragPreview,
            },
            icons::{icon, IconName},
            text_input::FieldOptions,
            theme::{self, Palette, ThemeId},
        },
    },
    workbench::Page,
};
use gpui::{
    div, img, prelude::*, px, relative, AnyElement, App, ClickEvent, ClipboardItem, Context, Div,
    DragMoveEvent, FontWeight, MouseButton, Role, SharedString, Window,
};
use gpui_component::{checkbox::Checkbox, input::Input, Disableable};

/// Prefix of the keys of the dialog's text fields.
const FIELD_PREFIX: &str = "preferences-";
const MAX_WIDTH: f32 = 760.0;
/// Space between the dialog and the window's edges.
const MARGIN: f32 = 24.0;
const MIN_ZOOM: f32 = 0.75;
const MAX_ZOOM: f32 = 2.0;
const ZOOM_STEP: f32 = 0.05;

/// Marker dragged by the interface scale slider's knob.
struct ZoomDrag;

/// Draw the Preferences dialog while it is open.
///
/// # Arguments
/// * `app` - Application holding the settings.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The dialog over a backdrop, or `None` while it is closed.
pub(crate) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> Option<AnyElement> {
    if app.ui.zoom_dragging && !cx.has_active_drag() {
        app.ui.zoom_dragging = false;
        app.save_settings();
    }
    if !app.workbench.show_preferences {
        return None;
    }
    let colors = theme::colors(cx);
    let viewport = window.viewport_size();
    let (screen_width, screen_height) = (
        unzoomed(viewport.width, window),
        unzoomed(viewport.height, window),
    );
    let margin = MARGIN.min(screen_width * 0.05).min(screen_height * 0.05);
    let category = app.preferences.category.unwrap_or(Category::General);
    let content = match category {
        Category::General => general(app, colors, cx),
        Category::Audio => audio(app, colors, cx),
        Category::Connections => connections(app, colors, window, cx),
        Category::Theme => theme_page(app, colors, cx),
        Category::View => view(app, colors, cx),
        Category::Services => services(app, colors, window, cx),
        Category::About => about(app, colors, cx),
    };
    let header = div()
        .flex()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .child(label("Preferences", 23.0, colors.text).font_weight(FontWeight::BOLD))
                .child(label(
                    "Manage your library, connections, and startup experience",
                    12.0,
                    colors.muted,
                )),
        )
        .child(close_button(
            "preferences-close",
            "Close preferences",
            colors,
            click(cx, |app, _, _| close(app)),
        ));
    Some(
        backdrop("preferences-backdrop", colors)
            .on_click(click(cx, |app, _, _| close(app)))
            .child(
                modal_frame(colors)
                    .w(sz((screen_width - margin * 2.0).clamp(0.0, MAX_WIDTH)))
                    .h(sz((screen_height - margin * 2.0).max(0.0)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(header)
                    .child(
                        div()
                            .mt(sz(18.0))
                            .child(category_tabs(category, colors, cx)),
                    )
                    .child(div().my(sz(12.0)).h(px(1.0)).bg(colors.border))
                    .child(
                        div()
                            .id("preferences-content")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .pt(sz(8.0))
                            .text_size(sz(13.0))
                            .text_color(colors.text)
                            .child(content),
                    ),
            )
            .into_any_element(),
    )
}

fn close(app: &mut ViperApp) {
    app.workbench.show_preferences = false;
    app.inputs.remove_prefixed(FIELD_PREFIX);
}

fn category_tabs(selected: Category, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    div()
        .id("preferences-category-tabs")
        .flex()
        .gap(sz(6.0))
        .overflow_x_scroll()
        .children(Category::ALL.map(|category| {
            let is_selected = selected == category;
            div()
                .id(SharedString::from(category.label()))
                .role(Role::Tab)
                .aria_label(category.label())
                .flex_none()
                .px(sz(11.0))
                .py(sz(7.0))
                .rounded(sz(6.0))
                .cursor_pointer()
                .text_size(sz(12.0))
                .text_color(if is_selected {
                    colors.text
                } else {
                    colors.muted
                })
                .when(is_selected, |tab| tab.bg(colors.selected))
                .when(!is_selected, |tab| {
                    tab.hover(|style| style.bg(colors.surface))
                })
                .child(category.label())
                .on_click(click(cx, move |app, _, _| {
                    app.preferences.category = Some(category);
                }))
        }))
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// Preferences page currently selected in the preferences dialog.
pub enum Category {
    /// General application options.
    General,
    /// Audio device and volume options.
    Audio,
    /// Local library and SMB connection options.
    Connections,
    /// Application color theme options.
    Theme,
    /// Sidebar and other view options.
    View,
    /// External service options.
    Services,
    /// Application version and information.
    About,
}

impl Category {
    const ALL: [Self; 7] = [
        Self::General,
        Self::Audio,
        Self::Connections,
        Self::Theme,
        Self::View,
        Self::Services,
        Self::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Audio => "Audio",
            Self::Connections => "Connections",
            Self::Theme => "Theme",
            Self::View => "View",
            Self::Services => "Services",
            Self::About => "About",
        }
    }
}

/// A page of the dialog: a column of sections.
fn page() -> Div {
    div().flex().flex_col().min_w(sz(420.0))
}

fn section_title(title: &'static str, colors: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .child(label(title, 15.0, colors.text).font_weight(FontWeight::BOLD))
        .child(div().mt(sz(4.0)).mb(sz(8.0)).h(px(1.0)).bg(colors.border))
}

fn hint(text: &'static str, colors: Palette) -> Div {
    label(text, 12.0, colors.muted)
}

fn general(app: &ViperApp, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    let path = app.settings.music_path.as_ref().map_or_else(
        || "No music folder selected".to_owned(),
        |path| path.display().to_string(),
    );
    let startup = [
        (StartupView::Home, "Home"),
        (StartupView::Albums, "Albums"),
        (StartupView::Songs, "Songs"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (view, name))| {
        choice_row(
            ("startup", index),
            name,
            app.settings.startup_view == view,
            "Current startup page",
            colors,
            click(cx, move |app, _, _| {
                if app.settings.startup_view != view {
                    app.settings.startup_view = view;
                    app.save_settings();
                }
            }),
        )
    });
    page()
        .child(section_title("Music library", colors))
        .child("Choose the folder that contains your music.")
        .child(
            div()
                .flex()
                .items_center()
                .gap(sz(12.0))
                .mt(sz(12.0))
                .child(
                    label(path, 12.0, colors.muted)
                        .flex_1()
                        .min_w_0()
                        .truncate(),
                )
                .child(
                    action_button(
                        "preferences-choose-folder",
                        "Choose folder",
                        true,
                        colors,
                        click(cx, |_, _, cx| ViperApp::choose_folder(cx)),
                    )
                    .min_w(sz(112.0)),
                ),
        )
        .child(section_title("Startup", colors).mt(sz(28.0)))
        .child("Choose which page opens when the player starts.")
        .child(
            div()
                .flex()
                .flex_col()
                .gap(sz(6.0))
                .mt(sz(12.0))
                .children(startup),
        )
        .into_any_element()
}

fn theme_page(app: &mut ViperApp, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    let themes = ThemeId::ALL.into_iter().enumerate().map(|(index, theme)| {
        choice_row(
            ("theme", index),
            theme.label(),
            app.settings.theme == theme,
            "Current theme",
            colors,
            click(cx, move |app, _, _| {
                if app.settings.theme != theme {
                    app.settings.theme = theme;
                    app.save_settings();
                }
            }),
        )
    });
    let themes = themes.collect::<Vec<_>>();
    let thumbnails = &mut app.preferences.logo_thumbnails;
    if thumbnails.is_empty() {
        *thumbnails = LogoColorway::ALL
            .iter()
            .map(|colorway| render_image(colorway.decode().thumbnail(176, 176)))
            .collect();
    }
    let logos = LogoColorway::ALL
        .into_iter()
        .zip(app.preferences.logo_thumbnails.iter().cloned())
        .enumerate()
        .map(|(index, (colorway, thumbnail))| {
            let selected = app.settings.logo == colorway;
            div()
                .id(("logo", index))
                .role(Role::RadioButton)
                .aria_label(colorway.label())
                .p(sz(4.0))
                .rounded(sz(6.0))
                .border_1()
                .cursor_pointer()
                .tooltip(tooltip(colorway.label()))
                .when(selected, |logo| {
                    logo.bg(colors.selected).border_color(colors.accent)
                })
                .when(!selected, |logo| {
                    logo.border_color(colors.border)
                        .hover(|style| style.bg(colors.hover))
                })
                .child(img(thumbnail).size(sz(88.0)))
                .on_click(click(cx, move |app, _, _| {
                    if app.settings.logo != colorway {
                        app.settings.logo = colorway;
                        app.save_settings();
                    }
                }))
        });
    page()
        .child(section_title("Themes", colors))
        .child("Choose a theme for the player.")
        .child(
            div()
                .flex()
                .flex_col()
                .gap(sz(6.0))
                .mt(sz(12.0))
                .children(themes),
        )
        .child(section_title("App logo", colors).mt(sz(20.0)))
        .child("Choose the cassette color way for the window and tray icon.")
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(sz(10.0))
                .mt(sz(12.0))
                .children(logos),
        )
        .into_any_element()
}

fn audio(app: &ViperApp, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    page()
        .child(section_title("Playback", colors))
        .child("Choose how tracks transition during album playback.")
        .child(
            div().mt(sz(10.0)).child(
                checkbox(
                    "gapless-playback",
                    "Gapless playback",
                    app.settings.gapless_playback,
                    true,
                    cx,
                    |settings, checked| settings.gapless_playback = checked,
                )
                .tooltip("Queue the next track before the current track ends."),
            ),
        )
        .into_any_element()
}

/// A row choosing one of several options, with a radio mark and a status while chosen.
fn choice_row(
    id: impl Into<gpui::ElementId>,
    text: &'static str,
    selected: bool,
    status: &'static str,
    colors: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::RadioButton)
        .aria_label(text)
        .flex()
        .items_center()
        .gap(sz(10.0))
        .h(sz(44.0))
        .px(sz(10.0))
        .rounded(sz(7.0))
        .border_1()
        .border_color(colors.border)
        .cursor_pointer()
        .bg(if selected {
            colors.selected
        } else {
            colors.surface
        })
        .hover(|style| style.bg(colors.hover))
        .child(icon(
            if selected {
                IconName::CircleDot
            } else {
                IconName::Circle
            },
            16.0,
            if selected {
                colors.accent
            } else {
                colors.subtle
            },
        ))
        .child(label(text, 13.0, colors.text).flex_1().min_w_0().truncate())
        .when(selected, |row| {
            row.child(label(status, 11.0, colors.muted).flex_none())
        })
        .on_click(on_click)
}

/// A checkbox that edits one setting and saves the settings.
fn checkbox(
    id: impl Into<gpui::ElementId>,
    text: &'static str,
    checked: bool,
    enabled: bool,
    cx: &Context<ViperApp>,
    set: impl Fn(&mut Settings, bool) + 'static,
) -> Checkbox {
    Checkbox::new(id)
        .label(text)
        .checked(checked)
        .disabled(!enabled)
        .on_click(listen(cx, move |app, checked: &bool, _, _| {
            set(&mut app.settings, *checked);
            app.save_settings();
        }))
}

fn view(app: &ViperApp, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    let settings = &app.settings;
    let pages = Page::ALL.into_iter().enumerate().map(|(index, page)| {
        let shown = !settings.hidden_pages.contains(&page);
        let enabled = !shown || settings.can_hide_page(page);
        let checkbox = checkbox(
            ("show-page", index),
            page.label(),
            shown,
            enabled,
            cx,
            move |settings, shown| {
                if shown {
                    settings.hidden_pages.remove(&page);
                } else {
                    settings.hidden_pages.insert(page);
                }
            },
        );
        if enabled {
            checkbox
        } else {
            checkbox.tooltip("The sidebar keeps at least one page")
        }
    });
    page()
        .child(section_title("Sidebar", colors))
        .child(checkbox(
            "compact-sidebar",
            "Compact sidebar",
            settings.compact_sidebar,
            true,
            cx,
            |settings, checked| settings.compact_sidebar = checked,
        ))
        .child(hint(
            "Show only icons in a narrow sidebar. Hover over an icon to see its name.",
            colors,
        ))
        .child(div().mt(sz(12.0)).mb(sz(4.0)).child("Pages in the sidebar"))
        .child(div().flex().flex_col().gap(sz(4.0)).children(pages))
        .child(hint(
            "Hidden pages stay in the View menu. You can also right-click a page in the sidebar to hide it.",
            colors,
        ))
        .child(section_title("Interface scale", colors).mt(sz(20.0)))
        .child(zoom_slider(app, colors, cx))
        .child(hint(
            "You can also press Ctrl + and Ctrl − to zoom, and Ctrl 0 to reset.",
            colors,
        ))
        .child(section_title("Albums", colors).mt(sz(20.0)))
        .child(checkbox(
            "album-format-badges",
            "Show album format badges",
            settings.album_format_badges,
            true,
            cx,
            |settings, checked| settings.album_format_badges = checked,
        ))
        .child(hint("Show the audio file format on album artwork.", colors))
        .into_any_element()
}

/// The interface scale slider, from 75% to 200%.
fn zoom_slider(app: &ViperApp, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    let zoom = app.settings.ui_zoom;
    let fraction = ((zoom - MIN_ZOOM) / (MAX_ZOOM - MIN_ZOOM)).clamp(0.0, 1.0);
    let bounds = std::rc::Rc::clone(&app.ui.zoom_bounds);
    let slider = div()
        .id("interface-scale")
        .role(Role::Slider)
        .aria_label("Interface scale")
        .relative()
        .flex()
        .items_center()
        .w(sz(240.0))
        .h(sz(24.0))
        .cursor_pointer()
        .child(measure(&app.ui.zoom_bounds))
        .child(
            div()
                .relative()
                .w_full()
                .h(px(4.0))
                .rounded(px(2.0))
                .bg(colors.surface)
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .rounded(px(2.0))
                        .bg(colors.accent),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(-5.0))
                        .left(relative(fraction))
                        .ml(px(-7.0))
                        .size(px(14.0))
                        .rounded_full()
                        .bg(colors.text),
                ),
        )
        .on_click(listen(cx, move |app, event: &ClickEvent, _, _| {
            set_zoom(app, horizontal_fraction(bounds.get(), event.position()));
            app.save_settings();
        }))
        .on_drag(ZoomDrag, |_, _, _, cx| cx.new(|_| DragPreview(None)))
        .on_drag_move(listen(cx, |app, event: &DragMoveEvent<ZoomDrag>, _, _| {
            set_zoom(app, horizontal_fraction(event.bounds, event.event.position));
            app.ui.zoom_dragging = true;
        }));
    div()
        .flex()
        .items_center()
        .gap(sz(12.0))
        .mb(sz(4.0))
        .child(slider)
        .child(label(format!("{:.0}%", zoom * 100.0), 13.0, colors.text))
        .into_any_element()
}

fn set_zoom(app: &mut ViperApp, fraction: f32) {
    let zoom = MIN_ZOOM + fraction * (MAX_ZOOM - MIN_ZOOM);
    app.settings.ui_zoom = ((zoom / ZOOM_STEP).round() * ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM);
}

/// A text field editing one of the dialog's drafts.
fn draft_field(
    app: &mut ViperApp,
    key: &'static str,
    placeholder: &'static str,
    masked: bool,
    draft: fn(&mut PreferencesState) -> &mut String,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> Input {
    let value = draft(&mut app.preferences).clone();
    text_field(
        app,
        format!("{FIELD_PREFIX}{key}"),
        &value,
        FieldOptions {
            placeholder,
            masked,
        },
        window,
        cx,
        move |app, text| text.clone_into(draft(&mut app.preferences)),
    )
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn connections(
    app: &mut ViperApp,
    colors: Palette,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let page = page();
    #[cfg(target_os = "linux")]
    let page = {
        use std::sync::atomic::Ordering;
        let library = &app.library;
        let is_smb = app
            .settings
            .music_path
            .as_deref()
            .is_some_and(crate::features::library::is_smb_path);
        let status = if library.scanning && is_smb {
            let completed = library.progress.completed.load(Ordering::Relaxed);
            let total = library.progress.total.load(Ordering::Relaxed);
            if total == 0 {
                "Scanning share…".to_owned()
            } else {
                format!("Scanning share… {completed} of {total}")
            }
        } else if is_smb {
            format!("Configured · {} tracks indexed", library.model.tracks.len())
        } else {
            "No SMB share configured".to_owned()
        };
        let url = draft_field(
            app,
            "smb-url",
            "smb://server/Music/Albums",
            false,
            |p| &mut p.smb_url_draft,
            window,
            cx,
        );
        let username = draft_field(
            app,
            "smb-username",
            "Optional",
            false,
            |p| &mut p.smb_username,
            window,
            cx,
        );
        let workgroup = draft_field(
            app,
            "smb-workgroup",
            "Optional",
            false,
            |p| &mut p.smb_workgroup,
            window,
            cx,
        );
        let password = draft_field(
            app,
            "smb-password",
            "Optional",
            true,
            |p| &mut p.smb_password,
            window,
            cx,
        );
        let field = |name: &'static str, input: Input| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(sz(4.0))
                .child(name)
                .child(input)
        };
        page.child(section_title("Samba / SMB share", colors))
            .child(format!("Status: {status}"))
            .child(div().mt(sz(8.0)).mb(sz(10.0)).child(
                "Connect directly to a network share. Share details are stored in SQLite, and the password is saved in the operating system's credential store.",
            ))
            .child(field("Share folder URL", url))
            .child(
                div()
                    .flex()
                    .gap(sz(12.0))
                    .mt(sz(8.0))
                    .child(field("Username", username))
                    .child(field("Workgroup", workgroup)),
            )
            .child(div().mt(sz(8.0)).child(field("Password", password)))
            .child(
                div().flex().justify_end().mt(sz(8.0)).child(action_button(
                    "preferences-connect-smb",
                    "Connect and scan",
                    true,
                    colors,
                    click(cx, |app, _, _| app.handle_action(AppAction::ConnectSmbShare)),
                )),
            )
    };
    page.into_any_element()
}

fn services(
    app: &mut ViperApp,
    colors: Palette,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let lastfm_status = app.lastfm_service.status();
    let status = match app.lastfm_settings.username.as_deref() {
        Some(username) if lastfm_status == "Not connected" => format!("Connected as {username}"),
        Some(username) => format!("Connected as {username} · {lastfm_status}"),
        None => lastfm_status.to_owned(),
    };
    let can_finish = app.lastfm_service.can_finish_auth();
    let connected = app.lastfm_settings.session_key.is_some();
    let api_key = draft_field(
        app,
        "lastfm-key",
        "Enter your API key",
        false,
        |p| &mut p.lastfm_api_key_draft,
        window,
        cx,
    );
    let api_secret = draft_field(
        app,
        "lastfm-secret",
        "Enter your API shared secret",
        true,
        |p| &mut p.lastfm_api_secret_draft,
        window,
        cx,
    );
    let button = |id: &'static str, text: &'static str, action: fn() -> AppAction| {
        action_button(
            id,
            text,
            true,
            colors,
            click(cx, move |app, _, _| app.handle_action(action())),
        )
    };
    let mut buttons = vec![
        button("lastfm-save", "Save API credentials", || {
            AppAction::SaveLastFmSettings
        }),
        button("lastfm-connect", "Connect Last.fm", || {
            AppAction::StartLastFmAuth
        }),
    ];
    if can_finish {
        buttons.push(button("lastfm-finish", "Finish sign-in", || {
            AppAction::FinishLastFmAuth
        }));
    }
    if connected {
        buttons.push(button("lastfm-disconnect", "Disconnect", || {
            AppAction::DisconnectLastFm
        }));
    }
    let settings = &app.settings;
    let available = discord::available();
    let presence_enabled = available && settings.discord_rich_presence_enabled;
    let options = settings.discord_presence;
    let statuses = [
        (PresenceStatus::Song, "The song title"),
        (PresenceStatus::Artist, "The artist and album"),
        (PresenceStatus::App, "Viper Music"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (status, text))| {
        let row = choice_row(
            ("presence-status", index),
            text,
            options.status == status,
            "Shown on your profile",
            colors,
            click(cx, move |app, _, _| {
                if app.settings.discord_presence.status != status {
                    app.settings.discord_presence.status = status;
                    app.save_settings();
                }
            }),
        );
        div()
            .when(!presence_enabled, |row| row.opacity(0.5).occlude())
            .child(row)
    });
    let presence_option =
        |id: &'static str, text: &'static str, checked: bool, set: fn(&mut Settings, bool)| {
            checkbox(id, text, checked, presence_enabled, cx, set)
        };
    page()
        .child(section_title("Last.fm scrobbling", colors))
        .child("Send currently playing tracks and scrobbles to your Last.fm account.")
        .child(div().mt(sz(8.0)).child(checkbox(
            "lastfm-scrobbling",
            "Enable Last.fm scrobbling",
            settings.lastfm_scrobbling_enabled,
            true,
            cx,
            |settings, checked| settings.lastfm_scrobbling_enabled = checked,
        )))
        .child(label(status, 11.0, colors.muted).mt(sz(4.0)).mb(sz(12.0)))
        .child("Last.fm API key")
        .child(div().mt(sz(4.0)).mb(sz(8.0)).child(api_key))
        .child("Last.fm shared secret")
        .child(div().mt(sz(4.0)).child(api_secret))
        .child(
            label(
                "Create an API account at last.fm/api/account/create. Credentials and the session key are stored locally.",
                11.0,
                colors.muted,
            )
            .mt(sz(6.0))
            .mb(sz(12.0)),
        )
        .child(div().flex().flex_wrap().gap(sz(8.0)).children(buttons))
        .child(section_title("Discord Rich Presence", colors).mt(sz(28.0)))
        .child("Show the currently playing song as a rich presence on your Discord profile.")
        .when(!available, |page| {
            page.child(
                label("This build doesn't include Discord Rich Presence.", 11.0, colors.muted)
                    .mt(sz(6.0)),
            )
        })
        .child(div().mt(sz(6.0)).child(checkbox(
            "discord-presence",
            "Show currently playing song as Discord status",
            settings.discord_rich_presence_enabled,
            available,
            cx,
            |settings, checked| settings.discord_rich_presence_enabled = checked,
        )))
        .child(
            div()
                .mt(sz(14.0))
                .mb(sz(8.0))
                .when(!presence_enabled, |text| text.text_color(colors.muted))
                .child("Show on Discord as “Listening to …”"),
        )
        .child(div().flex().flex_col().gap(sz(6.0)).children(statuses))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(sz(4.0))
                .mt(sz(12.0))
                .child(presence_option("discord-artist", "Show the artist", options.show_artist, |settings, checked| {
                    settings.discord_presence.show_artist = checked;
                }))
                .child(presence_option("discord-album", "Show the album", options.show_album, |settings, checked| {
                    settings.discord_presence.show_album = checked;
                }))
                .child(presence_option(
                    "discord-progress",
                    "Show the time left in the song",
                    options.show_progress,
                    |settings, checked| settings.discord_presence.show_progress = checked,
                ))
                .child(presence_option(
                    "discord-paused",
                    "Keep showing the song while paused",
                    options.show_when_paused,
                    |settings, checked| settings.discord_presence.show_when_paused = checked,
                )),
        )
        .into_any_element()
}

fn about(app: &ViperApp, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    let data = match Settings::file_path() {
        Ok(path) => {
            let path = path.display().to_string();
            let copied = path.clone();
            div()
                .flex()
                .items_center()
                .gap(sz(6.0))
                .child(
                    label(path, 12.0, colors.text)
                        .font_family("monospace")
                        .min_w_0()
                        .truncate(),
                )
                .child(icon_button(
                    "copy-settings-path",
                    IconName::Copy,
                    "Copy path",
                    colors,
                    click(cx, move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                    }),
                ))
        }
        Err(error) => label(error.to_string(), 13.0, colors.danger),
    };
    page()
        .child(
            div()
                .flex()
                .items_center()
                .gap(sz(10.0))
                .child(icon(IconName::Disc, 40.0, colors.accent))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            label("Viper Music", 20.0, colors.text).font_weight(FontWeight::BOLD),
                        )
                        .child(label(
                            format!("Version {}", env!("CARGO_PKG_VERSION")),
                            12.0,
                            colors.muted,
                        )),
                ),
        )
        .child(
            div()
                .mt(sz(12.0))
                .mb(sz(20.0))
                .child("A fast, album-centric music player for local folders and SMB shares."),
        )
        .child(section_title("Updates", colors))
        .child(updates(&app.preferences.update_state, colors, cx))
        .child(section_title("Data", colors).mt(sz(20.0)))
        .child(data)
        .into_any_element()
}

/// Where updating stands, and the button that moves it on.
fn updates(state: &UpdateState, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    let (text, failed) = match state {
        UpdateState::Idle => ("Not checked yet".to_owned(), false),
        UpdateState::UpToDate => ("Up to date".to_owned(), false),
        UpdateState::Checking => ("Checking for updates…".to_owned(), false),
        UpdateState::Available { version } => (format!("Viper Music {version} is out"), false),
        UpdateState::Downloading {
            version,
            downloaded,
            total,
        } => {
            let progress = total.map_or_else(
                || format!("{:.1} MB", *downloaded as f64 / f64::from(1 << 20)),
                |total| format!("{}%", downloaded * 100 / total.max(1)),
            );
            (
                format!("Downloading Viper Music {version} · {progress}"),
                false,
            )
        }
        UpdateState::Ready { version } => (
            format!("Viper Music {version} is installed. Restart to use it."),
            false,
        ),
        UpdateState::Failed { message } => (message.clone(), true),
    };
    let button = match state {
        UpdateState::Ready { .. } => action_button(
            "restart-to-update",
            "Restart to update",
            true,
            colors,
            click(cx, |app, _, _| {
                app.handle_action(AppAction::RestartToUpdate)
            }),
        ),
        UpdateState::Available { .. } => action_button(
            "download-update",
            "Download",
            true,
            colors,
            click(cx, |_, _, cx| cx.open_url(RELEASES_URL)),
        ),
        UpdateState::Checking | UpdateState::Downloading { .. } => action_button(
            "check-for-updates",
            "Check for updates",
            false,
            colors,
            |_, _, _| {},
        ),
        UpdateState::Idle | UpdateState::UpToDate | UpdateState::Failed { .. } => action_button(
            "check-for-updates",
            "Check for updates",
            true,
            colors,
            click(cx, |app, _, _| {
                app.handle_action(AppAction::CheckForUpdates)
            }),
        ),
    };
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(sz(12.0))
        .child(
            label(
                text,
                12.0,
                if failed { colors.danger } else { colors.muted },
            )
            .min_w_0(),
        )
        .child(button)
        .into_any_element()
}
