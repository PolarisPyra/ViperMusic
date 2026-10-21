use crate::{
    features::{
        library::{state::LibraryFeature, Album},
        playback::Playback,
    },
    platform::persistence::settings::{AlbumSort, Settings},
    shared::{
        action::AppAction,
        artwork::ArtworkCache,
        ui::theme::{self, icon_font},
    },
    workbench::{Page, WorkbenchState},
};
use eframe::egui;
use std::{cmp::Reverse, collections::BTreeMap};

struct HomeView<'a> {
    workbench: &'a mut WorkbenchState,
    library: &'a mut LibraryFeature,
    playback: &'a mut Playback,
    settings: &'a mut Settings,
    artwork_cache: &'a mut ArtworkCache,
    action: Option<AppAction>,
}

/// Albums shown per Home shelf; "See all" opens the full list in the album browser.
const SHELF_LIMIT: usize = 20;

impl HomeView<'_> {
    fn album_texture(
        &mut self,
        ctx: &egui::Context,
        album_index: usize,
    ) -> Option<egui::TextureHandle> {
        let bytes = self.library.model.albums.get(album_index)?.art.as_ref()?;
        self.artwork_cache.texture(ctx, album_index, bytes)
    }
}

fn home_action_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let colors = theme::colors(ui.ctx());
    let button = egui::Button::new(
        egui::RichText::new(label)
            .size(12.0)
            .strong()
            .color(colors.text),
    )
    .fill(colors.surface)
    .stroke(egui::Stroke::new(1.0_f32, colors.border))
    .corner_radius(7.0)
    .min_size(egui::vec2(0.0, 34.0));
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(12.0, 7.0);
        ui.add(button)
    })
    .inner
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    workbench: &mut WorkbenchState,
    library: &mut LibraryFeature,
    playback: &mut Playback,
    settings: &mut Settings,
    artwork_cache: &mut ArtworkCache,
) -> Option<AppAction> {
    let ctx = &ui.ctx().clone();
    let colors = theme::colors(ctx);
    let mut view = HomeView {
        workbench,
        library,
        playback,
        settings,
        artwork_cache,
        action: None,
    };
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(colors.canvas)
                .inner_margin(egui::Margin::symmetric(24, 0)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("home-content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(24.0);
                    ui.label(
                        egui::RichText::new("Your library")
                            .size(30.0)
                            .strong()
                            .color(colors.text),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Pick up your current track or browse albums.")
                            .size(14.0)
                            .color(colors.muted),
                    );
                    ui.add_space(24.0);

                    if view.settings.music_path.is_none() {
                        onboarding(ui, &mut view);
                        return;
                    }

                    let filtered = view.library.filtered_album_indices(&*view.settings);
                    if !view.library.view.search.trim().is_empty() && filtered.is_empty() {
                        empty_state(
                            ui,
                            "No albums found",
                            "Try another search, or clear it to see your library.",
                            "Clear search",
                            |view| view.library.view.search.clear(),
                            &mut view,
                        );
                        return;
                    }

                    if view.library.model.albums.is_empty() {
                        if view.library.scanning {
                            empty_state(
                                ui,
                                "Finding your music",
                                "Your albums will show up here as the folder is scanned.",
                                "Browse albums",
                                |view| view.workbench.page = Page::Albums,
                                &mut view,
                            );
                        } else {
                            empty_state(
                                ui,
                                "Your library is empty",
                                "Choose a folder with supported audio files to get started.",
                                "Choose Music Folder",
                                |view| view.action = Some(AppAction::ChooseMusicFolder),
                                &mut view,
                            );
                        }
                        return;
                    }

                    let recently_added = albums_by_timestamp(
                        &filtered,
                        &view.library.model.albums,
                        &view.settings.album_added,
                    );
                    let recently_played = albums_by_timestamp(
                        &filtered,
                        &view.library.model.albums,
                        &view.settings.album_last_played,
                    );

                    let albums = &view.library.model.albums;
                    let mut favorites: Vec<_> = filtered
                        .iter()
                        .copied()
                        .filter(|&index| {
                            albums.get(index).is_some_and(|album| {
                                view.settings.favorite_albums.contains(&album.key)
                            })
                        })
                        .collect();
                    favorites.sort_by_cached_key(|&index| {
                        let album = &albums[index];
                        (album.title.to_lowercase(), album.artist.to_lowercase())
                    });

                    show_continue_listening(ui, &mut view);
                    if !recently_added.is_empty() {
                        album_section(
                            ui,
                            ctx,
                            &mut view,
                            "Recently added",
                            &recently_added,
                            AlbumSort::RecentlyAdded,
                        );
                    }
                    if !recently_played.is_empty() {
                        album_section(
                            ui,
                            ctx,
                            &mut view,
                            "Recently played",
                            &recently_played,
                            AlbumSort::RecentlyPlayed,
                        );
                    }
                    if favorites.is_empty() {
                        section_heading(ui, "Favorites");
                        ui.label(
                            egui::RichText::new(
                                "Browse albums and mark a favorite to keep it close at hand.",
                            )
                            .size(13.0)
                            .color(colors.muted),
                        );
                        ui.add_space(10.0);
                        if home_action_button(ui, "Browse albums").clicked() {
                            view.workbench.page = Page::Albums;
                        }
                    } else {
                        album_section(
                            ui,
                            ctx,
                            &mut view,
                            "Favorites",
                            &favorites,
                            AlbumSort::Favorited,
                        );
                    }
                    ui.add_space(24.0);
                });
        });
    view.action
}

fn albums_by_timestamp(
    indices: &[usize],
    albums: &[Album],
    timestamps: &BTreeMap<String, u64>,
) -> Vec<usize> {
    let mut albums_with_timestamps: Vec<_> = indices
        .iter()
        .filter_map(|&index| Some((index, *timestamps.get(&albums.get(index)?.key)?)))
        .collect();

    albums_with_timestamps.sort_by_key(|&(_, timestamp)| Reverse(timestamp));
    albums_with_timestamps
        .into_iter()
        .map(|(index, _)| index)
        .collect()
}

fn onboarding(ui: &mut egui::Ui, view: &mut HomeView<'_>) {
    let colors = theme::colors(ui.ctx());
    egui::Frame::new()
        .fill(colors.panel)
        .corner_radius(14.0)
        .inner_margin(egui::Margin::same(24))
        .show(ui, |ui| {
            ui.set_max_width(720.0);
            ui.set_min_height(185.0);
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(72.0, 72.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 14.0, colors.selected);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    egui_phosphor::regular::MUSIC_NOTE,
                    icon_font(34.0),
                    colors.accent,
                );
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Start with your music folder")
                            .size(20.0)
                            .strong()
                            .color(colors.text),
                    );
                    ui.add_space(7.0);
                    ui.label(
                        egui::RichText::new(
                            "Choose where your music is stored. Your albums and artwork will appear here.",
                        )
                        .size(13.0)
                        .color(colors.muted),
                    );
                    ui.add_space(14.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Choose Music Folder")
                                    .strong()
                                    .color(colors.on_accent),
                            )
                            .fill(colors.accent)
                            .corner_radius(8.0)
                            .min_size(egui::vec2(190.0, 38.0)),
                        )
                        .clicked()
                    {
                        view.action = Some(AppAction::ChooseMusicFolder);
                    }
                });
            });
        });
}

fn empty_state(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    action: &str,
    on_click: impl FnOnce(&mut HomeView<'_>),
    view: &mut HomeView<'_>,
) {
    let colors = theme::colors(ui.ctx());
    egui::Frame::new()
        .fill(colors.panel)
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(24))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(19.0)
                    .strong()
                    .color(colors.text),
            );
            ui.add_space(5.0);
            ui.label(
                egui::RichText::new(description)
                    .size(13.0)
                    .color(colors.muted),
            );
            ui.add_space(12.0);
            if home_action_button(ui, action).clicked() {
                on_click(view);
            }
        });
}

fn show_continue_listening(ui: &mut egui::Ui, view: &mut HomeView<'_>) {
    let colors = theme::colors(ui.ctx());
    section_heading(ui, "Continue listening");
    if let Some(track_index) = view.playback.current {
        let Some(track) = view.library.model.tracks.get(track_index) else {
            return;
        };
        let title = track.title.clone();
        let subtitle = track.artist.clone();
        let album_index = view
            .library
            .model
            .track_album
            .get(track_index)
            .copied()
            .flatten()
            .map(crate::features::library::AlbumIndex::get);
        let subtitle = format!(
            "{subtitle} · {}",
            album_index
                .and_then(|index| view.library.model.albums.get(index))
                .map_or("", |album| album.title.as_str())
        );

        egui::Frame::new()
            .fill(colors.panel)
            .corner_radius(12.0)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(index) = album_index {
                        if let Some(texture) = view.album_texture(ui.ctx(), index) {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(84.0, 84.0), egui::Sense::hover());
                            ui.painter().image(
                                texture.id(),
                                rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .size(17.0)
                                .strong()
                                .color(colors.text),
                        );
                        ui.label(egui::RichText::new(subtitle).size(13.0).color(colors.muted));
                        ui.add_space(8.0);
                        let label = if view.playback.is_paused() {
                            "Resume"
                        } else {
                            "Pause"
                        };
                        if home_action_button(ui, label).clicked() {
                            view.playback.toggle_pause();
                        }
                    });
                });
            });
    } else {
        ui.label(
            egui::RichText::new("Nothing is playing. Browse albums to start playback.")
                .size(13.0)
                .color(colors.muted),
        );
        ui.add_space(8.0);
        if home_action_button(ui, "Browse albums").clicked() {
            view.workbench.page = Page::Albums;
        }
    }
    ui.add_space(25.0);
}

/// Draw a horizontal shelf of up to [`SHELF_LIMIT`] albums.
///
/// # Arguments
/// * `see_all` - Album browser sort applied, descending, when the user asks to see every album.
fn album_section(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    view: &mut HomeView<'_>,
    title: &str,
    indices: &[usize],
    see_all: AlbumSort,
) {
    let colors = theme::colors(ui.ctx());
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(19.0)
                .strong()
                .color(colors.text),
        );
        if indices.len() > SHELF_LIMIT {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let link = ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("See all {}", indices.len()))
                            .size(12.0)
                            .color(colors.accent),
                    )
                    .sense(egui::Sense::click()),
                );
                if link.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if link.clicked() {
                    view.settings.album_sort = see_all;
                    view.settings.sort_ascending = false;
                    view.library.view.album_sort_dirty = true;
                    view.workbench.page = Page::Albums;
                    view.workbench.show_album_details = false;
                    view.action = Some(AppAction::SaveSettings);
                }
            });
        }
    });
    ui.add_space(11.0);
    let indices = &indices[..indices.len().min(SHELF_LIMIT)];
    let output = egui::ScrollArea::horizontal()
        .id_salt(("home-albums", title))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = CARD_GAP;
                for &index in indices {
                    album_card(ui, ctx, view, index);
                }
            });
            // Keep the horizontal scrollbar clear of the album titles.
            ui.add_space(12.0);
        });
    scroll_shelf_with_wheel(ui, output);
    ui.add_space(14.0);
}

const CARD_ART: f32 = 144.0;
const CARD_GAP: f32 = 16.0;

/// Draw one album card; click opens the album and double-click plays it.
fn album_card(ui: &mut egui::Ui, ctx: &egui::Context, view: &mut HomeView<'_>, index: usize) {
    let colors = theme::colors(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(CARD_ART, CARD_ART + 46.0), egui::Sense::click());
    // Off-screen cards only reserve space, so shelves stay cheap to draw and never request
    // artwork decoding for covers the user cannot see.
    if !ui.is_rect_visible(rect) {
        return;
    }
    let texture = view.album_texture(ctx, index);
    let Some(album) = view.library.model.albums.get(index) else {
        return;
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("{} by {}", album.title, album.artist),
        )
    });
    let art_rect = egui::Rect::from_min_size(rect.min, egui::vec2(CARD_ART, CARD_ART));
    if let Some(texture) = texture {
        ui.painter().image(
            texture.id(),
            art_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        ui.painter().rect_filled(art_rect, 7.0, colors.surface);
        ui.painter().text(
            art_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::DISC,
            icon_font(32.0),
            colors.muted,
        );
    }
    if response.hovered() {
        ui.painter().rect_stroke(
            art_rect,
            4.0,
            egui::Stroke::new(2.0_f32, colors.accent),
            egui::StrokeKind::Inside,
        );
    }
    for (text, y, size, color) in [
        (&album.title, CARD_ART + 9.0, 13.0, colors.text),
        (&album.artist, CARD_ART + 28.0, 11.0, colors.muted),
    ] {
        let mut job = egui::text::LayoutJob::simple_singleline(
            text.clone(),
            egui::FontId::proportional(size),
            color,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(CARD_ART);
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        ui.painter()
            .galley(rect.min + egui::vec2(0.0, y), galley, color);
    }
    let response = response.on_hover_text("Double-click to play");
    if response.double_clicked() {
        if let Some(album) = view.library.model.albums.get(index) {
            view.playback.play_album(album, &view.library.model.tracks);
        }
    } else if response.clicked() {
        view.workbench.open_album(&mut view.library.view, index);
        ctx.request_repaint();
    }
}

/// Scroll a shelf horizontally with the vertical mouse wheel while the pointer is over it.
///
/// The wheel falls through to the page once the shelf reaches either end.
fn scroll_shelf_with_wheel(ui: &egui::Ui, output: egui::scroll_area::ScrollAreaOutput<()>) {
    let hovered = ui
        .ctx()
        .pointer_hover_pos()
        .is_some_and(|pointer| output.inner_rect.contains(pointer));
    let delta = ui.input(|input| input.smooth_scroll_delta.y);
    if !hovered || delta == 0.0 {
        return;
    }
    let mut state = output.state;
    let max_offset = (output.content_size.x - output.inner_rect.width()).max(0.0);
    let offset = (state.offset.x - delta).clamp(0.0, max_offset);
    if offset != state.offset.x {
        state.offset.x = offset;
        state.store(ui.ctx(), output.id);
        ui.input_mut(|input| input.smooth_scroll_delta.y = 0.0);
        ui.ctx().request_repaint();
    }
}

fn section_heading(ui: &mut egui::Ui, title: &str) {
    let colors = theme::colors(ui.ctx());
    ui.add_space(5.0);
    ui.label(
        egui::RichText::new(title)
            .size(19.0)
            .strong()
            .color(colors.text),
    );
    ui.add_space(11.0);
}
