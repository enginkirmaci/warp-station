//! The filtered picker: a select-style trigger over a filtered, virtualized
//! list of the opencode catalog. Standalone so the two settings slots share
//! one implementation and filter typing repaints only the picker.

use crate::store::Store;
use crate::theme;
use gpui::prelude::*;
use gpui::{div, px, uniform_list, App, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::Sizable as _;

const ROW_HEIGHT: f32 = 28.;
const VISIBLE_ROWS: usize = 10;

enum PickerRow {
    Default,
    Provider(String),
    Model(String),
    Message(String),
}

fn model_provider(model: &str) -> &str {
    model.split_once('/').map(|(provider, _)| provider).unwrap_or("")
}

fn model_name(model: &str) -> &str {
    model.split_once('/').map(|(_, name)| name).unwrap_or(model)
}

pub struct FilteredPicker {
    field: Entity<InputState>,
    filter: Entity<InputState>,
    store: Entity<Store>,
    key: &'static str,
    default_label: &'static str,
    open: bool,
    _subscriptions: Vec<gpui::Subscription>,
}

impl FilteredPicker {
    pub fn new(
        field: Entity<InputState>,
        store: Entity<Store>,
        key: &'static str,
        default_label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter models"));
        let sub_filter = cx.observe(&filter, |_, _, cx| cx.notify());
        let sub_field = cx.observe(&field, |_, _, cx| cx.notify());
        let sub_store = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            field,
            filter,
            store,
            key,
            default_label,
            open: false,
            _subscriptions: vec![sub_filter, sub_field, sub_store],
        }
    }

    fn rows(&self, cx: &App) -> Vec<PickerRow> {
        let filter = self.filter.read(cx).value().trim().to_lowercase();
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
        rows.push(PickerRow::Default);
        if models.is_empty() {
            rows.push(PickerRow::Message(
                "No models found. Start warp-agent, then refresh.".to_string(),
            ));
        } else {
            let mut last_provider = String::new();
            for model in models {
                let provider = model_provider(&model).to_string();
                if !provider.is_empty() && provider != last_provider {
                    rows.push(PickerRow::Provider(provider.to_uppercase()));
                    last_provider = provider;
                }
                rows.push(PickerRow::Model(model));
            }
        }
        rows
    }

    fn provider_row(id: SharedString, label: String) -> gpui::AnyElement {
        div()
            .id(id)
            .w_full()
            .h(px(ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .text_size(px(theme::font_small() - 1.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::TEXT_FAINT)
            .child(label)
            .into_any_element()
    }

    fn message_row(id: SharedString, label: String) -> gpui::AnyElement {
        div()
            .id(id)
            .w_full()
            .h(px(ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .text_size(px(theme::font_small()))
            .text_color(theme::TEXT_FAINT)
            .child(label)
            .into_any_element()
    }

    fn model_row(
        id: SharedString,
        key: &'static str,
        label: String,
        selected: bool,
        value: String,
        field: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let selector = format!("model-row-{key}-{value}");
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .w_full()
            .h(px(ROW_HEIGHT))
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .when(selected, |row| row.bg(theme::HOVER))
            .hover(|s| s.bg(theme::HOVER))
            .on_click(cx.listener(move |this, _, window, cx| {
                field.update(cx, |state, cx| state.set_value(value.as_str(), window, cx));
                this.open = false;
                cx.notify();
            }))
            .child(
                div()
                    .size(px(theme::icon_small()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(selected, |marker| {
                        marker.child(crate::ui::icon(
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
                    .text_color(theme::TEXT)
                    .child(label),
            )
            .into_any_element()
    }
}

impl Render for FilteredPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let key = self.key;
        let selected = self.field.read(cx).value().trim().to_string();
        let selected_label = if selected.is_empty() {
            self.default_label.to_string()
        } else {
            model_name(&selected).to_string()
        };
        let selected_provider = model_provider(&selected).to_string();
        let rows = self.rows(cx);
        let list_height = rows.len().min(VISIBLE_ROWS) as f32 * ROW_HEIGHT;
        let field = self.field.clone();
        let rows_selected = selected.clone();
        let default_label = self.default_label;
        let list = uniform_list(
            SharedString::from(format!("model-list-{key}")),
            rows.len(),
            cx.processor(move |_this, range: std::ops::Range<usize>, _window, cx| {
                range
                    .map(|ix| match &rows[ix] {
                        PickerRow::Default => Self::model_row(
                            SharedString::from(format!("model-default-{key}")),
                            key,
                            default_label.to_string(),
                            rows_selected.is_empty(),
                            String::new(),
                            field.clone(),
                            cx,
                        ),
                        PickerRow::Provider(provider) => Self::provider_row(
                            SharedString::from(format!("model-provider-{key}-{provider}")),
                            provider.clone(),
                        ),
                        PickerRow::Model(model) => Self::model_row(
                            SharedString::from(format!("model-{key}-{model}")),
                            key,
                            model_name(model).to_string(),
                            model.eq_ignore_ascii_case(&rows_selected),
                            model.clone(),
                            field.clone(),
                            cx,
                        ),
                        PickerRow::Message(message) => Self::message_row(
                            SharedString::from(format!("model-message-{key}")),
                            message.clone(),
                        ),
                    })
                    .collect()
            }),
        )
        .w_full()
        .h(px(list_height));

        div()
            .id(SharedString::from(format!("model-picker-{key}")))
            .relative()
            .flex()
            .flex_col()
            .gap_1()
            .when(self.open, |picker| {
                picker.on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(
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
                                    .when(!selected_provider.is_empty(), |row| {
                                        row.child(
                                            div()
                                                .px_1p5()
                                                .py_0p5()
                                                .rounded_sm()
                                                .bg(theme::CHIP)
                                                .text_size(px(theme::font_small() - 1.))
                                                .text_color(theme::TEXT_MUTED)
                                                .child(selected_provider.clone()),
                                        )
                                    })
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(theme::font_small()))
                                            .text_color(if selected.is_empty() {
                                                theme::TEXT_MUTED
                                            } else {
                                                theme::TEXT
                                            })
                                            .child(selected_label.clone()),
                                    ),
                            )
                            .child(crate::ui::icon(
                                "icons/icon-chevron-down.svg",
                                px(theme::icon_small()),
                                theme::TEXT_MUTED,
                            )),
                    )
                    .child({
                        let store = self.store.clone();
                        div()
                            .id(SharedString::from(format!("picker-refresh-{key}")))
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
                            .child(crate::ui::icon(
                                "icons/icon-refresh.svg",
                                px(theme::icon_small()),
                                theme::TEXT_MUTED,
                            ))
                    }),
            )
            .when(self.open, |picker| {
                picker.child(
                    div()
                        .id(SharedString::from(format!("picker-list-{key}")))
                        .rounded_md()
                        .border_1()
                        .border_color(theme::HAIRLINE)
                        .bg(theme::POPUP)
                        .p_1()
                        .shadow_md()
                        .debug_selector(move || format!("model-panel-{key}"))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .debug_selector(move || format!("model-filter-{key}"))
                                .child(Input::new(&self.filter).small()),
                        )
                        .child(list),
                )
            })
    }
}
