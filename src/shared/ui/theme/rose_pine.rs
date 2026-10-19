//! Rosé Pine palette adapter. Source: <https://rosepinetheme.com/palette/>
use super::palette::{Palette, ThemeDefinition};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Variant {
    Main,
    Moon,
    Dawn,
}

struct Colors {
    base: egui::Color32,
    surface: egui::Color32,
    overlay: egui::Color32,
    muted: egui::Color32,
    subtle: egui::Color32,
    text: egui::Color32,
    love: egui::Color32,
    gold: egui::Color32,
    rose: egui::Color32,
    pine: egui::Color32,
    foam: egui::Color32,
    highlight_low: egui::Color32,
    highlight_med: egui::Color32,
    highlight_high: egui::Color32,
}

const fn rgb(red: u8, green: u8, blue: u8) -> egui::Color32 {
    egui::Color32::from_rgb(red, green, blue)
}

fn colors(variant: Variant) -> Colors {
    match variant {
        Variant::Main => Colors {
            base: rgb(25, 23, 36),
            surface: rgb(31, 29, 46),
            overlay: rgb(38, 35, 58),
            muted: rgb(110, 106, 134),
            subtle: rgb(144, 140, 170),
            text: rgb(224, 222, 244),
            love: rgb(235, 111, 146),
            gold: rgb(246, 193, 119),
            rose: rgb(235, 188, 186),
            pine: rgb(49, 116, 143),
            foam: rgb(156, 207, 216),

            highlight_low: rgb(33, 32, 46),
            highlight_med: rgb(64, 61, 82),
            highlight_high: rgb(82, 79, 103),
        },
        Variant::Moon => Colors {
            base: rgb(35, 33, 54),
            surface: rgb(42, 39, 63),
            overlay: rgb(57, 53, 82),
            muted: rgb(110, 106, 134),
            subtle: rgb(144, 140, 170),
            text: rgb(224, 222, 244),
            love: rgb(235, 111, 146),
            gold: rgb(246, 193, 119),
            rose: rgb(234, 154, 151),
            pine: rgb(62, 143, 176),
            foam: rgb(156, 207, 216),

            highlight_low: rgb(42, 40, 62),
            highlight_med: rgb(68, 65, 90),
            highlight_high: rgb(86, 82, 110),
        },
        Variant::Dawn => Colors {
            base: rgb(250, 244, 237),
            surface: rgb(255, 250, 243),
            overlay: rgb(242, 233, 225),
            muted: rgb(152, 147, 165),
            subtle: rgb(121, 117, 147),
            text: rgb(70, 66, 97),
            love: rgb(180, 99, 122),
            gold: rgb(234, 157, 52),
            rose: rgb(215, 130, 126),
            pine: rgb(40, 105, 131),
            foam: rgb(86, 148, 159),

            highlight_low: rgb(244, 237, 232),
            highlight_med: rgb(223, 218, 217),
            highlight_high: rgb(206, 202, 205),
        },
    }
}

pub(super) fn definition(variant: Variant) -> ThemeDefinition {
    let colors = colors(variant);
    let mut visuals = if variant == Variant::Dawn {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.dark_mode = variant != Variant::Dawn;
    visuals.panel_fill = colors.base;
    visuals.window_fill = colors.base;
    visuals.extreme_bg_color = colors.base;
    visuals.faint_bg_color = colors.highlight_low;
    visuals.code_bg_color = colors.overlay;
    visuals.hyperlink_color = colors.foam;
    visuals.warn_fg_color = colors.gold;
    visuals.error_fg_color = colors.love;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, colors.highlight_med);
    visuals.selection.bg_fill = colors.highlight_med;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, colors.text);

    visuals.widgets.noninteractive = widget(
        visuals.widgets.noninteractive,
        colors.base,
        colors.text,
        colors.overlay,
    );
    visuals.widgets.inactive = widget(
        visuals.widgets.inactive,
        colors.surface,
        colors.text,
        colors.overlay,
    );
    visuals.widgets.hovered = widget(
        visuals.widgets.hovered,
        colors.highlight_low,
        colors.text,
        colors.highlight_med,
    );
    visuals.widgets.active = widget(
        visuals.widgets.active,
        colors.highlight_med,
        colors.text,
        colors.highlight_high,
    );
    visuals.widgets.open = widget(
        visuals.widgets.open,
        colors.surface,
        colors.text,
        colors.overlay,
    );

    ThemeDefinition {
        visuals,
        palette: Palette {
            canvas: colors.base,
            panel: colors.base,
            sidebar: colors.base,
            input: colors.surface,
            surface: colors.surface,
            hover: colors.highlight_low,
            selected: colors.highlight_med,
            border: colors.highlight_med,
            text: colors.text,
            muted: colors.muted,
            subtle: colors.subtle,
            accent: colors.rose,
            secondary: colors.pine,
            on_accent: colors.base,
            warning: colors.gold,
            danger: colors.love,
            backdrop: if variant == Variant::Dawn {
                egui::Color32::from_black_alpha(105)
            } else {
                egui::Color32::from_black_alpha(165)
            },
        },
    }
}

fn widget(
    old: egui::style::WidgetVisuals,
    background: egui::Color32,
    foreground: egui::Color32,
    border: egui::Color32,
) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: background,
        weak_bg_fill: background,
        bg_stroke: egui::Stroke::new(1.0_f32, border),
        fg_stroke: egui::Stroke::new(1.0_f32, foreground),
        ..old
    }
}
