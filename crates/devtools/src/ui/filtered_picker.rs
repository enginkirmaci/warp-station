//! The filtered picker: a select-style trigger over a filtered list. Three
//! slots share one implementation — the two OpenCode model slots on the
//! Settings page (virtualized rows over the catalog) and the Changes tab's
//! branch selector — and filter typing repaints only the picker.

use crate::net::GitAction;
use crate::store::Store;
use crate::theme;
use crate::ui::{icon, with_tooltip};
use gpui::prelude::*;
use gpui::{
    div, px, uniform_list, App, Context, Entity, FontWeight, MouseButton, SharedString, Window,
};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::Sizable as _;

const ROW_HEIGHT: f32 = 28.;
const MODEL_VISIBLE_ROWS: usize = 10;
/// Branch rows scroll after this many; also the old dropdown's max-h (~220px).
const BRANCH_VISIBLE_ROWS: usize = 8;
/// The old dropdown's render cap — the filter reaches anything past it.
const BRANCH_ROW_CAP: usize = 30;

/// What the picker lists. The Settings model slots write the pick into a
/// text field; the branch selector checks the picked branch out.
enum PickerSource {
    Models {
        field: Entity<InputState>,
        default_label: &'static str,
    },
    Branches,
}

struct PickerItem {
    /// Model id or branch name: the pick payload and the element identity.
    value: String,
    label: String,
    selected: bool,
    /// Branch mode: a remote-tracking row (faint, never deletable).
    remote: bool,
    /// Branch mode: a local non-current row — carries the armed trash.
    deletable: bool,
}

enum PickerRow {
    Group {
        id: String,
        label: String,
        selector: Option<&'static str>,
    },
    Item(PickerItem),
    Message(String),
}

/// Everything the row builders need, gathered once per render so the
/// virtualized row processor closes over one small struct.
struct RowCtx {
    branch: bool,
    key: &'static str,
    field: Option<Entity<InputState>>,
    store: Entity<Store>,
    show_tooltips: bool,
    /// Branch whose trash was clicked once (branch mode only).
    armed: Option<String>,
}

fn model_provider(model: &str) -> &str {
    model.split_once('/').map(|(provider, _)| provider).unwrap_or("")
}

fn model_name(model: &str) -> &str {
    model.split_once('/').map(|(_, name)| name).unwrap_or(model)
}

pub struct FilteredPicker {
    source: PickerSource,
    filter: Entity<InputState>,
    store: Entity<Store>,
    key: &'static str,
    open: bool,
    /// Branch whose trash was clicked once — the second click deletes.
    branch_delete_armed: Option<String>,
    new_branch_open: bool,
    new_branch_name: Option<Entity<InputState>>,
    /// The repo the ephemeral branch state belongs to; a mismatch resets it.
    loaded_for: Option<String>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl FilteredPicker {
    /// The Settings model slot: the pick lands in `field`.
    pub fn new(
        field: Entity<InputState>,
        store: Entity<Store>,
        key: &'static str,
        default_label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter models"));
        Self::build(
            PickerSource::Models { field, default_label },
            store,
            key,
            filter,
            None,
            cx,
        )
    }

    /// The Changes-tab branch selector: a pick checks the branch out; locals
    /// carry the delete trash and the popup carries the new-branch form.
    pub fn branches(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter branches"));
        let new_branch_name = cx.new(|cx| InputState::new(window, cx).placeholder("branch name"));
        Self::build(
            PickerSource::Branches,
            store,
            "Branch",
            filter,
            Some(new_branch_name),
            cx,
        )
    }

    fn build(
        source: PickerSource,
        store: Entity<Store>,
        key: &'static str,
        filter: Entity<InputState>,
        new_branch_name: Option<Entity<InputState>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sub_filter = cx.observe(&filter, |_, _, cx| cx.notify());
        let sub_store = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            source,
            filter,
            store,
            key,
            open: false,
            branch_delete_armed: None,
            new_branch_open: false,
            new_branch_name,
            loaded_for: None,
            _subscriptions: vec![sub_filter, sub_store],
        }
    }

    fn is_branch(&self) -> bool {
        matches!(self.source, PickerSource::Branches)
    }

    /// Closes the popup and clears the ephemeral branch state. The branch
    /// mode is wired to the window's outside-click and Escape dismissal
    /// through this. Returns whether it was actually open.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        self.new_branch_open = false;
        self.branch_delete_armed = None;
        cx.notify();
        true
    }

    /// One-time reset when the selected repo changed: the popup must never
    /// show the previous repo's branch list, filter or form.
    fn sync_repo_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_branch() {
            return;
        }
        let repo = self.store.read(cx).selected_repo.clone();
        if self.loaded_for == repo {
            return;
        }
        self.loaded_for = repo;
        self.open = false;
        self.branch_delete_armed = None;
        self.new_branch_open = false;
        self.filter.update(cx, |f, cx| f.set_value("", window, cx));
    }

    fn rows(&self, cx: &App) -> Vec<PickerRow> {
        let filter = self.filter.read(cx).value().trim().to_lowercase();
        match &self.source {
            PickerSource::Models { field, default_label } => {
                let selected = field.read(cx).value().trim().to_string();
                let mut models: Vec<String> = self
                    .store
                    .read(cx)
                    .models
                    .iter()
                    .filter(|model| {
                        let model = model.trim();
                        !model.is_empty()
                            && !model.starts_with('(')
                            && (filter.is_empty() || model.to_lowercase().contains(&filter))
                    })
                    .cloned()
                    .collect();
                models.sort_by(|a, b| {
                    model_provider(a)
                        .to_lowercase()
                        .cmp(&model_provider(b).to_lowercase())
                        .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
                });
                models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

                let mut rows = Vec::with_capacity(models.len() + 4);
                rows.push(PickerRow::Item(PickerItem {
                    value: String::new(),
                    label: default_label.to_string(),
                    selected: selected.is_empty(),
                    remote: false,
                    deletable: false,
                }));
                if models.is_empty() {
                    rows.push(PickerRow::Message(
                        "No models found. Start warp-agent, then refresh.".to_string(),
                    ));
                } else {
                    let mut last_provider = String::new();
                    for model in models {
                        let provider = model_provider(&model).to_string();
                        if !provider.is_empty() && provider != last_provider {
                            rows.push(PickerRow::Group {
                                id: format!("model-provider-{}-{provider}", self.key),
                                label: provider.to_uppercase(),
                                selector: None,
                            });
                            last_provider = provider;
                        }
                        rows.push(PickerRow::Item(PickerItem {
                            value: model.clone(),
                            label: model_name(&model).to_string(),
                            selected: model.eq_ignore_ascii_case(&selected),
                            remote: false,
                            deletable: false,
                        }));
                    }
                }
                rows
            }
            PickerSource::Branches => {
                let store = self.store.read(cx);
                let current = store
                    .selected_entry()
                    .and_then(|e| e.git_branch_name.clone())
                    .unwrap_or_default();
                let mut rows = Vec::new();
                let mut remotes_labeled = false;
                for branch in &store.branches {
                    if !filter.is_empty() && !branch.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    let is_remote = branch.kind == devtools_core::model::GitBranchKind::Remote;
                    // locals first, then the inert "Remote" group label
                    // (re-emerges before the first matching remote while
                    // filtering)
                    if is_remote && !remotes_labeled {
                        remotes_labeled = true;
                        rows.push(PickerRow::Group {
                            id: "branch-group-remote".into(),
                            label: "Remote".into(),
                            selector: Some("branch-group-remote"),
                        });
                    }
                    rows.push(PickerRow::Item(PickerItem {
                        value: branch.name.clone(),
                        label: branch.name.clone(),
                        selected: branch.name == current,
                        remote: is_remote,
                        // locals only, never the checked-out branch — git
                        // refuses the latter anyway, so no trash is offered
                        deletable: !is_remote && branch.name != current,
                    }));
                }
                rows.truncate(BRANCH_ROW_CAP);
                rows
            }
        }
    }

    fn group_row(id: &str, label: &str, selector: Option<&'static str>) -> gpui::AnyElement {
        let mut row = div()
            .id(SharedString::from(id.to_string()))
            .w_full()
            .h(px(ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .text_size(px(theme::font_small() - 1.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::TEXT_FAINT);
        if let Some(selector) = selector {
            row = row.debug_selector(move || selector.to_string());
        }
        row.child(label.to_string()).into_any_element()
    }

    fn message_row(ctx: &RowCtx, label: &str) -> gpui::AnyElement {
        let id = if ctx.branch {
            "branch-message".to_string()
        } else {
            format!("model-message-{}", ctx.key)
        };
        div()
            .id(SharedString::from(id))
            .w_full()
            .h(px(ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .text_size(px(theme::font_small()))
            .text_color(theme::TEXT_FAINT)
            .child(label.to_string())
            .into_any_element()
    }

    fn item_row(ctx: &RowCtx, item: &PickerItem, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (id, selector, color, weight) = if ctx.branch {
            let selector = format!("branch-row-{}", item.value);
            let color = if item.selected {
                theme::ACCENT
            } else if item.remote {
                theme::TEXT_FAINT
            } else {
                theme::TEXT_DIM
            };
            let weight = if item.selected { FontWeight::SEMIBOLD } else { FontWeight::NORMAL };
            (selector.clone(), selector, color, weight)
        } else {
            (
                format!("model-{}-{}", ctx.key, item.value),
                format!("model-row-{}-{}", ctx.key, item.value),
                theme::TEXT,
                FontWeight::NORMAL,
            )
        };
        let value = item.value.clone();
        let field = ctx.field.clone();
        let store = ctx.store.clone();
        let branch = ctx.branch;
        let mut row = div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.clone())
            .w_full()
            .h(px(ROW_HEIGHT))
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .flex()
            .items_center()
            .when(ctx.branch, |r| r.gap_1().group("branch-row"))
            .when(!ctx.branch, |r| r.gap_2())
            .when(item.selected, |r| r.bg(theme::HOVER))
            .hover(|r| r.bg(theme::HOVER))
            .on_click(cx.listener(move |this, _, window, cx| {
                if branch {
                    store.update(cx, |s, cx| {
                        s.run_git(GitAction::Checkout(value.clone()), cx)
                    });
                    // picking a branch closes the dropdown and resets the
                    // filter, so the next open lists everything
                    this.filter.update(cx, |f, cx| f.set_value("", window, cx));
                    this.dismiss(cx);
                } else if let Some(field) = field.clone() {
                    field.update(cx, |state, cx| state.set_value(value.as_str(), window, cx));
                    this.open = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .size(px(theme::icon_small()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(item.selected, |marker| {
                        marker.child(icon(
                            "icons/icon-check.svg",
                            px(theme::icon_small()),
                            theme::ACCENT,
                        ))
                    }),
            )
            .child(
                div()
                    .truncate()
                    .text_size(px(theme::font_small()))
                    .text_color(color)
                    .font_weight(weight)
                    .child(item.label.clone()),
            );
        if item.deletable {
            let armed = ctx.armed.as_deref() == Some(item.value.as_str());
            let name = item.value.clone();
            let tooltip = if armed { "Click again to delete" } else { "Delete branch" };
            let value = item.value.clone();
            row = row.child(with_tooltip(
                div()
                    .id(SharedString::from(format!("branch-delete-{value}")))
                    .debug_selector(move || format!("branch-delete-{value}"))
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(18.))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .opacity(0.)
                    .when(armed, |d| d.opacity(1.))
                    .group_hover("branch-row", |s| s.opacity(1.))
                    .hover(|s| s.bg(theme::tint(theme::DANGER)))
                    .on_click({
                        let name = name.clone();
                        cx.listener(move |this, _, _, cx| {
                            // never the row's checkout
                            cx.stop_propagation();
                            if this.branch_delete_armed.as_deref() == Some(name.as_str()) {
                                this.branch_delete_armed = None;
                                this.store.update(cx, |s, cx| {
                                    s.run_git(GitAction::DeleteBranch(name.clone()), cx)
                                });
                                this.dismiss(cx);
                            } else {
                                this.branch_delete_armed = Some(name.clone());
                            }
                            cx.notify();
                        })
                    })
                    .child(icon(
                        "icons/icon-trash.svg",
                        px(theme::icon_small() - 2.),
                        if armed { theme::DANGER } else { theme::TEXT_MUTED },
                    )),
                ctx.show_tooltips,
                tooltip,
            ));
        }
        row.into_any_element()
    }

    fn branch_trigger(&self, label: &str, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("branch-button")
            .debug_selector(|| "branch-button".into())
            .w_full()
            .cursor_pointer()
            .rounded_lg()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::INPUT)
            .pl_2()
            .pr_1()
            .py_1()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .hover(|s| s.border_color(theme::ACCENT))
            // the window's outside-click dismissal must not see the toggle
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                this.branch_delete_armed = None;
                this.new_branch_open = false;
                this.open = !this.open;
                if this.open {
                    this.filter.update(cx, |f, cx| {
                        f.set_value("", window, cx);
                        f.focus(window, cx);
                    });
                }
                cx.notify();
            }))
            .child(icon(
                "icons/icon-source-branch.svg",
                px(theme::icon_small()),
                theme::TEXT_MUTED,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::TEXT_DIM)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .rounded_md()
                    .bg(theme::CARD)
                    .p_1()
                    .flex()
                    .child(icon(
                        "icons/icon-chevron-down.svg",
                        px(theme::icon_small() - 2.),
                        theme::TEXT_MUTED,
                    )),
            )
    }

    fn model_trigger(
        &self,
        key: &'static str,
        label: &str,
        provider: &str,
        has_selection: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(format!("model-trigger-{key}")))
            .debug_selector(move || format!("model-trigger-{key}"))
            .flex_1()
            .min_w_0()
            .cursor_pointer()
            .rounded_md()
            .border_1()
            .border_color(if self.open { theme::ACCENT } else { theme::HAIRLINE })
            .bg(theme::INPUT)
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .hover(|s| s.border_color(theme::TEXT_MUTED))
            .on_click(cx.listener(|this, _, window, cx| {
                this.open = !this.open;
                if this.open {
                    let filter = this.filter.clone();
                    filter.update(cx, |state, cx| {
                        state.set_value("", window, cx);
                        state.focus(window, cx);
                    });
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(!provider.is_empty(), |row| {
                        row.child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(theme::CHIP)
                                .text_size(px(theme::font_small() - 1.))
                                .text_color(theme::TEXT_MUTED)
                                .child(provider.to_string()),
                        )
                    })
                    .child(
                        div()
                            .truncate()
                            .text_size(px(theme::font_small()))
                            .text_color(if has_selection { theme::TEXT } else { theme::TEXT_MUTED })
                            .child(label.to_string()),
                    ),
            )
            .child(icon(
                "icons/icon-chevron-down.svg",
                px(theme::icon_small()),
                theme::TEXT_MUTED,
            ))
    }

    fn refresh_button(&self) -> gpui::Stateful<gpui::Div> {
        let store = self.store.clone();
        div()
            .id(SharedString::from(format!("picker-refresh-{}", self.key)))
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .text_size(px(theme::font_small()))
            .text_color(theme::TEXT_MUTED)
            .border_1()
            .border_color(theme::HAIRLINE)
            .hover(|s| s.bg(theme::HOVER))
            .on_click(move |_, _, cx| {
                store.update(cx, |s, cx| s.load_models(cx));
            })
            .child(icon(
                "icons/icon-refresh.svg",
                px(theme::icon_small()),
                theme::TEXT_MUTED,
            ))
    }

    fn new_branch_footer(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("new-branch-row")
            .debug_selector(|| "new-branch-row".into())
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .py_1()
            .text_size(px(theme::font_small()))
            .text_color(theme::ACCENT)
            .hover(|s| s.bg(theme::HOVER))
            .on_click(cx.listener(|this, _, window, cx| {
                this.branch_delete_armed = None;
                // the form always opens seeded with the repo's prefix and
                // focused, ready to type the branch name
                if let Some(name) = this.new_branch_name.clone() {
                    let prefix = this.store.read(cx).settings().repos.branch_name_prefix.clone();
                    let seed = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
                    name.update(cx, |state, cx| {
                        state.set_value(seed.as_str(), window, cx);
                        state.focus(window, cx);
                    });
                }
                this.new_branch_open = true;
                cx.notify();
            }))
            .child("+ New branch…")
    }

    fn new_branch_actions(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let name_state = self.new_branch_name.clone();
        div()
            .flex()
            .flex_row()
            .gap_2()
            .child(
                div()
                    .id("new-branch-cancel")
                    .debug_selector(|| "new-branch-cancel".into())
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_DIM)
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.new_branch_open = false;
                        cx.notify();
                    }))
                    .child("Cancel"),
            )
            .child(
                div()
                    .id("new-branch-create")
                    .debug_selector(|| "new-branch-create".into())
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::font_small()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .bg(theme::ACCENT)
                    .text_color(theme::ON_ACCENT)
                    .hover(|s| s.opacity(0.9))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let Some(name_state) = name_state.clone() else { return };
                        let name = name_state.read(cx).value().trim().to_string();
                        if name.is_empty() {
                            return;
                        }
                        this.store.update(cx, |s, cx| {
                            s.run_git(
                                GitAction::CreateBranch { name, start: None, checkout: true },
                                cx,
                            )
                        });
                        // a created branch is a selection: the popup closes
                        // and the form resets
                        this.dismiss(cx);
                        name_state.update(cx, |f, cx| f.set_value("", window, cx));
                        cx.notify();
                    }))
                    .child("Create"),
            )
            .into_any_element()
    }
}

impl Render for FilteredPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let branch_mode = self.is_branch();
        if branch_mode {
            self.sync_repo_reset(window, cx);
        }
        let key = self.key;
        let (selected, selected_label, selected_provider) = self.selected_info(cx);

        let trigger_row = if branch_mode {
            div().child(self.branch_trigger(&selected_label, cx))
        } else {
            div()
                .flex()
                .flex_row()
                .gap_1()
                .child(self.model_trigger(
                    key,
                    &selected_label,
                    &selected_provider,
                    !selected.is_empty(),
                    cx,
                ))
                .child(self.refresh_button())
        };

        // models mode: the popup is an in-flow child below the trigger. The
        // branch popup is NOT rendered here — ChangesView appends it as the
        // rail's LAST child (branch_popup) so it paints over the rail's
        // actions without gpui::deferred, whose hitboxes misplace nested
        // interactive elements (the row trash) in real input.
        let popup = if !branch_mode && self.open {
            let list = self.build_list(key, cx);
            self.build_panel(key, list, cx).into_any_element()
        } else {
            div().into_any_element()
        };

        div()
            .id(SharedString::from(if branch_mode {
                format!("branch-picker-{key}")
            } else {
                format!("model-picker-{key}")
            }))
            .when(!branch_mode, |p| p.relative())
            .flex()
            .flex_col()
            .gap_1()
            .when(branch_mode, |p| p.w_full())
            .when(self.open && !branch_mode, |picker| {
                picker.on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }))
            })
            .child(trigger_row)
            .when(self.open && !branch_mode, |picker| picker.child(popup))
    }
}

impl FilteredPicker {
    fn selected_info(&self, cx: &App) -> (String, String, String) {
        match &self.source {
            PickerSource::Models { field, default_label } => {
                let selected = field.read(cx).value().trim().to_string();
                let label = if selected.is_empty() {
                    default_label.to_string()
                } else {
                    model_name(&selected).to_string()
                };
                let provider = model_provider(&selected).to_string();
                (selected, label, provider)
            }
            PickerSource::Branches => {
                let selected = self
                    .store
                    .read(cx)
                    .selected_entry()
                    .and_then(|e| e.git_branch_name.clone())
                    .unwrap_or_default();
                let label = if selected.is_empty() { "—".to_string() } else { selected.clone() };
                (selected, label, String::new())
            }
        }
    }

    fn build_list(&mut self, key: &'static str, cx: &mut Context<Self>) -> gpui::AnyElement {
        let branch_mode = self.is_branch();
        let rows = self.rows(cx);
        let row_ctx = RowCtx {
            branch: branch_mode,
            key,
            field: match &self.source {
                PickerSource::Models { field, .. } => Some(field.clone()),
                PickerSource::Branches => None,
            },
            store: self.store.clone(),
            show_tooltips: self.store.read(cx).settings().general.show_tooltips,
            armed: self.branch_delete_armed.clone(),
        };
        // the models list is virtualized; the branch list renders as a plain
        // scrollable stack capped in rows() — the old dropdown's behavior
        if branch_mode {
            let mut stack = div()
                .id("branch-list")
                .w_full()
                .max_h(px(BRANCH_VISIBLE_ROWS as f32 * ROW_HEIGHT))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_0p5();
            for row in &rows {
                stack = stack.child(match row {
                    PickerRow::Group { id, label, selector } => {
                        Self::group_row(id, label, *selector)
                    }
                    PickerRow::Item(item) => Self::item_row(&row_ctx, item, cx),
                    PickerRow::Message(message) => Self::message_row(&row_ctx, message),
                });
            }
            stack.into_any_element()
        } else {
            let list_height = rows.len().min(MODEL_VISIBLE_ROWS) as f32 * ROW_HEIGHT;
            uniform_list(
                SharedString::from(format!("model-list-{key}")),
                rows.len(),
                cx.processor(move |_this, range: std::ops::Range<usize>, _window, cx| {
                    range
                        .map(|ix| match &rows[ix] {
                            PickerRow::Group { id, label, selector } => {
                                Self::group_row(id, label, *selector)
                            }
                            PickerRow::Item(item) => Self::item_row(&row_ctx, item, cx),
                            PickerRow::Message(message) => Self::message_row(&row_ctx, message),
                        })
                        .collect()
                }),
            )
            .w_full()
            .h(px(list_height))
            .into_any_element()
        }
    }

    fn build_panel(
        &mut self,
        key: &'static str,
        list: gpui::AnyElement,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let branch_mode = self.is_branch();
        let mut panel = if branch_mode {
            div()
                .id("branch-menu")
                .debug_selector(|| "branch-menu".into())
                // presses inside the popup (filter field, scrollbar, rows)
                // must not reach the window's outside-click dismissal
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .occlude()
        } else {
            div()
                .id(SharedString::from(format!("picker-list-{key}")))
                .debug_selector(move || format!("model-panel-{key}"))
        };
        panel = panel
            .when(branch_mode, |p| p.rounded_lg().shadow_lg().p_2().gap_2())
            .when(!branch_mode, |p| p.rounded_md().shadow_md().p_1().gap_1())
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .flex()
            .flex_col();
        let new_branch_form = branch_mode && self.new_branch_open;
        if new_branch_form {
            let name = self.new_branch_name.clone().expect("branch mode carries the form field");
            panel
                .child(
                    div()
                        .debug_selector(|| "new-branch-name".into())
                        .child(Input::new(&name).small()),
                )
                .child(self.new_branch_actions(cx))
        } else {
            let mut content = if branch_mode {
                panel.child(
                    div()
                        .debug_selector(|| "branch-filter".into())
                        .child(Input::new(&self.filter).small()),
                )
            } else {
                panel.child(
                    div()
                        .debug_selector(move || format!("model-filter-{key}"))
                        .child(Input::new(&self.filter).small()),
                )
            };
            content = content.child(list);
            if branch_mode {
                content = content.child(self.new_branch_footer(cx));
            }
            content
        }
    }

    /// The branch popup as an overlay element for the host rail: ChangesView
    /// appends it AFTER the git actions so later-sibling paint order puts it
    /// on top (no deferred) and every hitbox registers in the normal pass.
    pub fn branch_popup(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.is_branch() || !self.open {
            return None;
        }
        let key = self.key;
        let list = self.build_list(key, cx);
        let panel = self.build_panel(key, list, cx);
        // anchored to the rail (the nearest positioned ancestor): covers the
        // trigger + actions, the old dropdown's geometry, without reflowing
        Some(
            panel
                .absolute()
                .top(px(-4.))
                .left(px(8.))
                .right(px(8.))
                .into_any_element(),
        )
    }
}
