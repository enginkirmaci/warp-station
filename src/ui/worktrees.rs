//! The Worktrees tab: the repository's linked checkouts, what state each is in,
//! and the two things a person does from here — start a chat inside one, or
//! hand its folder to the file manager. The list comes from the kit through the
//! net thread, so the tab never runs git itself.

use crate::store::Store;
use crate::theme;
use crate::ui::{danger_button, neutral_button, primary_button};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use warp_station_core::model::WorktreeRow;
use warpui_kit_core::{motion, shell};

pub struct WorktreesView {
    store: Entity<Store>,
}

impl WorktreesView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        // the store owns the rows; the view only redraws when they move
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store }
    }
}

impl Render for WorktreesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let load = {
            let store = self.store.read(cx);
            let Some(folder) = store.selected_repo.clone() else {
                return div()
                    .p(px(shell::DETAIL_PADDING))
                    .text_color(theme::text_muted())
                    .child("No repository selected")
                    .into_any_element();
            };
            store.worktree_load(&folder).cloned().unwrap_or_default()
        };

        let mut page = div()
            .debug_selector(|| "worktrees-page".into())
            .id("worktrees-page")
            .flex()
            .flex_col()
            .gap_3()
            .p(px(shell::DETAIL_PADDING))
            .overflow_y_scroll()
            .w_full();

        // the head: what the tab is, how many there are, and a re-read
        let count = load.rows.len();
        page = page.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(theme::font_name()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .child("Worktrees"),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .child(if count == 1 {
                                    SharedString::from("1 linked checkout")
                                } else {
                                    SharedString::from(format!("{count} linked checkouts"))
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child({
                            let store = self.store.clone();
                            neutral_button(
                                "worktrees-refresh",
                                "Refresh",
                                !load.loading,
                                move |_, _, cx| {
                                    store.update(cx, |store, cx| store.load_worktrees(cx));
                                },
                            )
                        })
                        .child({
                            let store = self.store.clone();
                            neutral_button(
                                "worktrees-prune",
                                "Prune",
                                !load.loading,
                                move |_, _, cx| {
                                    store.update(cx, |store, cx| store.prune_worktrees(cx));
                                },
                            )
                        }),
                ),
        );

        if load.loading && load.rows.is_empty() {
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(motion::spinner(
                        "worktrees-loading",
                        "icons/spinner.svg",
                        px(theme::icon_small()),
                        theme::text_muted(),
                    ))
                    .child("Reading worktrees"),
            );
        }

        if let Some(error) = &load.error {
            page = page.child(
                div()
                    .debug_selector(|| "worktrees-error".into())
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme::hairline())
                    .bg(theme::surface())
                    .p_3()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(SharedString::from(format!(
                        "Could not read the worktrees: {error}"
                    ))),
            );
        }

        if !load.loading && load.error.is_none() && load.rows.is_empty() {
            page = page.child(
                div()
                    .debug_selector(|| "worktrees-empty".into())
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme::hairline())
                    .bg(theme::surface())
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text())
                            .child("No worktrees yet"),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(
                                "Start a chat with the worktree toggle on in warp-agent and \
                                 its checkout appears here.",
                            ),
                    ),
            );
        }

        for row in &load.rows {
            // a refused removal turns that row into its own confirm
            let removal = self
                .store
                .read(cx)
                .worktree_removal_error(&row.path)
                .map(str::to_string);
            page = page.child(self.render_row(row, removal));
        }
        page.into_any_element()
    }
}

impl WorktreesView {
    /// One checkout: its branch, its numbers, its path and its verbs. A
    /// removal git refused adds the explicit force confirm to the same card.
    fn render_row(&self, row: &WorktreeRow, removal: Option<String>) -> gpui::AnyElement {
        let store = self.store.clone();
        let path = row.path.clone();
        let path_for_chat = path.clone();
        let path_for_folder = path.clone();
        let dirty = if row.dirty > 0 {
            Some(SharedString::from(format!("{} changed", row.dirty)))
        } else {
            None
        };
        let drift = if row.ahead > 0 || row.behind > 0 {
            Some(SharedString::from(format!(
                "{}↑ {}↓",
                row.ahead, row.behind
            )))
        } else {
            None
        };
        div()
            .debug_selector(|| "worktree-row".into())
            .rounded(px(8.))
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::surface())
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(theme::font_name()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .truncate()
                            .child(SharedString::from(row.branch.clone())),
                    )
                    .children(dirty.clone().map(|chip| self.chip(chip, theme::WARN)))
                    .children(
                        drift
                            .clone()
                            .map(|chip| self.chip(chip, theme::text_muted())),
                    ),
            )
            .child(
                div()
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::text_dim())
                    .truncate()
                    .child(SharedString::from(row.path.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child({
                        let folder = path_for_chat.clone();
                        let id = SharedString::from(format!("worktree-chat:{folder}"));
                        let chat_store = store.clone();
                        primary_button(id, "Chat here", true, move |_, _, cx| {
                            // the door opens the checkout as a workspace and
                            // creates the chat there in one command
                            chat_store.update(cx, |store, cx| {
                                store.worktree_new_chat(folder.clone(), cx);
                            });
                        })
                    })
                    .child({
                        let folder = path_for_folder.clone();
                        let id = SharedString::from(format!("worktree-folder:{folder}"));
                        let folder_store = store.clone();
                        neutral_button(id, "Open folder", true, move |_, _, cx| {
                            folder_store.update(cx, |store, cx| {
                                store.worktree_open_folder(folder.clone(), cx);
                            });
                        })
                    })
                    .child({
                        let folder = path.clone();
                        let id = SharedString::from(format!("worktree-remove:{folder}"));
                        let remove_store = store.clone();
                        danger_button(id, "Remove", move |_, _, cx| {
                            remove_store.update(cx, |store, cx| {
                                store.remove_worktree(folder.clone(), false, cx);
                            });
                        })
                    }),
            )
            .when_some(removal, |card, error| {
                let folder = path.clone();
                let force_store = store.clone();
                let cancel_store = store.clone();
                card.child(
                    div()
                        .debug_selector(|| "worktree-removal-confirm".into())
                        .flex()
                        .flex_col()
                        .gap_2()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme::hairline())
                        .p_2()
                        .child(
                            div()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::WARN)
                                .child(SharedString::from(format!("Not removed: {error}"))),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(danger_button(
                                    // one confirm exists at a time, so the id is static
                                    "worktree-force",
                                    "Remove anyway",
                                    move |_, _, cx| {
                                        force_store.update(cx, |store, cx| {
                                            store.remove_worktree(folder.clone(), true, cx);
                                        });
                                    },
                                ))
                                .child(neutral_button(
                                    "worktree-removal-cancel",
                                    "Cancel",
                                    true,
                                    move |_, _, cx| {
                                        cancel_store.update(cx, |store, cx| {
                                            store.dismiss_worktree_removal(cx);
                                        });
                                    },
                                )),
                        ),
                )
            })
            .into_any_element()
    }

    fn chip(&self, label: SharedString, color: gpui::Rgba) -> gpui::Div {
        div()
            .flex_shrink_0()
            .px_1p5()
            .py(px(1.))
            .rounded(px(4.))
            .bg(theme::chip())
            .text_size(px(theme::font_meta()))
            .text_color(color)
            .child(label)
    }
}
