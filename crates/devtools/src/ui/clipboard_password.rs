//! The Clipboard Password drawer: stores a password (Base64 over UTF-8 in the
//! settings file — the C# build's exact format) and copies it back via the
//! Copy button or the in-app Ctrl+Shift+V shortcut.

use crate::store::Store;
use crate::theme;
use gpui::prelude::*;
use gpui::{div, px, App, Context, Entity, FontWeight, Window};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::Sizable as _;

pub struct ClipboardPasswordDrawer {
    pub store: Entity<Store>,
    password: Entity<InputState>,
    status: String,
    /// A finished save asks the next render pass to clear the input
    /// (set_value needs a Window, which command paths don't carry).
    pending_clear: bool,
}

/// The stored password decoded, ready for the clipboard.
pub fn stored_password(settings: &devtools_core::settings::AppSettings) -> Option<String> {
    settings
        .clipboard_password
        .encrypted_password
        .as_deref()
        .filter(|stored| !stored.is_empty())
        .and_then(devtools_core::clipboard_password::decode)
}

/// Copies the stored password and returns the toast text.
pub fn copy_stored_password(store: &Entity<Store>, cx: &mut App) -> &'static str {
    match stored_password(&store.read(cx).settings()) {
        Some(password) => {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(password));
            "Password copied to clipboard"
        }
        None => "No password stored",
    }
}

fn status_line(has_stored: bool) -> &'static str {
    if has_stored {
        "Password is stored and ready to use with Ctrl+Shift+V"
    } else {
        "No password stored yet"
    }
}

impl ClipboardPasswordDrawer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let has_stored = stored_password(&store.read(cx).settings()).is_some();
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type your password here…")
                .masked(true)
        });
        Self {
            store,
            password,
            status: status_line(has_stored).to_string(),
            pending_clear: false,
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let value = self.password.read(cx).value().trim().to_string();
        if value.is_empty() {
            self.status = "Please enter a password".into();
            return;
        }
        let encoded = devtools_core::clipboard_password::encode(&value);
        self.store.update(cx, |store, _| {
            store.update_settings(|s| s.clipboard_password.encrypted_password = Some(encoded));
        });
        self.pending_clear = true;
        self.status = "Password saved successfully! Use Ctrl+Shift+V to paste it.".into();
        self.toast("Password saved", cx);
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, _| {
            store.update_settings(|s| s.clipboard_password.encrypted_password = None);
        });
        self.pending_clear = true;
        self.status = "Password cleared".into();
        self.toast("Password cleared", cx);
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let message = copy_stored_password(&self.store.clone(), cx);
        self.toast(message, cx);
    }

    fn toast(&mut self, message: &'static str, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.notice = Some((message.to_string(), std::time::Instant::now()));
            cx.notify();
        });
        cx.notify();
    }
}

impl Render for ClipboardPasswordDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if self.pending_clear {
            self.pending_clear = false;
            self.password.update(cx, |state, cx| state.set_value(String::new(), window, cx));
        }
        let stored = stored_password(&self.store.read(cx).settings()).is_some();

        div()
            .id("clipboard-password-drawer")
            .w(px(380.))
            .h_full()
            .bg(theme::POPUP)
            .border_l_1()
            .border_color(theme::HAIRLINE)
            .shadow_lg()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .occlude()
            .child(
                div()
                    .text_size(px(theme::font_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Clipboard Password"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(theme::font_small() - 1.))
                            .text_color(theme::TEXT_MUTED)
                            .child("Password"),
                    )
                    .child(Input::new(&self.password).small().mask_toggle()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(ghost_button(
                        "clipboard-clear",
                        "Clear Stored Password",
                        theme::DANGER,
                        stored,
                        cx.listener(|this, _, _, cx| this.clear(cx)),
                    ))
                    .child(ghost_button(
                        "clipboard-copy",
                        "Copy",
                        theme::ACCENT,
                        stored,
                        cx.listener(|this, _, _, cx| this.copy(cx)),
                    ))
                    .child(accent_button(
                        "clipboard-save",
                        "Save Password",
                        !stored,
                        cx.listener(|this, _, _, cx| this.save(cx)),
                    )),
            )
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_DIM)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .text_size(px(theme::font_small() - 1.))
                    .text_color(theme::TEXT_FAINT)
                    .child("Ctrl+Shift+V copies the stored password while DevTools is focused."),
            )
    }
}

fn ghost_button(
    id: &'static str,
    label: &str,
    color: gpui::Rgba,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .cursor_pointer()
        .rounded_md()
        .px_2p5()
        .py_1()
        .text_size(px(theme::font_small()))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::tint(color))
        .text_color(color)
        .map(|d| {
            if enabled {
                d.hover(|s| s.opacity(0.85))
            } else {
                d.opacity(0.4)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}

fn accent_button(
    id: &'static str,
    label: &str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::font_small()))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::ACCENT)
        .text_color(theme::ON_ACCENT)
        .map(|d| {
            if enabled {
                d.hover(|s| s.opacity(0.9))
            } else {
                d.opacity(0.4)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}
