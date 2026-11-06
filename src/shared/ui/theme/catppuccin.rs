//! Catppuccin palette adapter, ported from <https://github.com/catppuccin/egui> (MIT).
use super::palette::{black_alpha, lerp, rgb, Palette, ThemeDefinition};
use gpui::Rgba;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Flavor {
    Latte,
    Frappe,
    Macchiato,
    Mocha,
}

struct Colors {
    mauve: Rgba,
    red: Rgba,
    yellow: Rgba,
    blue: Rgba,
    text: Rgba,
    subtext0: Rgba,
    overlay1: Rgba,
    surface2: Rgba,
    surface1: Rgba,
    surface0: Rgba,
    base: Rgba,
    mantle: Rgba,
    crust: Rgba,
}

fn colors(flavor: Flavor) -> Colors {
    match flavor {
        Flavor::Latte => Colors {
            mauve: rgb(136, 57, 239),
            red: rgb(210, 15, 57),
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
            mauve: rgb(202, 158, 230),
            red: rgb(231, 130, 132),
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
            mauve: rgb(198, 160, 246),
            red: rgb(237, 135, 150),
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
            mauve: rgb(203, 166, 247),
            red: rgb(243, 139, 168),
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

pub(super) fn definition(flavor: Flavor) -> ThemeDefinition {
    let theme = colors(flavor);
    ThemeDefinition {
        dark: flavor != Flavor::Latte,
        palette: Palette {
            canvas: theme.base.into(),
            panel: theme.mantle.into(),
            sidebar: theme.crust.into(),
            input: theme.crust.into(),
            surface: theme.surface0.into(),
            hover: theme.surface1.into(),
            selected: lerp(theme.surface0, theme.mauve, 0.18).into(),
            border: theme.surface2.into(),
            text: theme.text.into(),
            muted: theme.subtext0.into(),
            subtle: theme.overlay1.into(),
            accent: theme.mauve.into(),
            secondary: theme.blue.into(),
            on_accent: theme.base.into(),
            warning: theme.yellow.into(),
            danger: theme.red.into(),
            backdrop: black_alpha(165),
        },
    }
}
