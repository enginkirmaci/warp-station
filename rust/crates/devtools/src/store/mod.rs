//! The app store: one entity owning all app state; views hold only ephemeral
//! UI state, snapshot `store.read(cx)` at render top and end mutations with
//! `cx.notify()`. The store never holds UI handles.

pub struct Store {
    pub accent: gpui::Rgba,
}

impl Store {
    pub fn new() -> Self {
        Self { accent: crate::theme::ACCENT }
    }
}
