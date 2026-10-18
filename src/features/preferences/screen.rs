use crate::{platform::persistence::settings::Settings, workbench::WorkbenchState};

use super::state::PreferencesState;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreferencesAction {
    ChooseMusicFolder,
    ConnectSmbShare,
    SaveSettings,
    SaveLastFmSettings,
    StartLastFmAuth,
    FinishLastFmAuth,
    DisconnectLastFm,
}

pub(crate) fn show(
    ctx: &egui::Context,
    workbench: &mut WorkbenchState,
    preferences: &mut PreferencesState,
    settings: &mut Settings,
    library: &crate::features::library::state::LibraryFeature,
    lastfm_status: &str,
    lastfm_connected: bool,
    lastfm_username: Option<&str>,
    lastfm_can_finish_auth: bool,
    error: &mut Option<String>,
) -> Option<PreferencesAction> {
    let colors = crate::shared::ui::theme::colors(ctx);
    if !workbench.show_preferences {
        return None;
    }
    let mut action = None;

    let screen_size = ctx.screen_rect().size();
    let margin = 24.0_f32.min(screen_size.x * 0.05).min(screen_size.y * 0.05);
    let modal_size = egui::vec2(
        (screen_size.x - margin * 2.0).min(760.0).max(0.0),
        (screen_size.y - margin * 2.0).max(0.0),
    );
    let content_size = (modal_size - egui::vec2(36.0, 36.0)).max(egui::Vec2::ZERO);
    let mut close_requested = false;

    let response = egui::Modal::new(egui::Id::new("preferences-modal"))
        .frame(
            egui::Frame::new()
                .fill(colors.panel)
                .stroke(egui::Stroke::new(1.0_f32, colors.border))
                .corner_radius(14.0)
                .inner_margin(egui::Margin::symmetric(18, 18)),
        )
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
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 8.0, colors.hover);
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor::regular::X,
                        egui::FontId::new(16.0, egui::FontFamily::Name("phosphor".into())),
                        if response.hovered() {
                            colors.text
                        } else {
                            colors.muted
                        },
                    );
                    if response.on_hover_text("Close preferences").clicked() {
                        close_requested = true;
                    }
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
                        Category::General => show_general(ui, settings, error, &mut action),
                        Category::Connections => {
                            show_connections(ui, preferences, settings, library, error, &mut action)
                        }
                        Category::View => show_view(ui, settings, &mut action),
                        Category::Theme => show_theme(ui, settings, &mut action),
                        Category::Audio => show_audio(ui, settings, &mut action),
                        Category::Services => show_services(
                            ui,
                            preferences,
                            settings,
                            lastfm_status,
                            lastfm_connected,
                            lastfm_username,
                            lastfm_can_finish_auth,
                            &mut action,
                        ),
                        Category::About => {}
                    }
                });
        });

    if close_requested || response.should_close() {
        workbench.show_preferences = false;
    }
    action
}

fn category_tabs(ui: &mut egui::Ui, preferences: &mut PreferencesState) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
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

fn show_general(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    error: &mut Option<String>,
    action: &mut Option<PreferencesAction>,
) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
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
                *action = Some(PreferencesAction::ChooseMusicFolder);
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
            (
                crate::platform::persistence::settings::StartupView::Home,
                "Home",
            ),
            (
                crate::platform::persistence::settings::StartupView::Albums,
                "Albums",
            ),
        ] {
            if choice_row(ui, label, new_startup_view == page, "Current startup page").clicked() {
                new_startup_view = page;
            }
        }
    });
    if new_startup_view != startup_view {
        settings.startup_view = new_startup_view;
        if let Err(save_error) = settings.save() {
            *error = Some(format!("Could not save settings: {save_error}"));
        }
    }
}

fn show_theme(ui: &mut egui::Ui, settings: &mut Settings, action: &mut Option<PreferencesAction>) {
    ui.add_space(8.0);
    section_title(ui, "Themes");
    ui.label("Choose a theme for the player.");
    ui.add_space(12.0);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for theme in crate::shared::ui::theme::ThemeId::ALL {
            if choice_row(ui, theme.label(), settings.theme == theme, "Current theme").clicked()
                && settings.theme != theme
            {
                settings.theme = theme;
                crate::shared::ui::theme::apply(ui.ctx(), theme);
                *action = Some(PreferencesAction::SaveSettings);
            }
        }
    });
}

fn show_audio(ui: &mut egui::Ui, settings: &mut Settings, action: &mut Option<PreferencesAction>) {
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
        *action = Some(PreferencesAction::SaveSettings);
    }
}

fn choice_row(ui: &mut egui::Ui, label: &str, selected: bool, status: &str) -> egui::Response {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
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
        egui::FontId::new(16.0, egui::FontFamily::Name("phosphor".into())),
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

fn show_view(ui: &mut egui::Ui, settings: &mut Settings, action: &mut Option<PreferencesAction>) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    ui.add_space(8.0);
    section_title(ui, "Sidebar");
    if ui
        .checkbox(&mut settings.compact_sidebar, "Compact sidebar")
        .changed()
    {
        *action = Some(PreferencesAction::SaveSettings);
    }
    ui.label(
        egui::RichText::new(
            "Show only icons in a narrow sidebar. Hover over an icon to see its name.",
        )
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
        *action = Some(PreferencesAction::SaveSettings);
    }
    ui.label(
        egui::RichText::new("Show the audio file format on album artwork.")
            .size(12.0)
            .color(colors.muted),
    );
}

fn show_connections(
    ui: &mut egui::Ui,
    preferences: &mut PreferencesState,
    settings: &mut Settings,
    library: &crate::features::library::state::LibraryFeature,
    _error: &mut Option<String>,
    action: &mut Option<PreferencesAction>,
) {
    ui.set_min_width(420.0);
    ui.add_space(8.0);
    #[cfg(target_os = "linux")]
    {
        section_title(ui, "Samba / SMB share");
        let is_smb = settings
            .music_path
            .as_deref()
            .is_some_and(crate::features::library::smb::is_smb_path);
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
            "Connect directly to a network share. Connection details are stored in the local SQLite database for future access.",
        );
        ui.add_space(10.0);
        ui.label("Share folder URL");
        crate::shared::ui::controls::connection_text_edit(
            ui,
            &mut preferences.smb_url_draft,
            "smb://server/Music/Albums",
            false,
        );
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            columns[0].vertical(|ui| {
                ui.label("Username");
                crate::shared::ui::controls::connection_text_edit(
                    ui,
                    &mut preferences.smb_username,
                    "Optional",
                    false,
                );
            });
            columns[1].vertical(|ui| {
                ui.label("Workgroup");
                crate::shared::ui::controls::connection_text_edit(
                    ui,
                    &mut preferences.smb_workgroup,
                    "Optional",
                    false,
                );
            });
        });
        ui.add_space(8.0);
        ui.label("Password");
        crate::shared::ui::controls::connection_text_edit(
            ui,
            &mut preferences.smb_password,
            "Optional",
            true,
        );
        ui.add_space(8.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 38.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if crate::shared::ui::controls::action_button(ui, "Connect and scan", true)
                    .clicked()
                {
                    *action = Some(PreferencesAction::ConnectSmbShare);
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
    lastfm_status: &str,
    lastfm_connected: bool,
    lastfm_username: Option<&str>,
    lastfm_can_finish_auth: bool,
    action: &mut Option<PreferencesAction>,
) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
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
        *action = Some(PreferencesAction::SaveSettings);
    }
    let status = match lastfm_username {
        Some(username) if lastfm_status == "Not connected" => {
            format!("Connected as {username}")
        }
        Some(username) => format!("Connected as {username} · {lastfm_status}"),
        None => lastfm_status.to_owned(),
    };
    ui.label(egui::RichText::new(status).size(11.0).color(colors.muted));
    ui.add_space(12.0);
    ui.label("Last.fm API key");
    crate::shared::ui::controls::connection_text_edit(
        ui,
        &mut preferences.lastfm_api_key_draft,
        "Enter your API key",
        false,
    );
    ui.add_space(8.0);
    ui.label("Last.fm shared secret");
    crate::shared::ui::controls::connection_text_edit(
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
        if crate::shared::ui::controls::action_button(ui, "Save API credentials", true).clicked() {
            *action = Some(PreferencesAction::SaveLastFmSettings);
        }
        if crate::shared::ui::controls::action_button(ui, "Connect Last.fm", true).clicked() {
            *action = Some(PreferencesAction::StartLastFmAuth);
        }
        if lastfm_can_finish_auth
            && crate::shared::ui::controls::action_button(ui, "Finish sign-in", true).clicked()
        {
            *action = Some(PreferencesAction::FinishLastFmAuth);
        }
        if lastfm_connected
            && crate::shared::ui::controls::action_button(ui, "Disconnect", true).clicked()
        {
            *action = Some(PreferencesAction::DisconnectLastFm);
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
            *action = Some(PreferencesAction::SaveSettings);
        }
    });
    ui.add_space(18.0);
    ui.label("Discord Application ID");
    crate::shared::ui::controls::connection_text_edit(
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
            if crate::shared::ui::controls::action_button(ui, "Save", true).clicked() {
                settings.discord_application_id =
                    match preferences.discord_application_id_draft.trim() {
                        "" => None,
                        id => Some(id.to_owned()),
                    };
                *action = Some(PreferencesAction::SaveSettings);
            }
        },
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
