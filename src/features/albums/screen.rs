use super::{grid, list, toolbar};
use crate::{
    features::{
        library::{state::LibraryFeature, Album, Library},
        playback::Playback,
    },
    platform::{
        desktop,
        persistence::settings::{AlbumLayout, Settings},
    },
    shared::{
        artwork::ArtworkCache,
        ui::{controls::menu_entry, theme, toasts::Toasts},
    },
    workbench::WorkbenchState,
};
use eframe::egui;
use std::borrow::Cow;

pub(crate) struct AlbumScreen<'a> {
    pub(crate) workbench: &'a mut WorkbenchState,
    pub(crate) library: &'a mut LibraryFeature,
    pub(crate) playback: &'a mut Playback,
    pub(crate) settings: &'a mut Settings,
    pub(crate) artwork_cache: &'a mut ArtworkCache,
    pub(crate) toasts: &'a mut Toasts,
}

/// Describe an album card or row to assistive technology.
pub(super) fn describe_album(response: &egui::Response, album: &Album, selected: bool) {
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            selected,
            format!(
                "{} by {}, {} tracks",
                album.title,
                album.artist,
                album.tracks.len()
            ),
        )
    });
}

/// Return the shared file format of an album's tracks, `MIXED`, or `OTHER`.
pub(super) fn album_format_label(library: &Library, album: &Album) -> Cow<'static, str> {
    let mut format: Option<&str> = None;
    for track in album
        .tracks
        .iter()
        .filter_map(|&index| library.tracks.get(index))
    {
        let extension = track
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("other");
        match format {
            Some(first) if !first.eq_ignore_ascii_case(extension) => return Cow::Borrowed("MIXED"),
            Some(_) => {}
            None => format = Some(extension),
        }
    }
    format.map_or(Cow::Borrowed("OTHER"), |extension| {
        Cow::Owned(extension.to_ascii_uppercase())
    })
}

pub(super) fn paint_format_badge(ui: &egui::Ui, cover: egui::Rect, label: Cow<'static, str>) {
    let colors = theme::colors(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        label.into_owned(),
        egui::FontId::proportional(11.0),
        colors.text,
    );
    let badge_size = galley.size() + egui::vec2(14.0, 8.0);
    let badge = egui::Rect::from_min_size(
        egui::pos2(cover.left() + 5.0, cover.bottom() - badge_size.y - 5.0),
        badge_size,
    );
    ui.painter().rect_filled(badge, 3.0, colors.surface);
    ui.painter()
        .galley(badge.center() - galley.size() * 0.5, galley, colors.text);
}

impl AlbumScreen<'_> {
    pub(crate) fn album_texture(
        &mut self,
        ctx: &egui::Context,
        album_index: usize,
    ) -> Option<egui::TextureHandle> {
        let bytes = self.library.model.albums.get(album_index)?.art.as_ref()?;
        self.artwork_cache.texture(ctx, album_index, bytes)
    }

    pub(crate) fn album_texture_opacity(&self, ctx: &egui::Context, album_index: usize) -> f32 {
        self.artwork_cache.texture_opacity(ctx, album_index)
    }
}

pub(crate) fn show(ctx: &egui::Context, mut screen: AlbumScreen<'_>) {
    let colors = theme::colors(ctx);
    toolbar::show(
        ctx,
        screen.settings,
        &mut screen.library.view,
        screen.toasts,
    );
    let albums = screen.library.filtered_album_indices(screen.settings);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(colors.canvas).inner_margin(0))
        .show(ctx, |ui| show_grid(ctx, ui, &mut screen, &albums));
}

fn show_grid(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    screen: &mut AlbumScreen<'_>,
    albums: &[usize],
) {
    let background_response = ui.interact(
        ui.max_rect(),
        ui.id().with("album-panel-background"),
        egui::Sense::click(),
    );
    ui.spacing_mut().scroll.bar_outer_margin = 10.0;

    let layout = screen.settings.album_layout;
    let width = ui.available_width();
    if layout == AlbumLayout::List {
        list::show_header(ui, width, screen.settings, screen.toasts);
    }

    let bounds = ui.max_rect();
    egui::ScrollArea::vertical()
        .id_salt(match layout {
            AlbumLayout::Grid => "album-grid-scroll",
            AlbumLayout::List => "album-list-scroll",
        })
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .scroll_bar_rect(bounds.shrink2(egui::vec2(0.0, 14.0)))
        .show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            if albums.is_empty() {
                show_empty(ctx, ui, screen, width);
                return;
            }
            // Keyboard navigation asks for the highlighted album to be scrolled into view.
            let scroll_to = std::mem::take(&mut screen.library.view.scroll_to_selection)
                .then(|| {
                    let highlighted = screen.library.view.highlighted_album?;
                    albums.iter().position(|&index| index == highlighted)
                })
                .flatten();
            match layout {
                AlbumLayout::Grid => {
                    grid::show_album_rows(ctx, ui, screen, albums, viewport, width, scroll_to)
                }
                AlbumLayout::List => {
                    screen.library.view.grid_columns = 1;
                    list::show_album_list(ctx, ui, screen, albums, viewport, width, scroll_to)
                }
            }
        });
    if background_response.clicked() {
        screen.library.view.highlighted_album = None;
    }
}

fn show_empty(ctx: &egui::Context, ui: &mut egui::Ui, screen: &AlbumScreen<'_>, width: f32) {
    let colors = theme::colors(ui.ctx());
    if screen.library.scanning
        && screen.library.model.albums.is_empty()
        && screen.library.view.search.trim().is_empty()
    {
        match screen.settings.album_layout {
            AlbumLayout::Grid => grid::show_skeleton_albums(ctx, ui, width),
            AlbumLayout::List => list::show_skeleton_list(ctx, ui),
        }
        return;
    }
    ui.add_space(70.0);
    let message = if screen.library.model.albums.is_empty() {
        "Your albums will appear here"
    } else if screen.library.scanning && screen.library.view.search.trim().is_empty() {
        "Scanning your music folder…"
    } else {
        "No albums match your search"
    };
    ui.centered_and_justified(|ui| {
        ui.label(egui::RichText::new(message).size(18.0).color(colors.subtle))
    });
}

pub(super) fn open_album(ctx: &egui::Context, screen: &mut AlbumScreen<'_>, index: usize) {
    screen.library.view.select_album(index);
    screen.workbench.show_album_details = true;
    ctx.request_repaint();
}

pub(super) fn play_album(screen: &mut AlbumScreen<'_>, index: usize) {
    if let Some(album) = screen.library.model.albums.get(index).cloned() {
        screen
            .playback
            .play_album(&album, &screen.library.model.tracks);
    }
}

pub(super) fn show_album_context_menu(
    response: egui::Response,
    screen: &mut AlbumScreen<'_>,
    index: usize,
) {
    response.context_menu(|ui| {
        crate::workbench::style_menu_popup(ui);
        let Some(album) = screen.library.model.albums.get(index) else {
            return;
        };
        if menu_entry(ui, true, "Play album") {
            screen
                .playback
                .play_album(album, &screen.library.model.tracks);
        }
        if menu_entry(ui, true, "Play next") {
            screen.playback.play_next(&album.tracks);
        }
        if menu_entry(ui, true, "Add album to queue") {
            screen.playback.enqueue(&album.tracks);
        }
        let folder = album
            .tracks
            .first()
            .and_then(|&track| screen.library.model.tracks.get(track))
            .and_then(|track| track.path.parent())
            .filter(|folder| !crate::features::library::is_smb_path(folder));
        if menu_entry(ui, folder.is_some(), "Open in file browser") {
            if let Some(folder) = folder {
                if let Err(error) = desktop::open_folder(folder) {
                    screen.toasts.push(format!(
                        "Could not open album folder {}: {error}",
                        folder.display()
                    ));
                }
            }
        }
    });
}

pub(super) fn paint_cover_with_opacity(
    ui: &egui::Ui,
    texture: &egui::TextureHandle,
    rect: egui::Rect,
    opacity: f32,
) {
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0) as u8),
    );
}

pub(super) fn show_shimmer_rect(ui: &egui::Ui, rect: egui::Rect, radius: f32) {
    let colors = theme::colors(ui.ctx());
    let time = ui.ctx().input(|input| input.time);
    let pulse = ((time * 1.8).sin() * 0.5 + 0.5) as f32;
    let base = colors.surface.lerp_to_gamma(colors.hover, pulse * 0.35);
    ui.painter().rect_filled(rect, radius, base);

    const SHIMMER_PERIOD: f32 = 1.5;
    const SHIMMER_SEGMENTS: usize = 12;
    let phase = (time as f32 / SHIMMER_PERIOD).rem_euclid(1.0);
    let eased_phase = phase * phase * (3.0 - 2.0 * phase);
    let band_width = (rect.width() * 0.72).max(30.0);
    let center = rect.right() + band_width * 0.5 - (rect.width() + band_width) * eased_phase;
    let segment_width = band_width / SHIMMER_SEGMENTS as f32;
    let highlight = colors.hover;
    let painter = ui.painter().with_clip_rect(rect.shrink(1.0));
    for segment in 0..SHIMMER_SEGMENTS {
        let midpoint = (segment as f32 + 0.5) / SHIMMER_SEGMENTS as f32;
        let intensity = (midpoint * std::f32::consts::PI).sin() * 0.24;
        let left = center - band_width * 0.5 + segment as f32 * segment_width;
        let slice = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(left + segment_width + 0.5, rect.bottom()),
        );
        painter.rect_filled(slice, radius * 0.35, highlight.linear_multiply(intensity));
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));
}
