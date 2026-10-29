//! The filtered picker: a shared select-style trigger over a filtered list.
//! The Settings model slots and the Changes tab's branch selector use the same
//! trigger, panel and row rendering; only their source data and actions differ.

use crate::net::GitAction;
use crate::store::Store;
use crate::theme;
use crate::ui::{icon, trim_branch_label, with_tooltip, A11y, BRANCH_LABEL_CHARS};
use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FontWeight, Hsla, MouseButton, SharedString, TextRun, Window, div, px,
    uniform_list,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};

const ROW_HEIGHT: f32 = 28.;

/// The look and layout choices a picker source contributes to the shared shell.
#[derive(Clone, Copy)]
struct PickerSpec {
    filter_placeholder: &'static str,
    trigger_icon: Option<&'static str>,
    provider_badge: bool,
    refresh: bool,
    stop_trigger_mousedown: bool,
    visible_rows: usize,
    selected_accent: bool,
    new_branch: bool,
    overlay: bool,
}

const MODEL_PICKER: PickerSpec = PickerSpec {
    filter_placeholder: "Filter models",
    trigger_icon: None,
    provider_badge: true,
    refresh: true,
    stop_trigger_mousedown: false,
    visible_rows: 10,
    selected_accent: false,
    new_branch: false,
    overlay: false,
};

const BRANCH_PICKER: PickerSpec = PickerSpec {
    filter_placeholder: "Filter branches",
    trigger_icon: Some("icons/icon-source-branch.svg"),
    provider_badge: false,
    refresh: false,
    stop_trigger_mousedown: true,
    visible_rows: 8,
    selected_accent: true,
    new_branch: true,
    overlay: true,
};

/// The ephemeral state a branch picker carries; model pickers have none.
struct BranchState {
    new_branch_name: Entity<InputState>,
    delete_armed: Option<String>,
    new_branch_open: bool,
    /// The repo the ephemeral branch state belongs to; a mismatch resets it.
    loaded_for: Option<String>,
}

/// What the picker lists. The Settings model slots write the pick into a text
/// field; the branch selector checks the picked branch out.
enum PickerSource {
    Models {
        field: Entity<InputState>,
        default_label: &'static str,
    },
    Branches(BranchState),
}

struct PickerSelection {
    value: String,
    label: String,
    badge: Option<String>,
}

struct PickerItem {
    /// Element identity.
    id: String,
    /// Debug selector used by the headless probes.
    selector: String,
    /// Model id or branch name: the pick payload.
    value: String,
    label: String,
    selected: bool,
    /// Branch mode: a remote-tracking row (faint).
    muted: bool,
    /// Branch mode: the armed trash action.
    trailing: Option<RowAction>,
}

enum RowAction {
    DeleteBranch { armed: bool },
}

enum PickerRow {
    Group {
        id: String,
        label: String,
        selector: Option<String>,
    },
    Item(PickerItem),
    Message {
        id: String,
        label: String,
    },
}

fn model_provider(model: &str) -> &str {
    model
        .split_once('/')
        .map(|(provider, _)| provider)
        .unwrap_or("")
}

fn model_name(model: &str) -> &str {
    model.split_once('/').map(|(_, name)| name).unwrap_or(model)
}

/// Width the rail leaves under the trigger's label; the pill's tighter
/// budget exists to cap the table column and does not apply here.
const LABEL_MAX_PX: f32 = 190.;

/// The picker's branch label: the full name while its shaped width fits the
/// rail, the head-trimmed pill label once it would overflow.
pub fn branch_label(branch: &str, window: &Window) -> String {
    let run = TextRun {
        len: branch.len(),
        font: gpui::font(theme::ui_font()),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let width = window
        .text_system()
        .layout_line(branch, px(theme::font_small()), &[run], None)
        .width;
    if width <= px(LABEL_MAX_PX) {
        branch.to_string()
    } else {
        trim_branch_label(branch, BRANCH_LABEL_CHARS)
    }
}

pub struct FilteredPicker {
    spec: PickerSpec,
    source: PickerSource,
    filter: Entity<InputState>,
    store: Entity<Store>,
    key: &'static str,
    open: bool,
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
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(MODEL_PICKER.filter_placeholder));
        Self::build(
            PickerSource::Models {
                field,
                default_label,
            },
            MODEL_PICKER,
            store,
            key,
            filter,
            cx,
        )
    }

    /// The Changes-tab branch selector: a pick checks the branch out; locals
    /// carry the delete trash and the popup carries the new-branch form.
    pub fn branches(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(BRANCH_PICKER.filter_placeholder));
        let new_branch_name = cx.new(|cx| InputState::new(window, cx).placeholder("branch name"));
        Self::build(
            PickerSource::Branches(BranchState {
                new_branch_name,
                delete_armed: None,
                new_branch_open: false,
                loaded_for: None,
            }),
            BRANCH_PICKER,
            store,
            "Branch",
            filter,
            cx,
        )
    }

    fn build(
        source: PickerSource,
        spec: PickerSpec,
        store: Entity<Store>,
        key: &'static str,
        filter: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sub_filter = cx.observe(&filter, |_, _, cx| cx.notify());
        let sub_store = cx.observe(&store, |_, _, cx| cx.notify());
        let mut subscriptions = vec![sub_filter, sub_store];
        if let PickerSource::Models { field, .. } = &source {
            subscriptions.push(cx.observe(field, |_, _, cx| cx.notify()));
        }
        Self {
            spec,
            source,
            filter,
            store,
            key,
            open: false,
            _subscriptions: subscriptions,
        }
    }

    fn new_branch_field(&self) -> Option<Entity<InputState>> {
        match &self.source {
            PickerSource::Branches(state) => Some(state.new_branch_name.clone()),
            PickerSource::Models { .. } => None,
        }
    }

    fn clear_ephemeral(&mut self) {
        if let PickerSource::Branches(state) = &mut self.source {
            state.delete_armed = None;
            state.new_branch_open = false;
        }
    }

    /// Closes the popup and clears the ephemeral branch state. The branch
    /// mode is wired to the window's outside-click and Escape dismissal
    /// through this. Returns whether it was actually open.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        self.clear_ephemeral();
        cx.notify();
        true
    }

    fn toggle_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_ephemeral();
        self.open = !self.open;
        if self.open {
            let filter = self.filter.clone();
            filter.update(cx, |state, cx| {
                state.set_value("", window, cx);
                state.focus(window, cx);
            });
        }
        cx.notify();
    }

    /// One-time reset when the selected repo changed: the popup must never
    /// show the previous repo's branch list, filter or form.
    fn sync_repo_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repo = self.store.read(cx).selected_repo.clone();
        let reset = match &mut self.source {
            PickerSource::Branches(state) => {
                if state.loaded_for == repo {
                    false
                } else {
                    state.loaded_for = repo;
                    state.delete_armed = None;
                    state.new_branch_open = false;
                    true
                }
            }
            PickerSource::Models { .. } => false,
        };
        if reset {
            self.open = false;
            let filter = self.filter.clone();
            filter.update(cx, |f, cx| f.set_value("", window, cx));
        }
    }

    fn selected_info(&self, window: &Window, cx: &App) -> PickerSelection {
        match &self.source {
            PickerSource::Models {
                field,
                default_label,
            } => {
                let selected = field.read(cx).value().trim().to_string();
                let label = if selected.is_empty() {
                    default_label.to_string()
                } else {
                    model_name(&selected).to_string()
                };
                let provider = model_provider(&selected);
                let badge = (!provider.is_empty()).then(|| provider.to_string());
                PickerSelection {
                    value: selected,
                    label,
                    badge,
                }
            }
            PickerSource::Branches(_) => {
                let selected = self
                    .store
                    .read(cx)
                    .selected_entry()
                    .and_then(|e| e.git_branch_name.clone())
                    .unwrap_or_default();
                let label = if selected.is_empty() {
                    "—".to_string()
                } else {
                    branch_label(&selected, window)
                };
                PickerSelection {
                    value: selected,
                    label,
                    badge: None,
                }
            }
        }
    }

    fn rows(&self, window: &Window, cx: &App) -> Vec<PickerRow> {
        let filter = self.filter.read(cx).value().trim().to_lowercase();
        match &self.source {
            PickerSource::Models {
                field,
                default_label,
            } => {
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
                    id: format!("model-{}-", self.key),
                    selector: format!("model-row-{}-", self.key),
                    value: String::new(),
                    label: default_label.to_string(),
                    selected: selected.is_empty(),
                    muted: false,
                    trailing: None,
                }));
                if models.is_empty() {
                    rows.push(PickerRow::Message {
                        id: format!("model-message-{}", self.key),
                        label: "No models found. Start warp-agent, then refresh.".to_string(),
                    });
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
                            id: format!("model-{}-{}", self.key, model),
                            selector: format!("model-row-{}-{}", self.key, model),
                            value: model.clone(),
                            label: model_name(&model).to_string(),
                            selected: model.eq_ignore_ascii_case(&selected),
                            muted: false,
                            trailing: None,
                        }));
                    }
                }
                rows
            }
            PickerSource::Branches(state) => {
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
                            selector: Some("branch-group-remote".into()),
                        });
                    }
                    let deletable = !is_remote && branch.name != current;
                    rows.push(PickerRow::Item(PickerItem {
                        id: format!("branch-row-{}", branch.name),
                        selector: format!("branch-row-{}", branch.name),
                        value: branch.name.clone(),
                        // the full name stays the pick payload; the display
                        // label sheds its head only when it would overflow
                        label: branch_label(&branch.name, window),
                        selected: branch.name == current,
                        muted: is_remote,
                        trailing: deletable.then(|| RowAction::DeleteBranch {
                            armed: state.delete_armed.as_deref() == Some(branch.name.as_str()),
                        }),
                    }));
                }
                rows
            }
        }
    }

    fn root_id(&self) -> String {
        if self.spec.overlay {
            format!("branch-picker-{}", self.key)
        } else {
            format!("model-picker-{}", self.key)
        }
    }

    fn trigger_id(&self) -> String {
        if self.spec.overlay {
            "branch-button".to_string()
        } else {
            format!("model-trigger-{}", self.key)
        }
    }

    fn panel_id(&self) -> String {
        if self.spec.overlay {
            "branch-menu".to_string()
        } else {
            format!("picker-list-{}", self.key)
        }
    }

    fn panel_selector(&self) -> String {
        if self.spec.overlay {
            "branch-menu".to_string()
        } else {
            format!("model-panel-{}", self.key)
        }
    }

    fn filter_selector(&self) -> String {
        if self.spec.overlay {
            "branch-filter".to_string()
        } else {
            format!("model-filter-{}", self.key)
        }
    }

    fn list_id(&self) -> String {
        if self.spec.overlay {
            "branch-list".to_string()
        } else {
            format!("model-list-{}", self.key)
        }
    }

    fn group_row(id: &str, label: &str, selector: Option<&str>) -> gpui::AnyElement {
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
            let selector = selector.to_string();
            row = row.debug_selector(move || selector.clone());
        }
        row.child(label.to_string()).into_any_element()
    }

    fn message_row(id: &str, label: &str) -> gpui::AnyElement {
        div()
            .id(SharedString::from(id.to_string()))
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

    fn item_row(
        item: &PickerItem,
        selected_accent: bool,
        show_tooltips: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let color = if item.selected && selected_accent {
            theme::ACCENT
        } else if item.muted {
            theme::TEXT_FAINT
        } else if selected_accent {
            theme::TEXT_DIM
        } else {
            theme::TEXT
        };
        let weight = if item.selected && selected_accent {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };
        let value = item.value.clone();
        let id = item.id.clone();
        let selector = item.selector.clone();
        let mut row = div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.clone())
            .a11y_option(item.label.clone(), item.selected)
            .w_full()
            .h(px(ROW_HEIGHT))
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .flex()
            .items_center()
            .when(selected_accent, |r| r.gap_1().group("branch-row"))
            .when(!selected_accent, |r| r.gap_2())
            .when(item.selected, |r| r.bg(theme::HOVER))
            .hover(|r| r.bg(theme::HOVER))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick(value.clone(), window, cx);
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
                // fills the row so a trailing action pins to the right edge —
                // hugging the label puts the invisible trash mid-row, under
                // the click meant for the branch name
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(theme::font_small()))
                    .text_color(color)
                    .font_weight(weight)
                    .child(item.label.clone()),
            );
        if let Some(RowAction::DeleteBranch { armed }) = &item.trailing {
            let armed = *armed;
            let name = item.value.clone();
            let tooltip = if armed {
                "Click again to delete"
            } else {
                "Delete branch"
            };
            let value = item.value.clone();
            let selector = format!("branch-delete-{value}");
            let id = selector.clone();
            row = row.child(with_tooltip(
                div()
                    .id(SharedString::from(id))
                    .debug_selector(move || selector.clone())
                    .a11y_button(if armed {
                        format!("Confirm delete branch {name}")
                    } else {
                        format!("Delete branch {name}")
                    })
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
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // never the row's checkout
                        cx.stop_propagation();
                        this.activate_trailing(name.clone(), cx);
                    }))
                    .child(icon(
                        "icons/icon-trash.svg",
                        px(theme::icon_small() - 2.),
                        if armed {
                            theme::DANGER
                        } else {
                            theme::TEXT_MUTED
                        },
                    )),
                show_tooltips,
                tooltip,
            ));
        }
        row.into_any_element()
    }

    fn pick(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        if let PickerSource::Models { field, .. } = &self.source {
            let field = field.clone();
            field.update(cx, |state, cx| state.set_value(value.as_str(), window, cx));
            self.open = false;
            cx.notify();
        } else {
            self.store
                .update(cx, |s, cx| s.run_git(GitAction::Checkout(value), cx));
            // picking a branch closes the dropdown and resets the filter, so
            // the next open lists everything
            let filter = self.filter.clone();
            filter.update(cx, |f, cx| f.set_value("", window, cx));
            self.dismiss(cx);
        }
    }

    fn activate_trailing(&mut self, name: String, cx: &mut Context<Self>) {
        let confirmed = match &mut self.source {
            PickerSource::Branches(state) => {
                if state.delete_armed.as_deref() == Some(name.as_str()) {
                    state.delete_armed = None;
                    true
                } else {
                    state.delete_armed = Some(name.clone());
                    false
                }
            }
            PickerSource::Models { .. } => return,
        };
        if confirmed {
            let store = self.store.clone();
            store.update(cx, |s, cx| s.run_git(GitAction::DeleteBranch(name), cx));
            self.dismiss(cx);
        } else {
            cx.notify();
        }
    }

    fn build_trigger(
        &self,
        selection: &PickerSelection,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let trigger_id = self.trigger_id();
        let selector = trigger_id.clone();
        let mut trigger = div()
            .id(SharedString::from(trigger_id))
            .debug_selector(move || selector.clone())
            .role(gpui::Role::ComboBox)
            .aria_label(self.key)
            .aria_expanded(self.open)
            .aria_value(selection.label.clone())
            .flex_1()
            .min_w_0()
            .cursor_pointer()
            .rounded_md()
            .border_1()
            .border_color(if self.open {
                theme::ACCENT
            } else {
                theme::HAIRLINE
            })
            .bg(theme::INPUT)
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .hover(|s| s.border_color(theme::TEXT_MUTED));
        if self.spec.stop_trigger_mousedown {
            trigger = trigger.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        }
        trigger = trigger
            .on_click(cx.listener(|this, _, window, cx| this.toggle_open(window, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(self.spec.trigger_icon, |row, icon_path| {
                        row.child(icon(icon_path, px(theme::icon_small()), theme::TEXT_MUTED))
                    })
                    .when(self.spec.provider_badge, |row| match &selection.badge {
                        Some(badge) if !badge.is_empty() => row.child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(theme::CHIP)
                                .text_size(px(theme::font_small() - 1.))
                                .text_color(theme::TEXT_MUTED)
                                .child(badge.clone()),
                        ),
                        _ => row,
                    })
                    .child(
                        div()
                            .truncate()
                            .text_size(px(theme::font_small()))
                            .text_color(if selection.value.is_empty() {
                                theme::TEXT_MUTED
                            } else {
                                theme::TEXT
                            })
                            .child(selection.label.clone()),
                    ),
            )
            .child(icon(
                "icons/icon-chevron-down.svg",
                px(theme::icon_small()),
                theme::TEXT_MUTED,
            ));
        trigger
    }

    fn refresh_button(&self) -> gpui::Stateful<gpui::Div> {
        let store = self.store.clone();
        div()
            .id(SharedString::from(format!("picker-refresh-{}", self.key)))
            .a11y_button("Refresh models")
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

    fn build_trigger_row(
        &self,
        selection: &PickerSelection,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(self.build_trigger(selection, cx))
            .when(self.spec.refresh, |row| row.child(self.refresh_button()))
            .into_any_element()
    }

    fn build_list(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let rows = self.rows(window, cx);
        let selected_accent = self.spec.selected_accent;
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        let list_id = self.list_id();
        let list_height = rows.len().min(self.spec.visible_rows) as f32 * ROW_HEIGHT;
        uniform_list(
            SharedString::from(list_id),
            rows.len(),
            cx.processor(move |_this, range: std::ops::Range<usize>, _window, cx| {
                range
                    .map(|ix| match &rows[ix] {
                        PickerRow::Group {
                            id,
                            label,
                            selector,
                        } => Self::group_row(id, label, selector.as_deref()),
                        PickerRow::Item(item) => {
                            Self::item_row(item, selected_accent, show_tooltips, cx)
                        }
                        PickerRow::Message { id, label } => Self::message_row(id, label),
                    })
                    .collect()
            }),
        )
        .w_full()
        .h(px(list_height))
        .into_any_element()
    }

    fn new_branch_footer(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("new-branch-row")
            .debug_selector(|| "new-branch-row".into())
            .a11y_button("New branch…")
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .py_1()
            .text_size(px(theme::font_small()))
            .text_color(theme::ACCENT)
            .hover(|s| s.bg(theme::HOVER))
            .on_click(cx.listener(|this, _, window, cx| {
                // the form always opens seeded with the repo's prefix and
                // focused, ready to type the branch name
                if let Some(name) = this.new_branch_field() {
                    let prefix = this
                        .store
                        .read(cx)
                        .settings()
                        .repos
                        .branch_name_prefix
                        .clone();
                    let seed = if prefix.is_empty() {
                        String::new()
                    } else {
                        format!("{prefix}/")
                    };
                    name.update(cx, |state, cx| {
                        state.set_value(seed.as_str(), window, cx);
                        state.focus(window, cx);
                    });
                }
                if let PickerSource::Branches(state) = &mut this.source {
                    state.delete_armed = None;
                    state.new_branch_open = true;
                }
                cx.notify();
            }))
            .child("+ New branch…")
    }

    fn new_branch_actions(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .flex()
            .flex_row()
            .gap_2()
            .child(
                div()
                    .id("new-branch-cancel")
                    .debug_selector(|| "new-branch-cancel".into())
                    .a11y_button("Cancel new branch")
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
                        if let PickerSource::Branches(state) = &mut this.source {
                            state.new_branch_open = false;
                        }
                        cx.notify();
                    }))
                    .child("Cancel"),
            )
            .child(
                div()
                    .id("new-branch-create")
                    .debug_selector(|| "new-branch-create".into())
                    .a11y_button("Create branch")
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::font_small()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .bg(theme::ACCENT)
                    .text_color(theme::ON_ACCENT)
                    .hover(|s| s.opacity(0.9))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let Some(name_state) = this.new_branch_field() else {
                            return;
                        };
                        let name = name_state.read(cx).value().trim().to_string();
                        if name.is_empty() {
                            return;
                        }
                        this.store.update(cx, |s, cx| {
                            s.run_git(
                                GitAction::CreateBranch {
                                    name,
                                    start: None,
                                    checkout: true,
                                },
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

    fn build_panel(
        &mut self,
        list: gpui::AnyElement,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let panel_id = self.panel_id();
        let panel_selector = self.panel_selector();
        let overlay = self.spec.overlay;
        let mut panel = div()
            .id(SharedString::from(panel_id))
            .debug_selector(move || panel_selector.clone())
            .rounded_md()
            .shadow_md()
            .p_1()
            .gap_1()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .flex()
            .flex_col();
        if overlay {
            panel = panel
                // presses inside the popup (filter field, scrollbar, rows)
                // must not reach the window's outside-click dismissal
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .occlude();
        }
        if matches!(&self.source, PickerSource::Branches(state) if state.new_branch_open) {
            let name = self
                .new_branch_field()
                .expect("branch mode carries the form field");
            panel
                .child(
                    div()
                        .debug_selector(|| "new-branch-name".into())
                        .child(Input::new(&name).small()),
                )
                .child(self.new_branch_actions(cx))
        } else {
            let filter_selector = self.filter_selector();
            let mut content = panel
                .child(
                    div()
                        .debug_selector(move || filter_selector.clone())
                        .child(Input::new(&self.filter).small()),
                )
                .child(list);
            if self.spec.new_branch {
                content = content.child(self.new_branch_footer(cx));
            }
            content
        }
    }

    /// The branch popup as an overlay element for the host rail: ChangesView
    /// appends it AFTER the git actions so later-sibling paint order puts it
    /// on top (no deferred) and every hitbox registers in the normal pass.
    pub fn overlay_popup(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.spec.overlay || !self.open {
            return None;
        }
        let list = self.build_list(window, cx);
        let panel = self.build_panel(list, cx);
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

impl Render for FilteredPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_repo_reset(window, cx);
        let selection = self.selected_info(window, cx);
        let trigger_row = self.build_trigger_row(&selection, cx);

        // models mode: the popup is an in-flow child below the trigger. The
        // branch popup is NOT rendered here — ChangesView appends it as the
        // rail's LAST child (overlay_popup) so it paints over the rail's
        // actions without gpui::deferred, whose hitboxes misplace nested
        // interactive elements (the row trash) in real input.
        let popup = if !self.spec.overlay && self.open {
            let list = self.build_list(window, cx);
            Some(self.build_panel(list, cx))
        } else {
            None
        };

        let root_id = self.root_id();
        div()
            .id(SharedString::from(root_id))
            .when(!self.spec.overlay, |p| p.relative())
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .when(!self.spec.overlay && self.open, |picker| {
                picker.on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.dismiss(cx);
                }))
            })
            .child(trigger_row)
            .children(popup)
    }
}
