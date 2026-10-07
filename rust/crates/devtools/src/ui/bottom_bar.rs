//! The bottom bar: the selected repo's workspace — a tab strip (Overview /
//! Changes / Pull Requests / Issues / Pipelines) above the active panel. The
//! bar itself is store-driven; only ephemeral panel state lives here.

use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::changes::ChangesView;
use crate::ui::icon;
use crate::ui::azure_tab::AzureView;
use crate::ui::overview::OverviewView;
use crate::ui::pr_issues::ItemListView;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};

pub struct BottomBar {
    pub store: Entity<Store>,
    changes_view: Entity<ChangesView>,
    overview_view: Entity<OverviewView>,
    pr_view: Entity<ItemListView>,
    issue_view: Entity<ItemListView>,
    azure_view: Entity<AzureView>,
}

impl BottomBar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let changes_view = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        let overview_view = cx.new(|cx| OverviewView::new(store.clone(), window, cx));
        let pr_view = cx.new(|_| ItemListView { store: store.clone(), pull_requests: true });
        let issue_view = cx.new(|_| ItemListView { store: store.clone(), pull_requests: false });
        let azure_view = cx.new(|_| AzureView { store: store.clone() });
        Self { store, changes_view, overview_view, pr_view, issue_view, azure_view }
    }
}

impl Render for BottomBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let Some(entry) = self.store.read(cx).selected_entry().cloned() else {
            return div().into_any_element();
        };
        let selected_tab = self.store.read(cx).selected_tab;

        let mut tabs = div().flex().flex_row().gap_1();
        for tab in [
            BottomBarTab::Overview,
            BottomBarTab::Changes,
            BottomBarTab::PullRequests,
            BottomBarTab::Issues,
            BottomBarTab::Azure,
        ] {
            let active = tab == selected_tab;
            tabs = tabs.child(
                div()
                    .id(SharedString::from(format!("tab-{}", tab.label())))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2p5()
                    .py_1()
                    .text_size(px(theme::FONT_SMALL))
                    .when(active, |d| {
                        d.bg(theme::tint(theme::ACCENT))
                            .text_color(theme::ACCENT)
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                    .hover(|s| s.opacity(0.85))
                    .on_click({
                        let store = self.store.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.set_tab(tab, cx));
                        }
                    })
                    .child(tab.label()),
            );
        }

        let store_handle = self.store.clone();
        div()
            .h(px(340.))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::STRONG_BACKGROUND)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .child(
                        div()
                            .text_size(px(theme::FONT_BODY))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(entry.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(theme::FONT_SMALL))
                            .text_color(theme::TEXT_MUTED)
                            .child(entry.git_branch_name.clone().unwrap_or_else(|| "—".into())),
                    )
                    .child(div().w(px(8.)))
                    .child(tabs)
                    .child(div().flex_1())
                    .when_some(self.store.read(cx).last_git_error.clone(), |row, error| {
                        row.child(
                            div()
                                .text_size(px(theme::FONT_SMALL))
                                .text_color(theme::DANGER)
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .id("close-bar")
                            .cursor_pointer()
                            .rounded_md()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(move |_, _, cx| {
                                store_handle.update(cx, |s, cx| s.select_repo(None, cx));
                            })
                            .child(icon(
                                "icons/icon-window-close.svg",
                                px(theme::ICON_SMALL),
                                theme::TEXT_MUTED,
                            )),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.render_panel(selected_tab, cx)))
            .into_any_element()
    }
}

impl BottomBar {
    fn render_panel(&mut self, tab: BottomBarTab, cx: &mut Context<Self>) -> gpui::AnyElement {
        match tab {
            BottomBarTab::Changes => self.changes_view.clone().into_any_element(),
            BottomBarTab::Overview => self.overview_view.clone().into_any_element(),
            BottomBarTab::PullRequests => self.pr_view.clone().into_any_element(),
            BottomBarTab::Issues => self.issue_view.clone().into_any_element(),
            BottomBarTab::Azure => self.azure_view.clone().into_any_element(),
        }
    }
}

