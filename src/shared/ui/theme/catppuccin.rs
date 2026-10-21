//! Catppuccin palette adapter, ported from <https://github.com/catppuccin/egui> (MIT).
use super::palette::{Palette, ThemeDefinition};
use eframe::egui::{self, epaint, style};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Flavor {
    Latte,
    Frappe,
    Macchiato,
    Mocha,
}

struct Colors {
    rosewater: egui::Color32,
    mauve: egui::Color32,
    red: egui::Color32,
    maroon: egui::Color32,
    peach: egui::Color32,
    yellow: egui::Color32,
    blue: egui::Color32,
    text: egui::Color32,
    subtext0: egui::Color32,
    overlay1: egui::Color32,
    surface2: egui::Color32,
    surface1: egui::Color32,
    surface0: egui::Color32,
    base: egui::Color32,
    mantle: egui::Color32,
    crust: egui::Color32,
}

const fn rgb(red: u8, green: u8, blue: u8) -> egui::Color32 {
    egui::Color32::from_rgb(red, green, blue)
}

fn colors(flavor: Flavor) -> Colors {
    match flavor {
        Flavor::Latte => Colors {
            rosewater: rgb(220, 138, 120),
            mauve: rgb(136, 57, 239),
            red: rgb(210, 15, 57),
            maroon: rgb(230, 69, 83),
            peach: rgb(254, 100, 11),
            yellow: rgb(223, 142, 29),
            blue: rgb(30, 102, 245),
            text: rgb(76, 79, 105),
            subtext0: rgb(108, 111, 133),
            overlay1: rgb(140, 143, 161),
            surface2: rgb(172, 176, 190),
            surface1: rgb(188, 192, 204),
            surface0: rgb(204, 208, 218),
            base: rgb(239, 241, 245),
            mantle: rgb(230, 233, 239),
            crust: rgb(220, 224, 232),
        },
        Flavor::Frappe => Colors {
            rosewater: rgb(242, 213, 207),
            mauve: rgb(202, 158, 230),
            red: rgb(231, 130, 132),
            maroon: rgb(234, 153, 156),
            peach: rgb(239, 159, 118),
            yellow: rgb(229, 200, 144),
            blue: rgb(140, 170, 238),
            text: rgb(198, 208, 245),
            subtext0: rgb(165, 173, 206),
            overlay1: rgb(131, 139, 167),
            surface2: rgb(98, 104, 128),
            surface1: rgb(81, 87, 109),
            surface0: rgb(65, 69, 89),
            base: rgb(48, 52, 70),
            mantle: rgb(41, 44, 60),
            crust: rgb(35, 38, 52),
        },
        Flavor::Macchiato => Colors {
            rosewater: rgb(244, 219, 214),
            mauve: rgb(198, 160, 246),
            red: rgb(237, 135, 150),
            maroon: rgb(238, 153, 160),
            peach: rgb(245, 169, 127),
            yellow: rgb(238, 212, 159),
            blue: rgb(138, 173, 244),
            text: rgb(202, 211, 245),
            subtext0: rgb(165, 173, 203),
            overlay1: rgb(128, 135, 162),
            surface2: rgb(91, 96, 120),
            surface1: rgb(73, 77, 100),
            surface0: rgb(54, 58, 79),
            base: rgb(36, 39, 58),
            mantle: rgb(30, 32, 48),
            crust: rgb(24, 25, 38),
        },
        Flavor::Mocha => Colors {
            rosewater: rgb(245, 224, 220),
            mauve: rgb(203, 166, 247),
            red: rgb(243, 139, 168),
            maroon: rgb(235, 160, 172),
            peach: rgb(250, 179, 135),
            yellow: rgb(249, 226, 175),
            blue: rgb(137, 180, 250),
            text: rgb(205, 214, 244),
            subtext0: rgb(166, 173, 200),
            overlay1: rgb(127, 132, 156),
            surface2: rgb(88, 91, 112),
            surface1: rgb(69, 71, 90),
            surface0: rgb(49, 50, 68),
            base: rgb(30, 30, 46),
            mantle: rgb(24, 24, 37),
            crust: rgb(17, 17, 27),
        },
    }
}

fn widget_visuals(
    old: style::WidgetVisuals,
    theme: &Colors,
    bg_fill: egui::Color32,
) -> style::WidgetVisuals {
    style::WidgetVisuals {
        bg_fill,
        weak_bg_fill: bg_fill,
        bg_stroke: egui::Stroke {
            color: theme.overlay1,
            ..old.bg_stroke
        },
        fg_stroke: egui::Stroke {
            color: theme.text,
            ..old.fg_stroke
        },
        ..old
    }
}

fn visuals(theme: &Colors, light: bool) -> egui::Visuals {
    // Build complete visuals so changing between light and dark never retains old colors.
    let old = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    let shadow_color = egui::Color32::from_black_alpha(if light { 25 } else { 96 });
    egui::Visuals {
        hyperlink_color: theme.rosewater,
        faint_bg_color: theme.surface0,
        extreme_bg_color: theme.crust,
        code_bg_color: theme.mantle,
        warn_fg_color: theme.peach,
        error_fg_color: theme.maroon,
        window_fill: theme.base,
        panel_fill: theme.base,
        window_stroke: egui::Stroke {
            color: theme.overlay1,
            ..old.window_stroke
        },
        widgets: style::Widgets {
            noninteractive: widget_visuals(old.widgets.noninteractive, theme, theme.base),
            inactive: widget_visuals(old.widgets.inactive, theme, theme.surface0),
            hovered: widget_visuals(old.widgets.hovered, theme, theme.surface2),
            active: widget_visuals(old.widgets.active, theme, theme.surface1),
            open: widget_visuals(old.widgets.open, theme, theme.surface0),
        },
        selection: style::Selection {
            bg_fill: theme.blue.linear_multiply(if light { 0.4 } else { 0.2 }),
            stroke: egui::Stroke {
                color: theme.text,
                ..old.selection.stroke
            },
        },
        window_shadow: epaint::Shadow {
            color: shadow_color,
            ..old.window_shadow
        },
        popup_shadow: epaint::Shadow {
            color: shadow_color,
            ..old.popup_shadow
        },
        dark_mode: !light,
        ..old
    }
}

pub(super) fn definition(flavor: Flavor) -> ThemeDefinition {
    let theme = colors(flavor);
    ThemeDefinition {
        visuals: visuals(&theme, flavor == Flavor::Latte),
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
