use super::state::PreferencesState;
use crate::{
    features::library::state::LibraryFeature,
    platform::{
        persistence::settings::{LastFmSettings, Settings, StartupView},
        presence::lastfm::LastFmService,
    },
    shared::{
        action::AppAction,
        ui::{
            controls::{action_button, close_button, connection_text_edit, modal_frame},
            theme::{self, ThemeId},
        },
    },
    workbench::WorkbenchState,
};
use eframe::egui;

pub(crate) fn show(
    ctx: &egui::Context,
    workbench: &mut WorkbenchState,
    preferences: &mut PreferencesState,
    settings: &mut Settings,
    library: &LibraryFeature,
    lastfm_service: &LastFmService,
    lastfm_settings: &LastFmSettings,
) -> Option<AppAction> {
    let colors = theme::colors(ctx);
    if !workbench.show_preferences {
        return None;
    }
    let mut action = None;

    let screen_size = ctx.screen_rect().size();
    let margin = 24.0_f32.min(screen_size.x * 0.05).min(screen_size.y * 0.05);
    let modal_size = egui::vec2(
        (screen_size.x - margin * 2.0).clamp(0.0, 760.0),
        (screen_size.y - margin * 2.0).max(0.0),
    );
    let content_size = (modal_size - egui::vec2(36.0, 36.0)).max(egui::Vec2::ZERO);
    let mut close_requested = false;

    let response = egui::Modal::new(egui::Id::new("preferences-modal"))
        .frame(modal_frame(colors))
        .backdrop_color(colors.backdrop)
        .show(ctx, |ui| {
            ui.set_min_size(content_size);
            ui.set_max_size(content_size);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Preferences")
                            .size(23.0)
                            .strong()
                            .color(colors.text),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Manage your library, connections, and startup experience",
                        )
                        .size(12.0)
                        .color(colors.muted),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close_requested = close_button(ui, "Close preferences");
                });
            });

            ui.add_space(18.0);
            category_tabs(ui, preferences);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            egui::ScrollArea::vertical()
                .id_salt("preferences-content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match preferences.category.unwrap_or(Category::General) {
                        Category::General => show_general(ui, settings, &mut action),
                        Category::Connections => {
                            show_connections(ui, preferences, settings, library, &mut action)
                        }
                        Category::View => show_view(ui, settings, &mut action),
                        Category::Theme => show_theme(ui, settings, &mut action),
                        Category::Audio => show_audio(ui, settings, &mut action),
                        Category::Services => show_services(
                            ui,
                            preferences,
                            settings,
                            lastfm_service,
                            lastfm_settings,
                            &mut action,
                        ),
                        Category::About => show_about(ui),
                    }
                });
        });

    if close_requested || response.should_close() {
        workbench.show_preferences = false;
    }
    action
}

fn category_tabs(ui: &mut egui::Ui, preferences: &mut PreferencesState) {
    let colors = theme::colors(ui.ctx());
    let mut selected = preferences.category.unwrap_or(Category::General);
    egui::ScrollArea::horizontal()
        .id_salt("preferences-category-tabs")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for category in Category::ALL {
                    let is_selected = selected == category;
                    let response = ui
                        .scope(|ui| {
                            let widgets = &mut ui.visuals_mut().widgets;
                            widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                            widgets.inactive.bg_stroke = egui::Stroke::NONE;
                            widgets.hovered.bg_fill = colors.surface;
                            widgets.hovered.bg_stroke = egui::Stroke::NONE;
                            widgets.active.bg_fill = colors.surface;
                            widgets.active.bg_stroke = egui::Stroke::NONE;
                            ui.spacing_mut().button_padding = egui::vec2(11.0, 7.0);
                            ui.selectable_label(
                                is_selected,
                                egui::RichText::new(category.label()).size(12.0).color(
                                    if is_selected {
                                        colors.text
                                    } else {
                                        colors.muted
                                    },
                                ),
                            )
                        })
                        .inner;
                    if response.clicked() {
                        selected = category;
                    }
                }
            });
        });
    preferences.category = Some(selected);
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

fn show_general(ui: &mut egui::Ui, settings: &mut Settings, action: &mut Option<AppAction>) {
    let colors = theme::colors(ui.ctx());
    ui.set_min_width(420.0);
    ui.add_space(8.0);
    section_title(ui, "Music library");
    ui.label("Choose the folder that contains your music.");
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        if let Some(path) = &settings.music_path {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(path.display().to_string())
                        .size(12.0)
                        .color(ui.visuals().widgets.inactive.text_color()),
                )
                .sense(egui::Sense::hover())
                .truncate(),
            );
        } else {
            ui.label(
                egui::RichText::new("No music folder selected")
                    .size(12.0)
                    .color(ui.visuals().widgets.inactive.text_color()),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = egui::Button::new(
                egui::RichText::new("Choose folder")
                    .size(12.0)
                    .strong()
                    .color(colors.text),
            )
            .fill(colors.surface)
            .stroke(egui::Stroke::new(1.0_f32, colors.border))
            .corner_radius(7.0)
            .min_size(egui::vec2(112.0, 34.0));
            if ui.add(button).clicked() {
                *action = Some(AppAction::ChooseMusicFolder);
            }
        });
    });

    ui.add_space(28.0);
    section_title(ui, "Startup");
    ui.label("Choose which page opens when the player starts.");
    ui.add_space(12.0);

    let startup_view = settings.startup_view;
    let mut new_startup_view = startup_view;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for (page, label) in [
            (StartupView::Home, "Home"),
            (StartupView::Albums, "Albums"),
            (StartupView::Songs, "Songs"),
        ] {
            if choice_row(ui, label, new_startup_view == page, "Current startup page").clicked() {
                new_startup_view = page;
            }
        }
    });
    if new_startup_view != startup_view {
        settings.startup_view = new_startup_view;
        *action = Some(AppAction::SaveSettings);
    }
}

fn show_theme(ui: &mut egui::Ui, settings: &mut Settings, action: &mut Option<AppAction>) {
    ui.add_space(8.0);
    section_title(ui, "Themes");
    ui.label("Choose a theme for the player.");
    ui.add_space(12.0);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for theme_id in ThemeId::ALL {
            if choice_row(
                ui,
                theme_id.label(),
                settings.theme == theme_id,
                "Current theme",
            )
            .clicked()
                && settings.theme != theme_id
            {
                settings.theme = theme_id;
                theme::apply(ui.ctx(), theme_id);
                *action = Some(AppAction::SaveSettings);
            }
        }
    });
}

fn show_audio(ui: &mut egui::Ui, settings: &mut Settings, action: &mut Option<AppAction>) {
    ui.set_min_width(420.0);
    ui.add_space(8.0);
    section_title(ui, "Playback");
    ui.label("Choose how tracks transition during album playback.");
    ui.add_space(10.0);
    if ui
        .checkbox(&mut settings.gapless_playback, "Gapless playback")
        .on_hover_text("Queue the next track before the current track ends.")
        .changed()
    {
        *action = Some(AppAction::SaveSettings);
    }
}

fn choice_row(ui: &mut egui::Ui, label: &str, selected: bool, status: &str) -> egui::Response {
    let colors = theme::colors(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let fill = if response.hovered() {
        colors.hover
    } else if selected {
        colors.selected
    } else {
        colors.surface
    };
    ui.painter().rect(
        rect,
        7.0,
        fill,
        egui::Stroke::new(1.0_f32, colors.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        if selected {
            egui_phosphor::regular::RADIO_BUTTON
        } else {
            egui_phosphor::regular::CIRCLE
        },
        theme::icon_font(16.0),
        if selected {
            colors.accent
        } else {
            colors.subtle
        },
    );
    // Reserve status space only when there is room; long names are clipped to the row.
    let show_status = selected && rect.width() >= 390.0;
    let label_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 36.0, rect.top()),
        egui::pos2(
            rect.right() - if show_status { 150.0 } else { 12.0 },
            rect.bottom(),
        ),
    );
    ui.painter().with_clip_rect(label_rect).text(
        egui::pos2(label_rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        colors.text,
    );
    if show_status {
        ui.painter().text(
            egui::pos2(rect.right() - 12.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            status,
            egui::FontId::proportional(11.0),
            colors.muted,
        );
    }
    response
}

fn show_view(ui: &mut egui::Ui, settings: &mut Settings, action: &mut Option<AppAction>) {
    let colors = theme::colors(ui.ctx());
    ui.add_space(8.0);
    section_title(ui, "Sidebar");
    if ui
        .checkbox(&mut settings.compact_sidebar, "Compact sidebar")
        .changed()
    {
        *action = Some(AppAction::SaveSettings);
    }
    ui.label(
        egui::RichText::new(
            "Show only icons in a narrow sidebar. Hover over an icon to see its name.",
        )
        .size(12.0)
        .color(colors.muted),
    );
    ui.add_space(20.0);
    section_title(ui, "Interface scale");
    let mut zoom = ui.ctx().zoom_factor();
    if ui
        .add(
            egui::Slider::new(&mut zoom, 0.75..=2.0)
                .step_by(0.05)
                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
        )
        .changed()
    {
        ui.ctx().set_zoom_factor(zoom);
    }
    ui.label(
        egui::RichText::new("You can also press Ctrl + and Ctrl − to zoom, and Ctrl 0 to reset.")
            .size(12.0)
            .color(colors.muted),
    );
    ui.add_space(20.0);
    section_title(ui, "Albums");
    if ui
        .checkbox(
            &mut settings.album_format_badges,
            "Show album format badges",
        )
        .changed()
    {
        *action = Some(AppAction::SaveSettings);
    }
    ui.label(
        egui::RichText::new("Show the audio file format on album artwork.")
            .size(12.0)
            .color(colors.muted),
    );
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn show_connections(
    ui: &mut egui::Ui,
    preferences: &mut PreferencesState,
    settings: &mut Settings,
    library: &LibraryFeature,
    action: &mut Option<AppAction>,
) {
    ui.set_min_width(420.0);
    ui.add_space(8.0);
    #[cfg(target_os = "linux")]
    {
        section_title(ui, "Samba / SMB share");
        let is_smb = settings
            .music_path
            .as_deref()
            .is_some_and(crate::features::library::is_smb_path);
        let status = if library.scanning && is_smb {
            let completed = library
                .progress
                .completed
                .load(std::sync::atomic::Ordering::Relaxed);
            let total = library
                .progress
                .total
                .load(std::sync::atomic::Ordering::Relaxed);
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
        ui.label(format!("Status: {status}"));
        ui.add_space(8.0);
        ui.label(
            "Connect directly to a network share. Share details are stored in SQLite, and the password is saved in the operating system's credential store.",
        );
        ui.add_space(10.0);
        ui.label("Share folder URL");
        connection_text_edit(
            ui,
            &mut preferences.smb_url_draft,
            "smb://server/Music/Albums",
            false,
        );
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            columns[0].vertical(|ui| {
                ui.label("Username");
                connection_text_edit(ui, &mut preferences.smb_username, "Optional", false);
            });
            columns[1].vertical(|ui| {
                ui.label("Workgroup");
                connection_text_edit(ui, &mut preferences.smb_workgroup, "Optional", false);
            });
        });
        ui.add_space(8.0);
        ui.label("Password");
        connection_text_edit(ui, &mut preferences.smb_password, "Optional", true);
        ui.add_space(8.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 38.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if action_button(ui, "Connect and scan", true).clicked() {
                    *action = Some(AppAction::ConnectSmbShare);
                }
            },
        );
        ui.add_space(28.0);
    }
}

fn show_services(
    ui: &mut egui::Ui,
    preferences: &mut PreferencesState,
    settings: &mut Settings,
    lastfm_service: &LastFmService,
    lastfm_settings: &LastFmSettings,
    action: &mut Option<AppAction>,
) {
    let lastfm_status = lastfm_service.status();
    let colors = theme::colors(ui.ctx());
    ui.set_min_width(420.0);
    ui.add_space(8.0);
    section_title(ui, "Last.fm scrobbling");
    ui.label("Send currently playing tracks and scrobbles to your Last.fm account.");
    ui.add_space(8.0);
    if ui
        .checkbox(
            &mut settings.lastfm_scrobbling_enabled,
            "Enable Last.fm scrobbling",
        )
        .changed()
    {
        *action = Some(AppAction::SaveSettings);
    }
    let status = match lastfm_settings.username.as_deref() {
        Some(username) if lastfm_status == "Not connected" => {
            format!("Connected as {username}")
        }
        Some(username) => format!("Connected as {username} · {lastfm_status}"),
        None => lastfm_status.to_owned(),
    };
    ui.label(egui::RichText::new(status).size(11.0).color(colors.muted));
    ui.add_space(12.0);
    ui.label("Last.fm API key");
    connection_text_edit(
        ui,
        &mut preferences.lastfm_api_key_draft,
        "Enter your API key",
        false,
    );
    ui.add_space(8.0);
    ui.label("Last.fm shared secret");
    connection_text_edit(
        ui,
        &mut preferences.lastfm_api_secret_draft,
        "Enter your API shared secret",
        true,
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Create an API account at last.fm/api/account/create. Credentials and the session key are stored locally.")
            .size(11.0)
            .color(colors.muted),
    );
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        if action_button(ui, "Save API credentials", true).clicked() {
            *action = Some(AppAction::SaveLastFmSettings);
        }
        if action_button(ui, "Connect Last.fm", true).clicked() {
            *action = Some(AppAction::StartLastFmAuth);
        }
        if lastfm_service.can_finish_auth() && action_button(ui, "Finish sign-in", true).clicked() {
            *action = Some(AppAction::FinishLastFmAuth);
        }
        if lastfm_settings.session_key.is_some() && action_button(ui, "Disconnect", true).clicked()
        {
            *action = Some(AppAction::DisconnectLastFm);
        }
    });

    ui.add_space(28.0);
    section_title(ui, "Discord Rich Presence");
    ui.label("Show the currently playing song as a rich presence on your Discord profile.");
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x += 6.0;
        if ui
            .checkbox(
                &mut settings.discord_rich_presence_enabled,
                "Show currently playing song as Discord status",
            )
            .changed()
        {
            *action = Some(AppAction::SaveSettings);
        }
    });
    ui.add_space(18.0);
    ui.label("Discord Application ID");
    connection_text_edit(
        ui,
        &mut preferences.discord_application_id_draft,
        "Enter your Discord application ID",
        false,
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Create an application in the Discord Developer Portal and copy its Application ID here.")
            .size(11.0)
            .color(colors.muted),
    );
    ui.add_space(12.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 38.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if action_button(ui, "Save", true).clicked() {
                settings.discord_application_id =
                    match preferences.discord_application_id_draft.trim() {
                        "" => None,
                        id => Some(id.to_owned()),
                    };
                *action = Some(AppAction::SaveDiscordApplicationId);
            }
        },
    );
}

const REPOSITORY_URL: &str = "https://github.com/PolarisPyra/ViperMusic";

fn show_about(ui: &mut egui::Ui) {
    let colors = theme::colors(ui.ctx());
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::DISC)
                .font(theme::icon_font(40.0))
                .color(colors.accent),
        );
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Viper Music")
                    .size(20.0)
                    .strong()
                    .color(colors.text),
            );
            ui.label(
                egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .size(12.0)
                    .color(colors.muted),
            );
        });
    });
    ui.add_space(12.0);
    ui.label("A fast, album-centric music player for local folders and SMB shares.");
    ui.add_space(20.0);
    section_title(ui, "Links");
    ui.hyperlink_to("Source code", REPOSITORY_URL);
    ui.hyperlink_to("Report a problem", format!("{REPOSITORY_URL}/issues"));
    ui.add_space(20.0);
    section_title(ui, "Data");
    ui.label("Preferences and library state are stored in:");
    match Settings::file_path() {
        Ok(path) => {
            let path = path.display().to_string();
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(&path).monospace().size(12.0)).truncate(),
                );
                if ui
                    .small_button(egui_phosphor::regular::COPY)
                    .on_hover_text("Copy path")
                    .clicked()
                {
                    ui.ctx().copy_text(path);
                }
            });
        }
        Err(error) => {
            ui.label(egui::RichText::new(error.to_string()).color(colors.danger));
        }
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "Passwords, API keys, and session tokens are kept in the operating system's credential store.",
        )
        .size(12.0)
        .color(colors.muted),
    );
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.label(
        eframe::egui::RichText::new(title)
            .size(15.0)
            .strong()
            .color(ui.visuals().widgets.active.text_color()),
    );
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(8.0);
}
