//! The New pull request form: the Pull requests tab's detail column while a
//! pull request is being written. It is not an overlay — the work list stays
//! beside it on a wide pane, and on a narrow one the form takes the pane with
//! Back returning to the list. The header and the Create footer stay fixed; the
//! form body scrolls.
//!
//! The form keeps to Title, Description and an optional Reviewer. Files and
//! Commits are previews of what the source branch would bring, read from the
//! repository itself — no network, no push needed to see the shape.

use crate::store::{ReviewerCandidate, Store};
use crate::theme;
use crate::ui::pull_request::{
    Shape, plural, primary_button_in, short_sha, source_tag, split_button,
};
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, MouseButton, SharedString, Window, div, px};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use warp_station_core::activity::ActivitySource;
use warp_station_core::azure::{PULL_DESCRIPTION_MAX_CHARS, PULL_TITLE_MAX_CHARS};
use warp_station_core::model::ComparePreview;
use warpui_kit_core::{field, motion, shell};
use warpui_kit_picker::{
    FilteredPicker, OverlayAnchor, PanelChrome, PickerBody, PickerData, PickerIcons, PickerItem,
    PickerRow, PickerSpec, PickerTarget, Selection, TriggerScale,
};

/// The form body's measure: one comfortable column of fields.
const FORM_WIDTH: f32 = 760.;

/// One branch pill's width: wide enough for a feature branch, narrow enough
/// that the pair and the swap fit one row.
const BRANCH_PILL_W: f32 = 220.;
/// The shared field picker's narrowest panel. The panel stops shrinking here,
/// so a row keeps more label room than the pill it hangs under.
const BRANCH_PANEL_MIN_W: f32 = 260.;

/// The app's icon paths for the shared picker shell.
const PICKER_ICONS: PickerIcons = PickerIcons {
    check: "icons/icon-check.svg",
    chevron_down: "icons/icon-chevron-down.svg",
    trash: "icons/icon-trash.svg",
    refresh: "icons/icon-refresh.svg",
};

/// The form's own tabs. They are the pull request's three, before it exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewPullTab {
    #[default]
    Overview,
    Files,
    Commits,
}

impl NewPullTab {
    pub const ALL: [NewPullTab; 3] = [NewPullTab::Overview, NewPullTab::Files, NewPullTab::Commits];

    pub fn label(self) -> &'static str {
        match self {
            NewPullTab::Overview => "Overview",
            NewPullTab::Files => "Files",
            NewPullTab::Commits => "Commits",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            NewPullTab::Overview => "overview",
            NewPullTab::Files => "files",
            NewPullTab::Commits => "commits",
        }
    }
}

pub struct NewPullView {
    store: Entity<Store>,
    tab: NewPullTab,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    reviewer: Entity<InputState>,
    /// The Source and Base branch pickers: the same branch dropdown the git
    /// toolbar uses, seated in the form's own slot.
    source_picker: Entity<FilteredPicker>,
    base_picker: Entity<FilteredPicker>,
    /// The Create split's menu.
    primary_menu: bool,
    /// The form is the whole detail pane: the workbench's list is not beside it.
    pub stacked: bool,
    /// The reviewer field's completion list is up, and the row the keys pick.
    reviewer_open: bool,
    reviewer_index: usize,
    /// The form generation the fields were seeded for; a reopened form gets
    /// the prefill's subject/body, a later answer only fills empty fields.
    form_seq: Option<u64>,
    title_seeded: bool,
    body_seeded: bool,
}

/// Which side of the branch row a picker belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchSide {
    Source,
    Base,
}

impl NewPullView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("What changed?"));
        let description =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe the change…"));
        let reviewer =
            cx.new(|cx| InputState::new(window, cx).placeholder("Reviewer login, name or email"));
        // the Azure character counters and the Create state are read in this
        // view's render, so a keystroke in either field has to re-render it
        cx.subscribe(&title, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        cx.subscribe(&description, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        cx.subscribe(&reviewer, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                this.reviewer_open = true;
                this.reviewer_index = 0;
                cx.notify();
            }
            InputEvent::Focus => {
                this.reviewer_open = true;
                this.reviewer_index = 0;
                cx.notify();
            }
            InputEvent::Blur => {
                this.reviewer_open = false;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => {}
        })
        .detach();
        let source_picker = build_branch_picker(&store, BranchSide::Source, window, cx);
        let base_picker = build_branch_picker(&store, BranchSide::Base, window, cx);
        Self {
            store,
            tab: NewPullTab::Overview,
            title,
            description,
            reviewer,
            source_picker,
            base_picker,
            primary_menu: false,
            stacked: false,
            reviewer_open: false,
            reviewer_index: 0,
            form_seq: None,
            title_seeded: false,
            body_seeded: false,
        }
    }

    fn swap(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.store.read(cx).new_pull.clone() else {
            return;
        };
        self.store.update(cx, |store, cx| {
            store.set_new_pull_branches(form.base, form.head, cx)
        });
        cx.notify();
    }

    /// Create, or Create as draft. The form always answers on Overview, where
    /// the error is visible.
    fn create(&mut self, draft: bool, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let body = self.description.read(cx).value().to_string();
        let reviewer = self.reviewer.read(cx).value().to_string();
        self.primary_menu = false;
        self.tab = NewPullTab::Overview;
        self.store.update(cx, |store, cx| {
            store.create_pull(title, body, reviewer, draft, cx)
        });
        cx.notify();
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let mut dismissed = false;
        dismissed |= self.source_picker.update(cx, FilteredPicker::dismiss);
        dismissed |= self.base_picker.update(cx, FilteredPicker::dismiss);
        if dismissed {
            cx.notify();
            return true;
        }
        if self.primary_menu {
            self.primary_menu = false;
            cx.notify();
            return true;
        }
        false
    }
}

impl Render for NewPullView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // the form's prefill is a read: seed the title from the workspace's
        // tip commit and the body from base..head when the net answer lands,
        // never over text the user already typed
        let form = self.store.read(cx).new_pull.clone();
        if let Some(form) = form.as_ref() {
            if self.form_seq != Some(form.seq) {
                self.form_seq = Some(form.seq);
                self.reviewer_open = false;
                self.reviewer_index = 0;
                self.store
                    .update(cx, |store, cx| store.load_new_pull_reviewers(cx));
                self.title_seeded = false;
                self.body_seeded = false;
                self.title
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.description
                    .update(cx, |state, cx| state.set_value("", window, cx));
            }
            if !self.title_seeded
                && !form.prefill_title.trim().is_empty()
                && self.title.read(cx).value().trim().is_empty()
            {
                let title = form.prefill_title.clone();
                self.title
                    .update(cx, |state, cx| state.set_value(&title, window, cx));
                self.title_seeded = true;
            }
            if !self.body_seeded
                && !form.prefill_body.trim().is_empty()
                && self.description.read(cx).value().trim().is_empty()
            {
                let body = form.prefill_body.clone();
                self.description
                    .update(cx, |state, cx| state.set_value(&body, window, cx));
                self.body_seeded = true;
            }
        }
        let this = cx.entity();
        self.store.update(cx, |store, _| store.load_compare());

        let store = self.store.read(cx);
        let Some(form) = store.new_pull.clone() else {
            return div().into_any_element();
        };
        let pair = (form.head.trim().to_string(), form.base.trim().to_string());
        let pair_ready = !pair.0.is_empty() && !pair.1.is_empty() && pair.0 != pair.1;
        let preview = if !pair_ready {
            PreviewState::NoPair
        } else if !store.compare_answered() {
            PreviewState::Waiting
        } else {
            match store.compare_preview() {
                Some(preview) => PreviewState::Ready(preview),
                None => PreviewState::Nothing,
            }
        };
        let repo_name = store
            .selected_entry()
            .map(|repo| repo.name.clone())
            .unwrap_or_default();
        let creating = form.creating;
        let error = form.error.clone();
        let source = form.source;
        // Azure DevOps caps a pull request's title and description; the form
        // counts as the create body would see the values and says the limit
        // beside each field, so the failure is named before the call.
        let limits = (source == ActivitySource::Azure).then(|| {
            (
                self.title.read(cx).value().trim().chars().count(),
                self.description.read(cx).value().chars().count(),
            )
        });
        let over_limits = limits.is_some_and(|(title, description)| {
            title > PULL_TITLE_MAX_CHARS || description > PULL_DESCRIPTION_MAX_CHARS
        });
        let files = preview.counts().0;
        let commits = preview.counts().1;

        let body = match self.tab {
            NewPullTab::Overview => self.overview(&error, limits, window, cx),
            NewPullTab::Files => preview_body(
                "new-pull-files",
                &preview,
                "file",
                "No files changed",
                "This branch has no changes against the base yet.",
                files_body,
            ),
            NewPullTab::Commits => preview_body(
                "new-pull-commits",
                &preview,
                "commit",
                "No commits yet",
                "Commits on this branch will show here.",
                commits_body,
            ),
        };

        let pane = div()
            .debug_selector(|| "new-pull".into())
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(self.head_row(&repo_name, source, &this))
            .child(self.branch_row(cx))
            .child(self.tab_strip(files, commits, &this))
            .child(body)
            .child(self.footer(creating, over_limits, &this));
        pane.children(
            self.primary_menu
                .then(|| self.create_menu(creating, over_limits, &this)),
        )
        .into_any_element()
    }
}

impl NewPullView {
    // ---- head -------------------------------------------------------------

    fn head_row(&self, repo_name: &str, source: ActivitySource, this: &Entity<Self>) -> AnyElement {
        let mut title_row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .min_w(px(220.))
                    .flex_1()
                    .text_size(px(theme::font_screen_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child("New pull request"),
            )
            .child(source_tag(source.label()));
        if self.stacked {
            let store = self.store.clone();
            let this = this.clone();
            title_row = title_row.child(
                div()
                    .id("new-pull-back")
                    .debug_selector(|| "new-pull-back".into())
                    .a11y_button("Back to pull requests")
                    .cursor_pointer()
                    .flex_shrink_0()
                    .size(px(shell::CONTROL_HEIGHT))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(move |_, _, cx| {
                        this.update(cx, |this, cx| {
                            this.primary_menu = false;
                            cx.notify();
                        });
                        store.update(cx, |store, cx| store.close_new_pull(cx));
                    })
                    .child(icon(
                        "icons/icon-arrow-left.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            );
        }

        div()
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .pt_4()
            .pb(px(14.))
            .flex()
            .flex_col()
            .child(title_row)
            .child(
                div()
                    .mt_2()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_dim())
                    .child(format!("Open a pull request in {repo_name}.")),
            )
            .into_any_element()
    }

    // ---- branches ---------------------------------------------------------

    fn branch_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let swap = this.clone();
        div()
            .debug_selector(|| "new-pull-branches".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .pb(px(16.))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(div().w(px(BRANCH_PILL_W)).child(self.source_picker.clone()))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child("into"),
            )
            .child(div().w(px(BRANCH_PILL_W)).child(self.base_picker.clone()))
            // the swap belongs to the pair it swaps: right after the base
            // picker, not pushed to the pane's far edge
            .child(
                div()
                    .id("new-pull-swap")
                    .debug_selector(|| "new-pull-swap".into())
                    .a11y_button("Swap source and base")
                    .cursor_pointer()
                    .flex_shrink_0()
                    .size(px(shell::CONTROL_HEIGHT))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(move |_, _, cx| {
                        swap.update(cx, |view, cx| view.swap(cx));
                    })
                    .child(icon(
                        "icons/icon-swap.svg",
                        px(theme::icon_small()),
                        theme::text_muted(),
                    )),
            )
            .into_any_element()
    }

    // ---- tabs -------------------------------------------------------------

    fn tab_strip(&self, files: usize, commits: usize, this: &Entity<Self>) -> AnyElement {
        let mut strip = div()
            .debug_selector(|| "new-pull-tabs".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .flex()
            .flex_row()
            .border_t_1()
            .border_b_1()
            .border_color(theme::hairline());
        for tab in NewPullTab::ALL {
            let count = match tab {
                NewPullTab::Files => files,
                NewPullTab::Commits => commits,
                NewPullTab::Overview => 0,
            };
            let selector = format!("new-pull-tab-{}", tab.key());
            let this = this.clone();
            strip = strip.child(
                shell::detail_tab(selector.clone(), tab.label(), self.tab == tab)
                    .debug_selector(move || selector.clone())
                    .on_click(move |_, _, cx| {
                        this.update(cx, |view, cx| {
                            view.tab = tab;
                            view.primary_menu = false;
                            cx.notify();
                        });
                    })
                    .children((count > 0).then(|| {
                        div()
                            .text_size(px(theme::font_meta()))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::text_muted())
                            .child(format!("{count}"))
                    })),
            );
        }
        strip.into_any_element()
    }

    // ---- Overview ---------------------------------------------------------

    fn overview(
        &mut self,
        error: &Option<String>,
        limits: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fields = div()
            .w_full()
            .max_w(px(FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(14.))
            .children(error.as_ref().map(|error| {
                div()
                    .debug_selector(|| "new-pull-error".into())
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme::DANGER)
                    .bg(theme::tint(theme::DANGER))
                    .px_3()
                    .py_2()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::DANGER)
                    .child(error.clone())
            }))
            .child(labeled_field(
                "Title (required)",
                limits.map(|(title, _)| {
                    limit_note("new-pull-title-limit", title, PULL_TITLE_MAX_CHARS)
                }),
                field::text("new-pull-title", &self.title, window, cx),
            ))
            .child(labeled_field(
                "Description",
                limits.map(|(_, description)| {
                    limit_note(
                        "new-pull-description-limit",
                        description,
                        PULL_DESCRIPTION_MAX_CHARS,
                    )
                }),
                // the design's four-line message box, pinned: the area's own
                // auto-height resolves its `h_full` editor to one row, which
                // showed the first line and left the rest empty. A definite
                // height gives the editor all four rows; a longer value
                // scrolls inside the box and the Overview scrolls as a whole.
                field::area("new-pull-description", &self.description, window, cx)
                    .h(px(120.))
                    .min_h(px(0.)),
            ))
            .child(labeled_field(
                "Reviewer (optional)",
                None,
                self.reviewer_field(window, cx),
            ));
        div()
            .id("new-pull-overview")
            .debug_selector(|| "new-pull-overview".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(shell::DETAIL_PADDING))
            .py(px(18.))
            .child(fields)
            .into_any_element()
    }

    /// The reviewer field and, while it has the keys and a candidate exists,
    /// the completion list under it: Up/Down move, Enter takes, Esc closes; a
    /// press takes the row under the pointer.
    fn reviewer_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let matches = self.reviewer_matches(cx);
        let open = self.reviewer_open && !matches.is_empty();
        let selected = self.reviewer_index.min(matches.len().saturating_sub(1));
        let mut column = div()
            .id("new-pull-reviewer-wrap")
            .flex()
            .flex_col()
            .child(field::text("new-pull-reviewer", &self.reviewer, window, cx));
        if open {
            let mut card = div()
                .id("new-pull-reviewer-suggestions")
                .debug_selector(|| "new-pull-reviewer-suggestions".into())
                .mt_1()
                .overflow_hidden()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::line_strong())
                .bg(theme::well())
                .flex()
                .flex_col()
                .py_1();
            for (index, candidate) in matches.iter().enumerate() {
                card = card.child(self.reviewer_suggestion_row(index, candidate, selected, cx));
            }
            column = column.child(card);
        }
        column.on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            if !this.reviewer_open {
                return;
            }
            let matches = this.reviewer_matches(cx);
            if matches.is_empty() {
                return;
            }
            let last = matches.len() - 1;
            match event.keystroke.key.to_lowercase().as_str() {
                "down" => {
                    this.reviewer_index = (this.reviewer_index + 1).min(last);
                    cx.stop_propagation();
                    cx.notify();
                }
                "up" => {
                    this.reviewer_index = this.reviewer_index.min(last).saturating_sub(1);
                    cx.stop_propagation();
                    cx.notify();
                }
                "enter" => {
                    let value = matches[this.reviewer_index.min(last)].value.clone();
                    this.pick_reviewer(value, window, cx);
                    cx.stop_propagation();
                }
                "escape" => {
                    this.reviewer_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }
                _ => {}
            }
        }))
    }

    fn reviewer_matches(&self, cx: &gpui::App) -> Vec<ReviewerCandidate> {
        let query = self.reviewer.read(cx).value().trim().to_lowercase();
        self.store
            .read(cx)
            .new_pull_reviewers()
            .into_iter()
            .filter(|candidate| {
                query.is_empty()
                    || candidate.label.to_lowercase().contains(&query)
                    || candidate.value.to_lowercase().contains(&query)
                    || candidate.hint.to_lowercase().contains(&query)
            })
            .take(8)
            .collect()
    }

    fn pick_reviewer(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        self.reviewer.update(cx, |state, cx| {
            state.set_value(value, window, cx);
            state.focus(window, cx);
        });
        self.reviewer_open = false;
        self.reviewer_index = 0;
        cx.notify();
    }

    fn reviewer_suggestion_row(
        &self,
        index: usize,
        candidate: &ReviewerCandidate,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = candidate.value.clone();
        let label = candidate.label.clone();
        let hint = candidate.hint.clone();
        div()
            .id(SharedString::from(format!("new-pull-reviewer-row-{index}")))
            .debug_selector(move || format!("new-pull-reviewer-suggestion-{index}"))
            .a11y_button(format!("Use reviewer {label}"))
            .cursor_pointer()
            .h(px(34.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .when(index == selected, |row| row.bg(theme::raised()))
            .hover(|row| row.bg(theme::raised()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.pick_reviewer(value.clone(), window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text())
                    .child(label),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_faint())
                    .child(hint),
            )
            .into_any_element()
    }

    // ---- footer -----------------------------------------------------------

    fn footer(&self, creating: bool, over_limits: bool, this: &Entity<Self>) -> AnyElement {
        let cancel = {
            let store = self.store.clone();
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |this, cx| {
                    this.primary_menu = false;
                    cx.notify();
                });
                store.update(cx, |store, cx| store.close_new_pull(cx));
            }
        };
        let label = if creating {
            "Creating…"
        } else {
            "Create pull request"
        };
        let create = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| view.create(false, cx));
            }
        };
        let menu = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| {
                    view.primary_menu = !view.primary_menu;
                    cx.notify();
                });
            }
        };
        let primary = if creating {
            div()
                .id("new-pull-create")
                .debug_selector(|| "new-pull-create".into())
                .a11y_button(label)
                .tooltip(crate::ui::tooltip_text("Creating the pull request"))
                .flex_shrink_0()
                .h(px(shell::CONTROL_HEIGHT))
                .px_3()
                .rounded_l(px(6.))
                .bg(theme::accent())
                .opacity(0.6)
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::on_accent())
                .child(motion::spinner(
                    "new-pull-spinner",
                    "icons/icon-spinner.svg",
                    px(theme::icon_small()),
                    theme::on_accent(),
                ))
                .child(label)
                .into_any_element()
        } else {
            primary_button_in("new-pull-create", label, !over_limits, Shape::Left, create)
        };

        div()
            .debug_selector(|| "new-pull-foot".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .py(px(14.))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_t_1()
            .border_color(theme::hairline())
            .bg(theme::panel())
            .child(div().flex_1())
            .child(crate::ui::neutral_button(
                "new-pull-cancel",
                "Cancel",
                !creating,
                cancel,
            ))
            .child(split_button("new-pull-primary", primary, true, menu))
            .into_any_element()
    }

    fn create_menu(&self, creating: bool, over_limits: bool, this: &Entity<Self>) -> AnyElement {
        let mut menu = crate::ui::pull_request::menu_shell("new-pull-menu");
        let create = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| view.create(false, cx));
            }
        };
        let draft = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| view.create(true, cx));
            }
        };
        menu = menu
            .child(crate::ui::pull_request::menu_item(
                "new-pull-menu-create",
                "icons/icon-git-pull-request.svg",
                "Create pull request",
                !creating && !over_limits,
                create,
            ))
            .child(crate::ui::pull_request::menu_item(
                "new-pull-menu-draft",
                "icons/icon-text.svg",
                "Create as draft",
                !creating && !over_limits,
                draft,
            ));
        // the form's head sits at the pane's top; the menu hangs under it
        gpui::deferred(
            div()
                .absolute()
                .top(px(56.))
                .right(px(warpui_kit_core::shell::DETAIL_PADDING))
                .child(menu),
        )
        .into_any_element()
    }
}

/// A labelled box: the design's `label > span` pair above a field. `note` sits
/// at the label's right edge — the Azure character count, when there is one.
fn labeled_field(
    label: &'static str,
    note: Option<AnyElement>,
    field: gpui::Stateful<gpui::Div>,
) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_dim())
                        .child(label),
                )
                .child(div().flex_1())
                .children(note),
        )
        .child(field)
        .into_any_element()
}

/// The Azure limit beside a field's label: the count and the cap, in the
/// danger ink once the value is past it.
fn limit_note(selector: &'static str, count: usize, max: usize) -> AnyElement {
    let over = count > max;
    div()
        .debug_selector(move || selector.into())
        .text_size(px(theme::font_meta()))
        .font_family(SharedString::from(theme::code_font()))
        .text_color(if over {
            theme::DANGER
        } else {
            theme::text_faint()
        })
        .child(format!("{count} / {max} characters"))
        .into_any_element()
}

/// What the preview can say: the branch pair is not a pair yet, the compare is
/// still out, it found nothing, or it has rows.
enum PreviewState<'a> {
    NoPair,
    Waiting,
    Nothing,
    Ready(&'a ComparePreview),
}

impl PreviewState<'_> {
    /// The counts the tab labels carry, when there are any.
    fn counts(&self) -> (usize, usize) {
        match self {
            PreviewState::Ready(preview) => (preview.files.len(), preview.commits.len()),
            _ => (0, 0),
        }
    }
}

/// The Files and Commits tabs share one shape: a summary line, then either the
/// rows or an empty state that says which one it is.
fn preview_body(
    selector: &'static str,
    state: &PreviewState<'_>,
    noun: &'static str,
    empty_title: &'static str,
    empty_detail: &'static str,
    rows: impl Fn(&ComparePreview) -> AnyElement,
) -> AnyElement {
    let preview = match state {
        PreviewState::Ready(preview) => preview,
        PreviewState::NoPair => {
            return plain_state(
                "Pick two branches",
                "The preview compares Source against Base.",
            );
        }
        PreviewState::Waiting => return plain_state("Reading the branches…", ""),
        PreviewState::Nothing => {
            return plain_state("Nothing to compare", "These branches bring no changes.");
        }
    };
    if preview.files.is_empty() && preview.commits.is_empty() {
        return plain_state(empty_title, empty_detail);
    }
    let own = if noun == "file" {
        preview.files.len()
    } else {
        preview.commits.len()
    };
    if own == 0 {
        return plain_state(empty_title, empty_detail);
    }
    let body = rows(preview);
    div()
        .id(SharedString::from(selector))
        .debug_selector(move || selector.into())
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(shell::DETAIL_PADDING))
        .py(px(18.))
        .child(
            div()
                .w_full()
                .max_w(px(FORM_WIDTH))
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .pb_3()
                        .border_b_1()
                        .border_color(theme::row_line())
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .child(format!("{own} {noun}{}", plural(own))),
                )
                .child(body),
        )
        .into_any_element()
}

fn files_body(preview: &ComparePreview) -> AnyElement {
    let (adds, dels) = preview.line_counts();
    let mut list = div().flex().flex_col();
    for file in &preview.files {
        let (dir, name) = split_path(&file.path);
        list = list.child(
            div()
                .debug_selector(|| "new-pull-file".into())
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(34.))
                .border_b_1()
                .border_color(theme::row_line())
                .text_size(px(theme::font_meta()))
                .text_color(theme::text())
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(14.))
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_mono_meta()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(super::diff_controls::status_color(&file.status_code))
                        .child(file.status_code.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(
                            div()
                                .flex_shrink(1.)
                                .min_w(px(0.))
                                .text_color(theme::text_muted())
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(dir),
                        )
                        .child(div().flex_shrink_0().child(name)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .gap_1p5()
                        .font_family(theme::code_font())
                        .child(
                            div()
                                .text_color(theme::SUCCESS_BRIGHT)
                                .child(format!("+{}", file.additions.unwrap_or(0))),
                        )
                        .child(
                            div()
                                .text_color(theme::DANGER)
                                .child(format!("−{}", file.deletions.unwrap_or(0))),
                        ),
                ),
        );
    }
    div()
        .flex()
        .flex_col()
        .child(list)
        .child(
            div()
                .pt_2()
                .flex()
                .gap_2()
                .font_family(theme::code_font())
                .text_size(px(theme::font_meta()))
                .child(
                    div()
                        .text_color(theme::SUCCESS_BRIGHT)
                        .child(format!("+{adds}")),
                )
                .child(div().text_color(theme::DANGER).child(format!("−{dels}"))),
        )
        .into_any_element()
}

fn commits_body(preview: &ComparePreview) -> AnyElement {
    let mut list = div().flex().flex_col();
    for commit in &preview.commits {
        let age = commit
            .committed_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339)
            .unwrap_or_default();
        let who = commit.author.clone().unwrap_or_else(|| "Unknown".into());
        list = list.child(
            div()
                .debug_selector(|| "new-pull-commit".into())
                .flex()
                .items_start()
                .gap_2p5()
                .py_2()
                .border_b_1()
                .border_color(theme::row_line())
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_mono_meta()))
                        .text_color(theme::text_muted())
                        .child(short_sha(&commit.hash)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_size(px(theme::font_body()))
                                .text_color(theme::text())
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(commit.subject.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .child(format!("{who} · {age}")),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(format!("{} file{}", commit.files, plural(commit.files))),
                ),
        );
    }
    list.into_any_element()
}

fn plain_state(title: &'static str, detail: &'static str) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(empty_state(title, detail))
        .into_any_element()
}

fn empty_state(title: &'static str, detail: &'static str) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .p_6()
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child(title),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(detail),
        )
        .into_any_element()
}

fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_string()),
        None => (String::new(), path.to_string()),
    }
}

/// The shared branch dropdown, seated in the form: the trigger is the pill and
/// the panel floats under it.
fn build_branch_picker(
    store: &Entity<Store>,
    side: BranchSide,
    window: &mut Window,
    cx: &mut Context<NewPullView>,
) -> Entity<FilteredPicker> {
    let (trigger_label, trigger_id, panel_id) = match side {
        BranchSide::Source => ("Source branch", "new-pull-source", "new-pull-branch-menu"),
        BranchSide::Base => ("Base branch", "new-pull-base", "new-pull-branch-menu-base"),
    };
    let spec = PickerSpec {
        key: trigger_label,
        trigger_label,
        filter_placeholder: "Filter branches",
        icons: PICKER_ICONS,
        trigger_icon: Some("icons/icon-source-branch.svg"),
        provider_badge: false,
        refresh: false,
        refresh_label: "",
        stop_trigger_mousedown: true,
        visible_rows: 8,
        selected_accent: true,
        overlay: true,
        anchor: OverlayAnchor::Field,
        trigger_scale: TriggerScale::Compact,
        trigger_rounded_lg: false,
        trigger_min_h: Some(shell::CONTROL_HEIGHT),
        panel_chrome: PanelChrome::Menus,
        trigger_id: Some(trigger_id),
        panel_id: Some(panel_id),
        filter_selector: Some("new-pull-branch-filter"),
        list_id: Some("new-pull-branch-list"),
    };
    let pick_store = store.clone();
    let picker = cx.new(|cx| {
        FilteredPicker::new(
            spec,
            PickerTarget::Callback(Box::new(move |value, _window, cx| {
                let Some(form) = pick_store.read(cx).new_pull.clone() else {
                    return;
                };
                let (head, base) = match side {
                    BranchSide::Source => (value.to_string(), form.base),
                    BranchSide::Base => (form.head, value.to_string()),
                };
                pick_store.update(cx, |store, cx| store.set_new_pull_branches(head, base, cx));
            })),
            Box::new(|_, _, _| unreachable!("the real source lands right after construction")),
            window,
            cx,
        )
    });
    let data_store = store.clone();
    picker.update(cx, |picker, cx| {
        picker.set_data_source(Box::new(
            move |filter: &str, window: &Window, cx: &gpui::App| {
                branch_picker_data(&data_store, side, filter, window, cx)
            },
        ));
        picker.observe_entity(store, cx);
    });
    picker
}

/// One render's worth of the branch list: locals first, the inert "Remote"
/// group label before the first remote, and the current side's branch ticked.
fn branch_picker_data(
    store: &Entity<Store>,
    side: BranchSide,
    filter: &str,
    window: &Window,
    cx: &gpui::App,
) -> PickerData {
    let store_read = store.read(cx);
    let form = store_read.new_pull.clone();
    let current = form
        .as_ref()
        .map(|form| match side {
            BranchSide::Source => form.head.clone(),
            BranchSide::Base => form.base.clone(),
        })
        .unwrap_or_default();
    let filter = filter.to_lowercase();
    let mut rows = Vec::new();
    let mut remotes_labeled = false;
    for branch in &store_read.branches {
        if !filter.is_empty() && !branch.name.to_lowercase().contains(&filter) {
            continue;
        }
        let is_remote = branch.kind == warp_station_core::model::GitBranchKind::Remote;
        if is_remote && !remotes_labeled {
            remotes_labeled = true;
            rows.push(PickerRow::Group {
                id: "new-pull-branch-group-remote".into(),
                label: "Remote".into(),
                selector: Some("new-pull-branch-group-remote".into()),
            });
        }
        rows.push(PickerRow::Item(PickerItem {
            id: format!("new-pull-branch-{}", branch.name),
            selector: format!("new-pull-branch-{}", branch.name),
            value: branch.name.clone(),
            label: crate::ui::branch_label(
                &branch.name,
                crate::ui::branch_row_label_width(BRANCH_PANEL_MIN_W),
                window,
            ),
            selected: branch.name == current,
            muted: is_remote,
            trailing_delete: None,
        }));
    }
    if rows.is_empty() {
        rows.push(PickerRow::Item(PickerItem {
            id: "new-pull-branch-empty".into(),
            selector: "new-pull-branch-empty".into(),
            value: String::new(),
            label: "No branches yet".into(),
            selected: false,
            muted: true,
            trailing_delete: None,
        }));
    }
    PickerData {
        body: PickerBody::Rows(rows),
        selected: Selection {
            value: current.clone(),
            label: if current.is_empty() {
                "Select a branch".to_string()
            } else {
                crate::ui::branch_label(
                    &current,
                    crate::ui::branch_trigger_label_width(BRANCH_PILL_W),
                    window,
                )
            },
            badge: None,
        },
        footer: None,
        fonts: theme::active_tokens(),
        accent: theme::blue_bright(),
        show_tooltips: store_read.settings().general.show_tooltips,
        refreshing: false,
    }
}
