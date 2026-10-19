//! Adapter for <https://github.com/catppuccin/egui> (egui 0.31 feature).
use super::palette::{Palette, ThemeDefinition};
use eframe::egui;

pub(super) fn definition(theme: catppuccin_egui::Theme) -> ThemeDefinition {
    // Build complete visuals so changing between light and dark never retains old colors.
    let mut style = egui::Style {
        visuals: if theme == catppuccin_egui::LATTE {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        },
        ..Default::default()
    };
    catppuccin_egui::set_style_theme(&mut style, theme);
    ThemeDefinition {
        visuals: style.visuals,
        palette: Palette {
            canvas: theme.base,
            panel: theme.mantle,
            sidebar: theme.crust,
            input: theme.crust,
            surface: theme.surface0,
            hover: theme.surface1,
            selected: theme.surface0.lerp_to_gamma(theme.mauve, 0.18),
            border: theme.surface2,
            text: theme.text,
            muted: theme.subtext0,
            subtle: theme.overlay1,
            accent: theme.mauve,
            secondary: theme.blue,
            on_accent: theme.base,
            warning: theme.yellow,
            danger: theme.red,
            backdrop: egui::Color32::from_black_alpha(165),
        },
    }
}
