use crate::{
    app::ViperApp,
    shared::ui::{
        controls::{action_button, backdrop, click, label, modal_frame, primary_button, sz},
        theme,
    },
};
use gpui::{div, prelude::*, AnyElement, Context, FontWeight, MouseButton};

/// Draw the dialog asking whether to quit while a disc is being read or ripped.
///
/// # Arguments
/// * `app` - Application holding whether the dialog is open.
/// * `cx` - Application entity context.
///
/// # Returns
/// The dialog, or `None` while it is closed.
pub(in crate::workbench) fn render(
    app: &ViperApp,
    cx: &mut Context<ViperApp>,
) -> Option<AnyElement> {
    if !app.workbench.confirm_quit {
        return None;
    }
    let colors = theme::colors(cx);
    let keep = |cx: &Context<ViperApp>| click(cx, |app, _, _| app.answer_quit(false, false));
    Some(
        backdrop("quit-dialog-backdrop", colors)
            .on_click(keep(cx))
            .child(
                modal_frame(colors)
                    .max_w(sz(440.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(label("Quit Viper Music?", 20.0, colors.text).font_weight(FontWeight::BOLD))
                    .child(
                        label(
                            "A disc is still being ripped. Tracks already finished are kept either way.",
                            13.0,
                            colors.muted,
                        )
                        .mt(sz(7.0))
                        .mb(sz(18.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(sz(8.0))
                            .child(action_button("quit-keep-ripping", "Keep Ripping", true, colors, keep(cx)))
                            .child(action_button(
                                "quit-stop-ripping",
                                "Stop and Quit",
                                true,
                                colors,
                                click(cx, |app, _, _| app.answer_quit(true, true)),
                            ))
                            .child(primary_button(
                                "quit-when-done",
                                "Quit When Done",
                                true,
                                colors,
                                click(cx, |app, _, _| app.answer_quit(true, false)),
                            )),
                    ),
            )
            .into_any_element(),
    )
}
