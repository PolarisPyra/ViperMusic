//! Lucide icons drawn by the application, and the asset source that embeds them.
use super::controls::sz;
use gpui::{AssetSource, Hsla, SharedString, Styled};
use gpui_component::Icon;
pub(crate) use gpui_kit_assets::IconName;
use std::borrow::Cow;

gpui_kit_assets::icon_assets!(
    AppIcons,
    [
        ArrowDownToLine,
        ArrowDownWideNarrow,
        ArrowUpWideNarrow,
        Check,
        ChevronDown,
        ChevronUp,
        Circle,
        CircleAlert,
        CircleArrowUp,
        CircleCheck,
        CircleDot,
        Clock,
        Copy,
        Dice5,
        Disc,
        Disc3,
        Eject,
        Grid2x2,
        GripVertical,
        Heart,
        House,
        Info,
        List,
        ListMusic,
        Minus,
        Music,
        Music2,
        Pause,
        Play,
        Repeat,
        Repeat1,
        Search,
        Shuffle,
        SkipBack,
        SkipForward,
        Star,
        TriangleAlert,
        Volume1,
        Volume2,
        VolumeX,
        X,
    ]
);

/// Assets for the window: the application's icons, then the component library's own.
pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match AppIcons.load(path)? {
            Some(data) => Ok(Some(data)),
            None => gpui_kit_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut paths = AppIcons.list(path)?;
        paths.extend(gpui_kit_assets::Assets.list(path)?);
        Ok(paths)
    }
}

/// Build an icon at `size` pixels in `color`.
///
/// # Arguments
/// * `name` - Icon to draw.
/// * `size` - Edge length in pixels.
/// * `color` - Stroke color.
///
/// # Returns
/// The icon element.
pub(crate) fn icon(name: IconName, size: f32, color: Hsla) -> Icon {
    Icon::new(name).size(sz(size)).text_color(color)
}

/// Build an icon at `size` pixels in the surrounding text color, so it follows hover styles.
///
/// # Arguments
/// * `name` - Icon to draw.
/// * `size` - Edge length in pixels.
///
/// # Returns
/// The icon element.
pub(crate) fn inherited_icon(name: IconName, size: f32) -> Icon {
    Icon::new(name).size(sz(size))
}
