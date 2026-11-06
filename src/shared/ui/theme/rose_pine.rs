//! Rosé Pine palette adapter. Source: <https://rosepinetheme.com/palette/>
use super::palette::{black_alpha, rgb, Palette, ThemeDefinition};
use gpui::Rgba;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Variant {
    Main,
    Moon,
    Dawn,
}

struct Colors {
    base: Rgba,
    surface: Rgba,
    muted: Rgba,
    subtle: Rgba,
    text: Rgba,
    love: Rgba,
    gold: Rgba,
    rose: Rgba,
    pine: Rgba,
    highlight_low: Rgba,
    highlight_med: Rgba,
}

fn colors(variant: Variant) -> Colors {
    match variant {
        Variant::Main => Colors {
            base: rgb(25, 23, 36),
            surface: rgb(31, 29, 46),
            muted: rgb(110, 106, 134),
            subtle: rgb(144, 140, 170),
            text: rgb(224, 222, 244),
            love: rgb(235, 111, 146),
            gold: rgb(246, 193, 119),
            rose: rgb(235, 188, 186),
            pine: rgb(49, 116, 143),

            highlight_low: rgb(33, 32, 46),
            highlight_med: rgb(64, 61, 82),
        },
        Variant::Moon => Colors {
            base: rgb(35, 33, 54),
            surface: rgb(42, 39, 63),
            muted: rgb(110, 106, 134),
            subtle: rgb(144, 140, 170),
            text: rgb(224, 222, 244),
            love: rgb(235, 111, 146),
            gold: rgb(246, 193, 119),
            rose: rgb(234, 154, 151),
            pine: rgb(62, 143, 176),

            highlight_low: rgb(42, 40, 62),
            highlight_med: rgb(68, 65, 90),
        },
        Variant::Dawn => Colors {
            base: rgb(250, 244, 237),
            surface: rgb(255, 250, 243),
            muted: rgb(152, 147, 165),
            subtle: rgb(121, 117, 147),
            text: rgb(70, 66, 97),
            love: rgb(180, 99, 122),
            gold: rgb(234, 157, 52),
            rose: rgb(215, 130, 126),
            pine: rgb(40, 105, 131),

            highlight_low: rgb(244, 237, 232),
            highlight_med: rgb(223, 218, 217),
        },
    }
}

pub(super) fn definition(variant: Variant) -> ThemeDefinition {
    let colors = colors(variant);
    ThemeDefinition {
        dark: variant != Variant::Dawn,
        palette: Palette {
            canvas: colors.base.into(),
            panel: colors.base.into(),
            sidebar: colors.base.into(),
            input: colors.surface.into(),
            surface: colors.surface.into(),
            hover: colors.highlight_low.into(),
            selected: colors.highlight_med.into(),
            border: colors.highlight_med.into(),
            text: colors.text.into(),
            muted: colors.muted.into(),
            subtle: colors.subtle.into(),
            accent: colors.rose.into(),
            secondary: colors.pine.into(),
            on_accent: colors.base.into(),
            warning: colors.gold.into(),
            danger: colors.love.into(),
            backdrop: black_alpha(if variant == Variant::Dawn { 105 } else { 165 }),
        },
    }
}
