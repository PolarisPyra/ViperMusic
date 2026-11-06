//! Theme registry and semantic colors for native widgets and custom-painted views.
//! Add theme choices here; screens should consume Palette roles, not vendor colors.
use super::{catppuccin, rose_pine};
use gpui::{App, Global, Hsla, Rgba};
use gpui_component::{Theme, ThemeMode};
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
            Self::CatppuccinLatte => catppuccin::definition(catppuccin::Flavor::Latte),
            Self::CatppuccinFrappe => catppuccin::definition(catppuccin::Flavor::Frappe),
            Self::CatppuccinMacchiato => catppuccin::definition(catppuccin::Flavor::Macchiato),
            Self::CatppuccinMocha => catppuccin::definition(catppuccin::Flavor::Mocha),
            Self::RosePine => rose_pine::definition(rose_pine::Variant::Main),
            Self::RosePineMoon => rose_pine::definition(rose_pine::Variant::Moon),
            Self::RosePineDawn => rose_pine::definition(rose_pine::Variant::Dawn),
        }
    }
}

/// Each provider supplies its light or dark mode and the semantic colors views paint with.
pub(super) struct ThemeDefinition {
    pub(super) dark: bool,
    pub(super) palette: Palette,
}

/// The applied theme, kept as a global so any view can read its colors.
#[derive(Clone, Copy)]
struct AppliedTheme {
    id: ThemeId,
    palette: Palette,
}

impl Global for AppliedTheme {}

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    /// Main application background.
    pub canvas: Hsla,
    /// Standard panel background.
    pub panel: Hsla,
    /// Sidebar background.
    pub sidebar: Hsla,
    /// Input control background.
    pub input: Hsla,
    /// Raised surface background.
    pub surface: Hsla,
    /// Hovered control background.
    pub hover: Hsla,
    /// Selected control background.
    pub selected: Hsla,
    /// Standard border color.
    pub border: Hsla,
    /// Primary text color.
    pub text: Hsla,
    /// Secondary text color.
    pub muted: Hsla,
    /// Low-emphasis text and details.
    pub subtle: Hsla,
    /// Primary accent color.
    pub accent: Hsla,
    /// Secondary accent color.
    pub secondary: Hsla,
    /// Text color to use over the accent color.
    pub on_accent: Hsla,
    /// Warning status color.
    pub warning: Hsla,
    /// Error or danger status color.
    pub danger: Hsla,
    /// Overlay backdrop color.
    pub backdrop: Hsla,
}

/// An opaque color from its 8-bit channels.
pub(super) const fn rgb(red: u8, green: u8, blue: u8) -> Rgba {
    Rgba {
        r: red as f32 / 255.0,
        g: green as f32 / 255.0,
        b: blue as f32 / 255.0,
        a: 1.0,
    }
}

/// Black at `alpha` out of 255, for shadows and backdrops.
pub(super) fn black_alpha(alpha: u8) -> Hsla {
    gpui::black().opacity(f32::from(alpha) / 255.0)
}

/// Blend from `from` to `to` by `amount` in sRGB space.
pub(super) fn lerp(from: Rgba, to: Rgba, amount: f32) -> Rgba {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Rgba {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

/// Colors of the applied theme.
///
/// # Arguments
/// * `cx` - Application context holding the theme.
///
/// # Returns
/// The applied palette, or the default theme's before any is applied.
pub(crate) fn colors(cx: &App) -> Palette {
    cx.try_global::<AppliedTheme>()
        .map(|theme| theme.palette)
        .unwrap_or_else(|| ThemeId::default().definition().palette)
}

/// Apply `theme` to custom-painted views and the component library, when it has changed.
///
/// # Arguments
/// * `cx` - Application context.
/// * `theme` - Theme to apply.
pub(crate) fn apply(cx: &mut App, theme: ThemeId) {
    if cx.try_global::<AppliedTheme>().map(|applied| applied.id) == Some(theme) {
        return;
    }
    let ThemeDefinition { dark, palette } = theme.definition();
    cx.set_global(AppliedTheme { id: theme, palette });
    let mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    // A mode change loads that mode's stock colors, so the palette goes over them afterwards.
    Theme::update(cx, |components| components.mode = mode);
    Theme::update(cx, |components| {
        let colors = &mut components.colors;
        colors.background = palette.canvas;
        colors.foreground = palette.text;
        colors.border = palette.border;
        colors.input = palette.border;
        colors.ring = palette.accent;
        colors.caret = palette.text;
        colors.selection = palette.selected;
        colors.muted = palette.surface;
        colors.muted_foreground = palette.muted;
        colors.accent = palette.hover;
        colors.accent_foreground = palette.text;
        colors.primary = palette.accent;
        colors.primary_hover = palette.accent.opacity(0.9);
        colors.primary_active = palette.accent.opacity(0.8);
        colors.primary_foreground = palette.on_accent;
        colors.secondary = palette.surface;
        colors.secondary_hover = palette.hover;
        colors.secondary_active = palette.selected;
        colors.secondary_foreground = palette.text;
        colors.popover = palette.panel;
        colors.popover_foreground = palette.text;
        colors.list = palette.panel;
        colors.list_hover = palette.hover;
        colors.list_active = palette.selected;
        colors.list_active_border = palette.accent;
        colors.slider_bar = palette.accent;
        colors.slider_thumb = palette.text;
        colors.switch = palette.surface;
        colors.switch_thumb = palette.text;
        colors.scrollbar = gpui::transparent_black();
        colors.scrollbar_thumb = palette.border;
        colors.scrollbar_thumb_hover = palette.subtle;
        colors.link = palette.secondary;
        colors.link_hover = palette.secondary;
        colors.link_active = palette.secondary;
        colors.warning = palette.warning;
        colors.danger = palette.danger;
        colors.overlay = palette.backdrop;
        colors.window_border = palette.border;
        colors.progress_bar = palette.accent;
    });
}
