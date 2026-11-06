use super::{
    icons::{icon, IconName},
    text_input::FieldOptions,
    theme::{self, Palette},
};
use crate::app::ViperApp;
use gpui::{
    canvas, div, prelude::*, rems, AnyView, App, Bounds, ClickEvent, Context, Div, ElementId,
    FontWeight, Hsla, Pixels, Point, Rems, Role, SharedString, Stateful, WeakEntity, Window,
};
use gpui_component::{input::Input, menu::PopupMenuItem, tooltip::Tooltip};
use std::{cell::Cell, rc::Rc};

/// Key of the library search field, used to focus it from the keyboard.
pub(crate) const SEARCH_FIELD_ID: &str = "library-search-field";

/// A length given in pixels at the default zoom, as rems so it follows the interface zoom.
///
/// # Arguments
/// * `pixels` - Length at 100% zoom.
///
/// # Returns
/// The length in rems.
pub(crate) fn sz(pixels: f32) -> Rems {
    rems(pixels / 16.0)
}

/// Wrap `f` as a click handler on the application, redrawing once it has run.
///
/// # Arguments
/// * `cx` - Application entity context.
/// * `f` - Called with the application when the element is clicked.
///
/// # Returns
/// A handler for `on_click`.
pub(crate) fn click(
    cx: &Context<ViperApp>,
    f: impl Fn(&mut ViperApp, &mut Window, &mut Context<ViperApp>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let app = cx.weak_entity();
    move |_, window, cx| {
        let _ = app.update(cx, |app, cx| {
            f(app, window, cx);
            cx.notify();
        });
    }
}

/// Wrap `f` as an event handler on the application, redrawing once it has run.
///
/// # Arguments
/// * `cx` - Application entity context.
/// * `f` - Called with the application and the event.
///
/// # Returns
/// A handler for any element event.
pub(crate) fn listen<E: ?Sized + 'static>(
    cx: &Context<ViperApp>,
    f: impl Fn(&mut ViperApp, &E, &mut Window, &mut Context<ViperApp>) + 'static,
) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let app = cx.weak_entity();
    move |event, window, cx| {
        let _ = app.update(cx, |app, cx| {
            f(app, event, window, cx);
            cx.notify();
        });
    }
}

/// A popup menu entry that runs `f` on the application when chosen.
///
/// # Arguments
/// * `label` - Entry text.
/// * `app` - Application the entry acts on.
/// * `f` - Called with the application when the entry is chosen.
///
/// # Returns
/// The menu entry.
pub(crate) fn menu_item(
    label: impl Into<SharedString>,
    app: &WeakEntity<ViperApp>,
    f: impl Fn(&mut ViperApp, &mut Window, &mut Context<ViperApp>) + 'static,
) -> PopupMenuItem {
    let app = app.clone();
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let _ = app.update(cx, |app, cx| {
            f(app, window, cx);
            cx.notify();
        });
    })
}

/// Build a hover tooltip showing `text`.
///
/// # Arguments
/// * `text` - Tooltip text.
///
/// # Returns
/// A builder for `tooltip`.
pub(crate) fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |window, cx| Tooltip::new(text.clone()).build(window, cx)
}

/// One line of text at `size` pixels in `color`.
///
/// # Arguments
/// * `content` - Text to show.
/// * `size` - Font size at 100% zoom.
/// * `color` - Text color.
///
/// # Returns
/// The text element.
pub(crate) fn label(content: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
    div()
        .text_size(sz(size))
        .text_color(color)
        .child(content.into())
}

/// Label `element` as a button for assistive technology such as screen readers.
///
/// # Arguments
/// * `element` - The custom-drawn control.
/// * `label` - Spoken name of the control.
///
/// # Returns
/// The described element.
pub(crate) fn describe_button(
    element: Stateful<Div>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    element.role(Role::Button).aria_label(label)
}

/// A compact button using the application's preference style.
///
/// # Arguments
/// * `id` - Element ID, unique among its siblings.
/// * `text` - Text displayed inside the button.
/// * `enabled` - Whether the button can be clicked.
/// * `colors` - Active theme palette.
/// * `on_click` - Called when the button is clicked while enabled.
///
/// # Returns
/// The button element.
pub(crate) fn action_button(
    id: impl Into<ElementId>,
    text: &'static str,
    enabled: bool,
    colors: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let button = describe_button(div().id(id), text)
        .flex()
        .items_center()
        .justify_center()
        .h(sz(36.0))
        .px(sz(12.0))
        .rounded(sz(7.0))
        .border_1()
        .border_color(colors.border)
        .text_size(sz(12.0));
    if enabled {
        button
            .bg(colors.surface)
            .text_color(colors.text)
            .cursor_pointer()
            .hover(|style| style.bg(colors.hover))
            .active(|style| style.bg(colors.selected))
            .on_click(on_click)
            .child(text)
    } else {
        button.bg(colors.panel).text_color(colors.muted).child(text)
    }
}

/// A page or dialog's main action, filled with the accent color.
///
/// # Arguments
/// * `id` - Element ID, unique among its siblings.
/// * `text` - Text displayed inside the button.
/// * `enabled` - Whether the button can be clicked.
/// * `colors` - Active theme palette.
/// * `on_click` - Called when the button is clicked while enabled.
///
/// # Returns
/// The button element.
pub(crate) fn primary_button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    enabled: bool,
    colors: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let text = text.into();
    let button = describe_button(div().id(id), text.clone())
        .flex()
        .items_center()
        .justify_center()
        .h(sz(36.0))
        .px(sz(16.0))
        .rounded(sz(7.0))
        .text_size(sz(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text);
    if enabled {
        button
            .bg(colors.accent)
            .text_color(colors.on_accent)
            .cursor_pointer()
            .hover(|style| style.opacity(0.9))
            .active(|style| style.opacity(0.8))
            .on_click(on_click)
    } else {
        button.bg(colors.surface).text_color(colors.muted)
    }
}

/// A square button showing only an icon, highlighted on hover.
///
/// # Arguments
/// * `id` - Element ID, unique among its siblings.
/// * `name` - Icon to draw.
/// * `description` - Tooltip and spoken name.
/// * `colors` - Active theme palette.
/// * `on_click` - Called when the button is clicked.
///
/// # Returns
/// The button element.
pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    description: &'static str,
    colors: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    describe_button(div().id(id), description)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(sz(32.0))
        .rounded(sz(8.0))
        .cursor_pointer()
        .text_color(colors.muted)
        .hover(|style| style.bg(colors.hover).text_color(colors.text))
        .tooltip(tooltip(description))
        .on_click(on_click)
        .child(icon(name, 16.0, colors.muted))
}

/// A dialog close button.
///
/// # Arguments
/// * `id` - Element ID, unique among its siblings.
/// * `description` - Tooltip describing what the button closes.
/// * `colors` - Active theme palette.
/// * `on_click` - Called when the button is clicked.
///
/// # Returns
/// The button element.
pub(crate) fn close_button(
    id: impl Into<ElementId>,
    description: &'static str,
    colors: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    icon_button(id, IconName::X, description, colors, on_click)
}

/// Frame shared by the application's modal dialogs.
///
/// # Arguments
/// * `colors` - Active theme palette.
///
/// # Returns
/// A rounded, bordered panel.
pub(crate) fn modal_frame(colors: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .bg(colors.panel)
        .border_1()
        .border_color(colors.border)
        .rounded(sz(14.0))
        .p(sz(18.0))
        .shadow_lg()
}

/// A full-window backdrop that dims the page behind a dialog and blocks its input.
///
/// # Arguments
/// * `id` - Element ID of the backdrop.
/// * `colors` - Active theme palette.
///
/// # Returns
/// The backdrop, which centers its children.
pub(crate) fn backdrop(id: impl Into<ElementId>, colors: Palette) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(colors.backdrop)
        .occlude()
}

/// The label of a form field.
///
/// # Arguments
/// * `colors` - Active theme palette.
/// * `text` - Label text.
///
/// # Returns
/// The label element.
pub(crate) fn field_label(colors: Palette, text: impl Into<SharedString>) -> Div {
    label(text, 13.0, colors.muted)
}

/// A notice with a tinted icon, wrapping long messages.
///
/// # Arguments
/// * `colors` - Active theme palette.
/// * `name` - Icon shown before the text.
/// * `tint` - Icon color.
/// * `text` - Notice text.
///
/// # Returns
/// The notice element.
pub(crate) fn notice(
    colors: Palette,
    name: IconName,
    tint: Hsla,
    text: impl Into<SharedString>,
) -> Div {
    div()
        .flex()
        .items_start()
        .gap(sz(6.0))
        .child(div().flex_none().pt(sz(1.0)).child(icon(name, 15.0, tint)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(sz(13.0))
                .text_color(colors.text)
                .child(text.into()),
        )
}

/// A single-line text field using the application's preference style.
///
/// # Arguments
/// * `app` - Application owning the field.
/// * `key` - Name that identifies the field.
/// * `value` - Current text, used when the field is first drawn.
/// * `options` - Placeholder and masking.
/// * `window` - Window the field belongs to.
/// * `cx` - Application entity context.
/// * `on_change` - Called with the field's text whenever it changes.
///
/// # Returns
/// The field element.
pub(crate) fn text_field(
    app: &mut ViperApp,
    key: impl Into<SharedString>,
    value: &str,
    options: FieldOptions<'_>,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
    on_change: impl Fn(&mut ViperApp, &str) + 'static,
) -> Input {
    let colors = theme::colors(cx);
    let state = app.inputs.field(
        key,
        value,
        options,
        window,
        cx,
        move |app, _, text, _, _| on_change(app, text),
    );
    Input::new(&state)
        .bg(colors.input)
        .border_color(colors.border)
        .text_color(colors.text)
        .text_size(sz(13.0))
}

/// The library search box; `F` focuses it from anywhere.
///
/// # Arguments
/// * `app` - Application whose search query the field edits.
/// * `window` - Window the field belongs to.
/// * `cx` - Application entity context.
///
/// # Returns
/// The search box element.
pub(crate) fn search_field(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> impl IntoElement {
    let colors = theme::colors(cx);
    let query = app.library.view.search.clone();
    let state = app.inputs.field(
        SEARCH_FIELD_ID,
        &query,
        FieldOptions {
            placeholder: "Search",
            masked: false,
        },
        window,
        cx,
        |app, _, text, _, _| app.library.view.search = text.to_owned(),
    );
    if std::mem::take(&mut app.ui.focus_search) {
        state.update(cx, |state, cx| state.focus(window, cx));
    }
    div()
        .id("search-field")
        .flex()
        .flex_none()
        .items_center()
        .gap(sz(4.0))
        .w(sz(220.0))
        .h(sz(32.0))
        .px(sz(10.0))
        .bg(colors.panel)
        .border_1()
        .border_color(colors.border)
        .rounded(sz(3.0))
        .tooltip(tooltip("Search (F)"))
        .child(icon(IconName::Search, 18.0, colors.text))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(&state)
                    .appearance(false)
                    .text_color(colors.text)
                    .text_size(sz(14.0)),
            ),
        )
        .child(
            div()
                .flex_none()
                .bg(colors.canvas)
                .rounded(sz(4.0))
                .px(sz(5.0))
                .py(sz(2.0))
                .child(label("F", 10.0, colors.muted)),
        )
}

/// An invisible layer that records where its parent was laid out, for pointer math in handlers.
///
/// The parent must be `relative` so the layer covers it.
///
/// # Arguments
/// * `bounds` - Updated with the parent's window bounds every frame.
///
/// # Returns
/// The layer element.
pub(crate) fn measure(bounds: &Rc<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    let bounds = Rc::clone(bounds);
    canvas(move |area, _, _| bounds.set(area), |_, _, _, _| {})
        .absolute()
        .size_full()
}

/// Convert window pixels into lengths at 100% zoom, the unit settings store widths in.
///
/// # Arguments
/// * `pixels` - Length on screen.
/// * `window` - Window whose zoom applies.
///
/// # Returns
/// The length at 100% zoom.
pub(crate) fn unzoomed(pixels: Pixels, window: &Window) -> f32 {
    f32::from(pixels) * 16.0 / f32::from(window.rem_size())
}

/// How far along `bounds` the pointer is horizontally.
///
/// # Arguments
/// * `bounds` - Area of the control.
/// * `position` - Pointer position in the window.
///
/// # Returns
/// A fraction from 0 at the left edge to 1 at the right.
pub(crate) fn horizontal_fraction(bounds: Bounds<Pixels>, position: Point<Pixels>) -> f32 {
    let width = f32::from(bounds.size.width);
    if width <= 0.0 {
        return 0.0;
    }
    (f32::from(position.x - bounds.left()) / width).clamp(0.0, 1.0)
}

/// What follows the pointer while something is dragged: nothing, or a short label.
pub(crate) struct DragPreview(pub(crate) Option<SharedString>);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        match &self.0 {
            Some(text) => div()
                .px(sz(10.0))
                .py(sz(6.0))
                .bg(colors.surface)
                .border_1()
                .border_color(colors.accent)
                .rounded(sz(8.0))
                .text_size(sz(12.0))
                .text_color(colors.text)
                .shadow_md()
                .child(text.clone()),
            None => div(),
        }
    }
}
