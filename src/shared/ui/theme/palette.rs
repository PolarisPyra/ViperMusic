//! Theme registry and semantic colors for native widgets and custom-painted views.
//! Add theme choices here; screens should consume Palette roles, not vendor colors.
use super::{catppuccin, rose_pine};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Built-in color theme selected by the user.
pub enum ThemeId {
    /// Light Catppuccin palette.
    CatppuccinLatte,
    /// Muted Catppuccin palette.
    CatppuccinFrappe,
    /// Dark Catppuccin palette.
    CatppuccinMacchiato,
    /// Deep dark Catppuccin palette.
    #[default]
    CatppuccinMocha,
    /// Rosé Pine's default dark palette.
    RosePine,
    /// Rosé Pine Moon's darker palette.
    RosePineMoon,
    /// Rosé Pine Dawn's light palette.
    RosePineDawn,
}

impl ThemeId {
    pub(crate) const ALL: [Self; 7] = [
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
        Self::RosePine,
        Self::RosePineMoon,
        Self::RosePineDawn,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::CatppuccinFrappe => "Catppuccin Frappé",
            Self::CatppuccinMacchiato => "Catppuccin Macchiato",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::RosePine => "Rosé Pine",
            Self::RosePineMoon => "Rosé Pine Moon",
            Self::RosePineDawn => "Rosé Pine Dawn",
        }
    }

    fn definition(self) -> ThemeDefinition {
        match self {
            Self::CatppuccinLatte => catppuccin::definition(catppuccin_egui::LATTE),
            Self::CatppuccinFrappe => catppuccin::definition(catppuccin_egui::FRAPPE),
            Self::CatppuccinMacchiato => catppuccin::definition(catppuccin_egui::MACCHIATO),
            Self::CatppuccinMocha => catppuccin::definition(catppuccin_egui::MOCHA),
            Self::RosePine => rose_pine::definition(rose_pine::Variant::Main),
            Self::RosePineMoon => rose_pine::definition(rose_pine::Variant::Moon),
            Self::RosePineDawn => rose_pine::definition(rose_pine::Variant::Dawn),
        }
    }
}

/// Each provider supplies native egui visuals and custom-paint semantic colors.
pub(super) struct ThemeDefinition {
    pub(super) visuals: egui::Visuals,
    pub(super) palette: Palette,
}

#[derive(Clone, Copy)]
struct AppliedTheme {
    id: ThemeId,
    palette: Palette,
}

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    /// Main application background.
    pub canvas: egui::Color32,
    /// Standard panel background.
    pub panel: egui::Color32,
    /// Sidebar background.
    pub sidebar: egui::Color32,
    /// Input control background.
    pub input: egui::Color32,
    /// Raised surface background.
    pub surface: egui::Color32,
    /// Hovered control background.
    pub hover: egui::Color32,
    /// Selected control background.
    pub selected: egui::Color32,
    /// Standard border color.
    pub border: egui::Color32,
    /// Primary text color.
    pub text: egui::Color32,
    /// Secondary text color.
    pub muted: egui::Color32,
    /// Low-emphasis text and details.
    pub subtle: egui::Color32,
    /// Primary accent color.
    pub accent: egui::Color32,
    /// Secondary accent color.
    pub secondary: egui::Color32,
    /// Text color to use over the accent color.
    pub on_accent: egui::Color32,
    /// Warning status color.
    pub warning: egui::Color32,
    /// Error or danger status color.
    pub danger: egui::Color32,
    /// Overlay backdrop color.
    pub backdrop: egui::Color32,
}

pub(crate) fn colors(ctx: &egui::Context) -> Palette {
    ctx.data(|data| data.get_temp::<AppliedTheme>(egui::Id::new("app-theme")))
        .map(|theme| theme.palette)
        .unwrap_or_else(|| ThemeId::default().definition().palette)
}

pub(crate) fn apply(ctx: &egui::Context, theme: ThemeId) {
    let id = egui::Id::new("app-theme");
    if ctx
        .data(|data| data.get_temp::<AppliedTheme>(id))
        .map(|theme| theme.id)
        == Some(theme)
    {
        return;
    }
    let ThemeDefinition {
        mut visuals,
        palette,
    } = theme.definition();
    visuals.selection.bg_fill = palette.selected;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, palette.text);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, palette.border);
    visuals.hyperlink_color = palette.secondary;
    visuals.warn_fg_color = palette.warning;
    visuals.error_fg_color = palette.danger;
    ctx.set_visuals(visuals);
    ctx.data_mut(|data| data.insert_temp(id, AppliedTheme { id: theme, palette }));
    ctx.request_repaint();
}
