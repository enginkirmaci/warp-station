//! The workspace sheet: what the rail has no room for, as one bottom sheet with
//! a tab per surface. Changes keeps its three columns, the pull request, issue
//! and pipeline lists get the full width, and the README reads at a proper
//! measure. The window hosts it over a scrim; its top edge drags to resize.

use crate::store::{Store, WorkspaceTab};
use crate::theme;
use crate::ui::azure_tab::AzureView;
use crate::ui::changes::ChangesView;
use crate::ui::pr_issues::ItemListView;
use crate::ui::readme::ReadmeView;
use crate::ui::repos::{segment, segment_row};
use crate::ui::{A11y, DrawerCloseHandler, icon};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, MouseButton, Pixels, Window, canvas, div, px};
use std::cell::Cell;
use std::rc::Rc;

/// The range the top edge drags through.
const MIN_HEIGHT: f32 = 260.;
const MAX_HEIGHT: f32 = 780.;

pub struct WorkspaceSheet {
    store: Entity<Store>,
    tab: WorkspaceTab,
    changes: Entity<ChangesView>,
    pulls: Entity<ItemListView>,
    issues: Entity<ItemListView>,
    pipelines: Entity<AzureView>,
    readme: Entity<ReadmeView>,
    /// live top-edge drag: (cursor y at grab, height at grab)
    drag: Rc<Cell<Option<(Pixels, f32)>>>,
    on_close: DrawerCloseHandler,
}

impl WorkspaceSheet {
    pub fn new(
        store: Entity<Store>,
        on_close: DrawerCloseHandler,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let changes = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        let pulls = cx.new(|_| ItemListView {
            store: store.clone(),
            pull_requests: true,
        });
        let issues = cx.new(|_| ItemListView {
            store: store.clone(),
            pull_requests: false,
        });
        let pipelines = cx.new(|_| AzureView {
            store: store.clone(),
        });
        let readme = cx.new(|cx| ReadmeView::new(store.clone(), cx));
        Self {
            store,
            tab: WorkspaceTab::Changes,
            changes,
            pulls,
            issues,
            pipelines,
            readme,
            drag: Rc::new(Cell::new(None)),
            on_close,
        }
    }

    pub fn show(&mut self, tab: WorkspaceTab) {
        self.tab = tab;
    }

    /// Closes the Changes tab's branch popup from an outside press or Escape.
    /// Returns whether it was open.
    pub fn dismiss_branch_menu(&mut self, cx: &mut Context<Self>) -> bool {
        self.changes.update(cx, ChangesView::dismiss_branch_menu)
    }

    /// The tabs these settings allow: an integration that is off takes its
    /// tabs along.
    fn tabs(&self, cx: &App) -> Vec<WorkspaceTab> {
        let settings = self.store.read(cx).settings();
        WorkspaceTab::ALL
            .into_iter()
            .filter(|tab| match tab {
                WorkspaceTab::PullRequests | WorkspaceTab::Issues => settings.repos.enable_git_hub,
                WorkspaceTab::Pipelines => settings.repos.enable_azure_dev_ops,
                WorkspaceTab::Changes | WorkspaceTab::Readme => true,
            })
            .collect()
    }

    /// The top edge: drag to resize, whole pixels only.
    fn render_splitter(&self) -> AnyElement {
        let grab_store = self.store.clone();
        let grab = self.drag.clone();
        let drag = self.drag.clone();
        let store = self.store.clone();
        div()
            .id("sheet-splitter")
            .debug_selector(|| "sheet-splitter".into())
            .role(gpui::Role::Splitter)
            .aria_label("Resize sheet")
            .aria_orientation(gpui::Orientation::Vertical)
            .relative()
            .flex_shrink_0()
            .w_full()
            .h(px(6.))
            .cursor_row_resize()
            .hover(|s| s.bg(theme::HOVER_BG))
            .on_mouse_down(
                MouseButton::Left,
                move |event: &gpui::MouseDownEvent, _, cx| {
                    let height = grab_store.read(cx).sheet_height;
                    grab.set(Some((event.position.y, height)));
                    cx.stop_propagation();
                },
            )
            .child(
                canvas(
                    // this gpui rev: on_mouse_event must register from the
                    // paint closure, prepaint panics on it
                    |_, _, _| {},
                    move |_, _, window, _| {
                        if drag.get().is_none() {
                            return;
                        }
                        let drag = drag.clone();
                        let store = store.clone();
                        window.on_mouse_event(
                            move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
                                if phase != gpui::DispatchPhase::Bubble {
                                    return;
                                }
                                let Some((start_y, start_height)) = drag.get() else {
                                    return;
                                };
                                if event.pressed_button != Some(MouseButton::Left) {
                                    drag.set(None);
                                    cx.stop_propagation();
                                    return;
                                }
                                // the edge follows the cursor: up = taller
                                let delta = (event.position.y - start_y).as_f32();
                                let height =
                                    (start_height - delta).round().clamp(MIN_HEIGHT, MAX_HEIGHT);
                                store.update(cx, |s, cx| {
                                    if (s.sheet_height - height).abs() >= 0.5 {
                                        s.sheet_height = height;
                                        cx.notify();
                                    }
                                });
                            },
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }

    fn render_header(
        &self,
        tabs: &[WorkspaceTab],
        active: WorkspaceTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (repo_name, counts) = {
            let store = self.store.read(cx);
            let repo = store.selected_entry();
            let counts: Vec<Option<usize>> = tabs
                .iter()
                .map(|tab| {
                    let repo = repo?;
                    match tab {
                        WorkspaceTab::Changes => Some(repo.git_modified_count as usize),
                        WorkspaceTab::PullRequests => Some(repo.github_pr_count as usize),
                        WorkspaceTab::Issues => Some(repo.github_issue_count as usize),
                        WorkspaceTab::Pipelines | WorkspaceTab::Readme => None,
                    }
                })
                .collect();
            (repo.map(|repo| repo.name.clone()), counts)
        };

        let mut strip = segment_row();
        for (&tab, count) in tabs.iter().zip(counts) {
            strip = strip.child(segment(
                tab.selector(),
                tab.label(),
                count,
                tab == active,
                cx.listener(move |this, _, _, cx| {
                    this.tab = tab;
                    cx.notify();
                }),
            ));
        }

        let on_close = self.on_close.clone();
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(10.))
            .h(px(40.))
            .px(px(18.))
            .border_b_1()
            .border_color(theme::BORDER_LIGHT)
            .child(strip)
            .child(div().flex_1())
            .when_some(repo_name, |row, name| {
                row.child(
                    div()
                        .text_size(px(theme::font_sub()))
                        .text_color(theme::TEXT_MUTED)
                        .child(name),
                )
            })
            .child(
                div()
                    .id("sheet-close")
                    .debug_selector(|| "sheet-close".into())
                    .a11y_button("Close sheet")
                    .cursor_pointer()
                    .size(px(24.))
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|d| d.bg(theme::HOVER_BG))
                    .on_click(move |_, window, cx| on_close(window, cx))
                    .child(icon("icons/icon-close.svg", px(13.), theme::TEXT_MUTED)),
            )
            .into_any_element()
    }

    fn render_panel(&self, tab: WorkspaceTab) -> AnyElement {
        let panel = match tab {
            WorkspaceTab::Changes => self.changes.clone().into_any_element(),
            WorkspaceTab::PullRequests => self.pulls.clone().into_any_element(),
            WorkspaceTab::Issues => self.issues.clone().into_any_element(),
            WorkspaceTab::Pipelines => self.pipelines.clone().into_any_element(),
            WorkspaceTab::Readme => self.readme.clone().into_any_element(),
        };
        div()
            .debug_selector(move || format!("{}-panel", tab.selector()))
            .flex_1()
            .min_h_0()
            .px(px(10.))
            .child(panel)
            .into_any_element()
    }
}

impl Render for WorkspaceSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self.tabs(cx);
        // a tab whose integration was switched off falls back to Changes
        let active = if tabs.contains(&self.tab) {
            self.tab
        } else {
            WorkspaceTab::Changes
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(self.render_splitter())
            .child(self.render_header(&tabs, active, cx))
            .child(self.render_panel(active))
    }
}
