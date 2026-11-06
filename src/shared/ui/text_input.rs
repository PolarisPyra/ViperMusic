//! Text fields of the open window, created on first draw and copied into the model as they change.
use crate::app::ViperApp;
use gpui::{AppContext, Context, Entity, Focusable, SharedString, Subscription, Window};
use gpui_component::input::{InputEvent, InputState};
use std::collections::HashMap;

/// Text fields by key, with the subscriptions that copy their edits into the model.
#[derive(Default)]
pub(crate) struct TextInputs {
    fields: HashMap<SharedString, (Entity<InputState>, Subscription)>,
}

/// How a text field looks when it is created.
#[derive(Clone, Copy, Default)]
pub(crate) struct FieldOptions<'a> {
    /// Text shown while the field is empty.
    pub(crate) placeholder: &'a str,
    /// Whether the field hides what is typed, such as for a password.
    pub(crate) masked: bool,
}

impl TextInputs {
    /// Return the field `key`, creating it holding `value` the first time it is drawn.
    ///
    /// # Arguments
    /// * `key` - Name that identifies the field while it stays open.
    /// * `value` - Text the field starts with.
    /// * `options` - Placeholder and masking.
    /// * `window` - Window the field belongs to.
    /// * `cx` - Application entity context.
    /// * `on_event` - Called with the field's event and text whenever it changes or Enter is
    ///   pressed.
    ///
    /// # Returns
    /// The field's state, to draw with `Input::new`.
    pub(crate) fn field(
        &mut self,
        key: impl Into<SharedString>,
        value: &str,
        options: FieldOptions<'_>,
        window: &mut Window,
        cx: &mut Context<ViperApp>,
        on_event: impl Fn(&mut ViperApp, &InputEvent, &str, &mut Window, &mut Context<ViperApp>)
            + 'static,
    ) -> Entity<InputState> {
        let key = key.into();
        if let Some((state, _)) = self.fields.get(&key) {
            return state.clone();
        }
        let placeholder = SharedString::from(options.placeholder.to_owned());
        let value = SharedString::from(value.to_owned());
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .masked(options.masked)
                .default_value(value)
        });
        let subscription = cx.subscribe_in(
            &state,
            window,
            move |app, state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change | InputEvent::PressEnter { .. }) {
                    let text = state.read(cx).value();
                    on_event(app, event, &text, window, cx);
                    cx.notify();
                }
            },
        );
        self.fields.insert(key, (state.clone(), subscription));
        state
    }

    /// Replace the text of the field `key`, if it exists, without reporting it as an edit.
    ///
    /// # Arguments
    /// * `key` - Field name.
    /// * `value` - New text.
    /// * `window` - Window the field belongs to.
    /// * `cx` - Application entity context.
    pub(crate) fn set(
        &self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<ViperApp>,
    ) {
        if let Some((state, _)) = self.fields.get(key) {
            let value = SharedString::from(value.to_owned());
            state.update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }

    /// Show `value` in the field `key` when the model changed it, unless the field is being edited.
    ///
    /// # Arguments
    /// * `key` - Field name.
    /// * `value` - Text the model holds for the field.
    /// * `window` - Window the field belongs to.
    /// * `cx` - Application entity context.
    pub(crate) fn sync(
        &self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<ViperApp>,
    ) {
        let Some((state, _)) = self.fields.get(key) else {
            return;
        };
        let field = state.read(cx);
        if field.value().as_ref() != value && !field.focus_handle(cx).is_focused(window) {
            self.set(key, value, window, cx);
        }
    }

    /// Move keyboard focus to the field `key`, if it exists.
    ///
    /// # Arguments
    /// * `key` - Field name.
    /// * `window` - Window the field belongs to.
    /// * `cx` - Application entity context.
    pub(crate) fn focus(&self, key: &str, window: &mut Window, cx: &mut Context<ViperApp>) {
        if let Some((state, _)) = self.fields.get(key) {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    /// Forget the fields whose keys start with `prefix`, such as when their dialog closes.
    ///
    /// # Arguments
    /// * `prefix` - Key prefix shared by the fields.
    pub(crate) fn remove_prefixed(&mut self, prefix: &str) {
        self.fields.retain(|key, _| !key.starts_with(prefix));
    }

    /// Forget every field, such as when the window they belong to closes.
    pub(crate) fn clear(&mut self) {
        self.fields.clear();
    }
}
