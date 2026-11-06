use crate::{
    app::ViperApp,
    platform::persistence::settings::StartupView,
    shared::ui::{
        controls::{listen, menu_item, sz, tooltip, unzoomed, DragPreview},
        icons::{icon, IconName},
        theme,
    },
    workbench::Page,
};
use gpui::{div, prelude::*, px, relative, AnyElement, Context, CursorStyle, DragMoveEvent};
use gpui_component::menu::ContextMenuExt;

/// Width of the sidebar when it shows only icons.
pub(in crate::workbench) const COMPACT_WIDTH: f32 = 56.0;
const COMPACT_THRESHOLD: f32 = 140.0;
const MIN_EXPANDED_WIDTH: f32 = 170.0;
const MAX_WIDTH: f32 = 360.0;
/// Width of the grab area along the sidebar's right edge.
const HANDLE_WIDTH: f32 = 6.0;

/// Marks a drag of the sidebar's edge.
struct SidebarDrag;

/// Draw the navigation sidebar, whose right edge can be dragged to resize or collapse it.
///
/// # Arguments
/// * `app` - Application holding the current page and sidebar settings.
/// * `cx` - Application entity context.
///
/// # Returns
/// The sidebar element.
pub(in crate::workbench) fn render(app: &mut ViperApp, cx: &mut Context<ViperApp>) -> AnyElement {
    if app.ui.sidebar_drag.is_some() && !cx.has_active_drag() {
        finish_resize(app);
    }
    // If album details were closed while the panel was hidden, show it again.
    if app.settings.left_panel_hidden && !app.workbench.show_album_details {
        app.settings.left_panel_hidden = false;
        app.save_settings();
    }
    let colors = theme::colors(cx);
    let width = width(app);
    // Switch the nav layout with the live width so labels do not clip while resizing.
    let compact = width < COMPACT_THRESHOLD;
    let rip_progress = app.ripper.progress();
    let dragging = app.ui.sidebar_drag.is_some();
    let items = Page::ALL
        .into_iter()
        .filter(|page| !app.settings.hidden_pages.contains(page))
        .map(|page| nav_item(app, page, compact, rip_progress, cx))
        .collect::<Vec<_>>();
    div()
        .id("library-sidebar")
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .gap(sz(2.0))
        .w(sz(width))
        .h_full()
        .px(sz(8.0))
        .py(sz(10.0))
        .bg(colors.sidebar)
        .border_r_1()
        .border_color(colors.border)
        .on_drag_move(listen(
            cx,
            |app, event: &DragMoveEvent<SidebarDrag>, window, _| {
                let width = unzoomed(event.event.position.x - event.bounds.left(), window);
                app.ui.sidebar_drag = Some(width.clamp(COMPACT_WIDTH, MAX_WIDTH));
            },
        ))
        .children(items)
        .child(
            div()
                .id("sidebar-resize-handle")
                .group("sidebar-resize-handle")
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(sz(HANDLE_WIDTH))
                .flex()
                .items_center()
                .justify_end()
                .cursor(CursorStyle::ResizeLeftRight)
                .on_drag(SidebarDrag, |_, _, _, cx| cx.new(|_| DragPreview(None)))
                .child(
                    div()
                        .h(sz(20.0))
                        .w(if dragging { px(2.0) } else { px(1.0) })
                        .when(dragging, |line| line.bg(colors.accent))
                        .when(!dragging, |line| {
                            line.group_hover("sidebar-resize-handle", |style| {
                                style.bg(colors.muted)
                            })
                        }),
                ),
        )
        .into_any_element()
}

/// Width of the sidebar at 100% zoom, following its edge while it is dragged.
///
/// # Arguments
/// * `app` - Application holding the sidebar settings.
///
/// # Returns
/// The sidebar width.
pub(in crate::workbench) fn width(app: &ViperApp) -> f32 {
    app.ui
        .sidebar_drag
        .unwrap_or(if app.settings.compact_sidebar {
            COMPACT_WIDTH
        } else {
            app.settings
                .left_panel_width
                .clamp(MIN_EXPANDED_WIDTH, MAX_WIDTH)
        })
}

/// Snap a dragged sidebar to compact or expanded and remember the choice.
fn finish_resize(app: &mut ViperApp) {
    let Some(width) = app.ui.sidebar_drag.take() else {
        return;
    };
    let compact = width < COMPACT_THRESHOLD;
    app.settings.compact_sidebar = compact;
    if !compact {
        app.settings.left_panel_width = width.clamp(MIN_EXPANDED_WIDTH, MAX_WIDTH);
    }
    app.save_settings();
}

/// Draw a sidebar navigation entry with a context menu for choosing the startup page and hiding
/// the entry, and the rip's progress under Rip CD while a CD is ripped.
fn nav_item(
    app: &ViperApp,
    page: Page,
    compact: bool,
    rip_progress: Option<f32>,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let label = page.label();
    let (icon_name, startup_view) = match page {
        Page::Home => (IconName::House, Some(StartupView::Home)),
        Page::Albums => (IconName::Disc, Some(StartupView::Albums)),
        Page::Songs => (IconName::Music2, Some(StartupView::Songs)),
        Page::Ripper => (IconName::Disc3, None),
    };
    let selected = app.workbench.page == page;
    let rip_progress = rip_progress.filter(|_| page == Page::Ripper);
    let hover_text = match rip_progress {
        Some(progress) => Some(format!("Ripping a CD: {:.0}%", progress * 100.0)),
        None => compact.then(|| label.to_owned()),
    };
    let weak = cx.weak_entity();
    div()
        .id(("nav", page as usize))
        .role(gpui::Role::Tab)
        .aria_label(label)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .h(sz(40.0))
        .rounded(sz(3.0))
        .cursor_pointer()
        .when(selected, |item| item.bg(colors.selected))
        .when(!selected, |item| item.hover(|style| style.bg(colors.hover)))
        .when(compact, |item| item.justify_center())
        .when(!compact, |item| item.pl(sz(13.0)).gap(sz(12.0)))
        .child(icon(
            icon_name,
            18.0,
            if selected { colors.text } else { colors.subtle },
        ))
        .when(!compact, |item| {
            item.child(
                div()
                    .text_size(sz(13.0))
                    .text_color(colors.text)
                    .whitespace_nowrap()
                    .child(label),
            )
        })
        .when_some(rip_progress, |item, progress| {
            // A thin bar along the entry's bottom edge, so a rip can be followed from any page.
            item.child(
                div()
                    .absolute()
                    .left(sz(8.0))
                    .right(sz(8.0))
                    .bottom(sz(2.0))
                    .h(sz(2.0))
                    .rounded(sz(1.0))
                    .bg(colors.border)
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress.clamp(0.0, 1.0)))
                            .rounded(sz(1.0))
                            .bg(colors.accent),
                    ),
            )
        })
        .when_some(hover_text, |item, text| item.tooltip(tooltip(text)))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.workbench.page = page;
            cx.notify();
        }))
        .context_menu(move |mut menu, _, cx| {
            let Some(app) = weak.upgrade() else {
                return menu;
            };
            let settings = &app.read(cx).settings;
            let can_hide = settings.can_hide_page(page);
            if let Some(startup_view) = startup_view {
                let current = settings.startup_view == startup_view;
                let text = if current {
                    "Current startup view".to_owned()
                } else {
                    format!("Set {label} as startup view")
                };
                menu = menu.item(
                    menu_item(text, &weak, move |app, _, _| {
                        app.settings.startup_view = startup_view;
                        app.save_settings();
                    })
                    .disabled(current),
                );
            }
            menu.item(
                menu_item("Hide from sidebar", &weak, move |app, _, _| {
                    app.settings.hidden_pages.insert(page);
                    app.save_settings();
                })
                .disabled(!can_hide),
            )
        })
        .into_any_element()
}
