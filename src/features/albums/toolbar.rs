use crate::{
    app::ViperApp,
    platform::persistence::settings::{AlbumLayout, AlbumSort},
    shared::ui::{
        controls::{describe_button, menu_item, search_field, sz, tooltip},
        icons::{inherited_icon, IconName},
        theme::{self, Palette},
    },
};
use gpui::{div, prelude::*, px, AnyElement, Context, Div, Stateful, Window};
use gpui_component::{button::Button, menu::DropdownMenu};

/// Width of the sort menu and the button that opens it, matching the search field's height.
const SORT_WIDTH: f32 = 176.0;

/// Draw the toolbar above the albums: sorting, the search field and the layout choice.
///
/// # Arguments
/// * `app` - Application holding the album settings.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The toolbar element.
pub(super) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let active = app.settings.album_sort;
    let ascending = app.settings.sort_ascending;
    let layout = app.settings.album_layout;
    let weak = cx.weak_entity();
    let sort = Button::new("album-sort")
        .label(active.label())
        .outline()
        .dropdown_caret(true)
        .w(sz(SORT_WIDTH))
        .h(sz(32.0))
        .px(sz(10.0))
        .dropdown_menu(move |mut menu, _, _| {
            menu = menu.min_w(px(SORT_WIDTH));
            for sort in AlbumSort::ALL {
                menu = menu.item(
                    menu_item(sort.label(), &weak, move |app, _, _| set_sort(app, sort))
                        .checked(active == sort),
                );
            }
            menu
        });
    let direction = view_button(
        "album-sort-direction",
        if ascending {
            IconName::ArrowUpWideNarrow
        } else {
            IconName::ArrowDownWideNarrow
        },
        if ascending { "Ascending" } else { "Descending" },
        false,
        colors,
    )
    .on_click(cx.listener(|app, _, _, cx| {
        app.settings.sort_ascending = !app.settings.sort_ascending;
        app.library.view.album_sort_dirty = true;
        app.save_settings();
        cx.notify();
    }));
    let layout_button = |id, name, label, choice: AlbumLayout, cx: &mut Context<ViperApp>| {
        view_button(id, name, label, layout == choice, colors).on_click(cx.listener(
            move |app, _, _, cx| {
                app.settings.album_layout = choice;
                app.save_settings();
                cx.notify();
            },
        ))
    };
    let grid = layout_button(
        "album-grid-view",
        IconName::Grid2x2,
        "Grid view",
        AlbumLayout::Grid,
        cx,
    );
    let list = layout_button(
        "album-list-view",
        IconName::List,
        "List view",
        AlbumLayout::List,
        cx,
    );
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(sz(8.0))
        .h(sz(48.0))
        .px(sz(20.0))
        .py(sz(8.0))
        .bg(colors.canvas)
        .border_b_1()
        .border_color(colors.border)
        .child(sort)
        .child(direction)
        .child(div().flex_1())
        .child(search_field(app, window, cx))
        .child(grid)
        .child(list)
        .into_any_element()
}

/// Sort the albums by `sort`, reshuffling when random order is chosen again.
fn set_sort(app: &mut ViperApp, sort: AlbumSort) {
    if app.settings.album_sort == sort {
        return;
    }
    app.settings.album_sort = sort;
    let view = &mut app.library.view;
    if sort == AlbumSort::Random {
        view.random_sort_seed = view.random_sort_seed.wrapping_add(1);
    }
    view.album_sort_dirty = true;
    app.save_settings();
}

/// A toolbar icon button, filled while its choice is active.
fn view_button(
    id: &'static str,
    name: IconName,
    label: &'static str,
    selected: bool,
    colors: Palette,
) -> Stateful<Div> {
    describe_button(div().id(id), label)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(sz(34.0))
        .h(sz(32.0))
        .rounded(sz(6.0))
        .cursor_pointer()
        .tooltip(tooltip(label))
        .when(selected, |button| {
            button.bg(colors.selected).text_color(colors.text)
        })
        .when(!selected, |button| {
            button
                .text_color(colors.muted)
                .hover(|style| style.bg(colors.hover).text_color(colors.text))
        })
        .child(inherited_icon(name, 18.0))
}
