//! The bar at a list's right edge. One drag moves the column beside it: every
//! workbench list uses this, so the handle, the cursor and the bounds are one
//! shape instead of a hand-rolled copy per view.
//!
//! The list's width is the store's ([`Store::pane_width`]); the view that draws
//! the list puts the handle on the list's non-scrolling column and wraps the
//! row with [`row`], which reads the pointer and writes the new width.

use crate::store::{Pane, Store};
use crate::theme;
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, DragMoveEvent, Entity, IntoElement, Render, Window, div, px};

/// What a drag on a list's edge carries. The pane is part of the drag so a row
/// that also holds another list's edge ignores a drag that is not its own.
struct PaneResize {
    pane: Pane,
}

impl Render for PaneResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// The drag bar on a list's right edge, on top of its hairline: a 6 px strip
/// that takes the column-resize cursor and hands its drag to [`row`].
pub fn handle(pane: Pane) -> AnyElement {
    let selector = pane.selector();
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .absolute()
        .top_0()
        .bottom_0()
        .right(px(-3.))
        .w(px(6.))
        .cursor_col_resize()
        .hover(|s| s.bg(theme::line_strong()))
        .on_drag(PaneResize { pane }, move |value: &PaneResize, _, _, cx| {
            cx.stop_propagation();
            let pane = value.pane;
            cx.new(|_| PaneResize { pane })
        })
        .into_any_element()
}

/// The row that holds `pane`'s list first: a drag on the list's edge moves the
/// column to the pointer, inside [`Pane`]'s bounds, and the drop saves it.
/// A drag that is not this pane's is ignored, so nested rows never fight over
/// one pointer.
pub fn row(store: &Entity<Store>, pane: Pane, row: gpui::Div) -> gpui::Div {
    let drag_store = store.clone();
    let drop_store = store.clone();
    row.on_drag_move(move |event: &DragMoveEvent<PaneResize>, _, cx: &mut App| {
        if event.drag(cx).pane != pane {
            return;
        }
        let width = f32::from(event.event.position.x - event.bounds.left());
        drag_store.update(cx, |store, cx| store.set_pane_width(pane, width, cx));
    })
    .on_drop(move |_: &PaneResize, _, cx: &mut App| {
        drop_store.update(cx, |store, _| store.save_pane_width(pane));
    })
}
