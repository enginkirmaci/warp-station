//! The Summary section: the daily summaries already written to the notes store,
//! newest first, and the one verb that writes another. A summary covers
//! yesterday's commits across every tracked repository.

use crate::store::Store;
use crate::theme;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::base::text::TextView;
use std::path::PathBuf;
use warpui_kit_core::shell;

pub struct SummaryPage {
    store: Entity<Store>,
    /// Newest first: the names are dates.
    files: Vec<PathBuf>,
    selected: Option<PathBuf>,
    text: Option<SharedString>,
    /// A run that finishes adds a file, so the list reloads on the falling edge.
    was_busy: bool,
    loaded: bool,
}

impl SummaryPage {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            files: Vec::new(),
            selected: None,
            text: None,
            was_busy: false,
            loaded: false,
        }
    }

    fn reload(&mut self, cx: &Context<Self>) {
        let settings = self.store.read(cx).settings();
        let folder =
            warp_station_core::notes::resolve_store_root(&settings.general.notes_store_path)
                .join("Summaries");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by(|a, b| b.cmp(a));
        self.files = files;
        self.loaded = true;
        let first = self.files.first().cloned();
        let keep = self
            .selected
            .clone()
            .filter(|path| self.files.contains(path));
        self.select(keep.or(first));
    }

    fn select(&mut self, path: Option<PathBuf>) {
        self.text = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(SharedString::from);
        self.selected = path;
    }

    fn row(&self, path: &PathBuf, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let name = day_name(path);
        let target = path.clone();
        shell::list_row(
            SharedString::from(format!("summary-{name}")),
            self.selected.as_ref() == Some(path),
        )
        .a11y_button(name.clone())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select(Some(target.clone()));
            cx.notify();
        }))
        .child(
            div()
                .text_size(px(theme::font_name()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT)
                .child(name),
        )
    }

    fn summarize_button(&self, busy: bool, enabled: bool) -> impl IntoElement + use<> {
        let store = self.store.clone();
        let live = enabled && !busy;
        div()
            .id("summary-button")
            .debug_selector(|| "summary-button".into())
            .a11y_button(if busy {
                "Summarizing…"
            } else {
                "Summarize yesterday"
            })
            .flex_shrink_0()
            .h(px(shell::CONTROL_HEIGHT))
            .px_3()
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(theme::font_control()))
            .font_weight(FontWeight::SEMIBOLD)
            .when(live, |d| {
                d.cursor_pointer()
                    .bg(theme::ACCENT)
                    .text_color(theme::ON_ACCENT)
                    .hover(|s| s.bg(theme::ACCENT_HOVER))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |s, cx| s.run_summary(cx));
                    })
            })
            .when(!live, |d| d.bg(theme::RAISED).text_color(theme::TEXT_MUTED))
            .child(if busy {
                warpui_kit_core::motion::spinner(
                    "summary-spinner",
                    "icons/icon-spinner.svg",
                    px(theme::icon_small()),
                    theme::TEXT_MUTED,
                )
                .into_any_element()
            } else {
                icon(
                    "icons/icon-agent-sparkle.svg",
                    px(theme::icon_small()),
                    if live {
                        theme::ON_ACCENT
                    } else {
                        theme::TEXT_MUTED
                    },
                )
                .into_any_element()
            })
            .child(if busy {
                "Summarizing…"
            } else {
                "Summarize yesterday"
            })
    }
}

/// The file's name is its day.
fn day_name(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default()
}

impl Render for SummaryPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let (busy, available) = {
            let store = self.store.read(cx);
            (store.summary_busy, store.wand_available())
        };
        if !self.loaded || (self.was_busy && !busy) {
            self.reload(cx);
        }
        self.was_busy = busy;

        let files = self.files.clone();
        let mut rows = div()
            .id("summary-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for path in &files {
            let row = self.row(path, cx);
            rows = rows.child(row);
        }
        let list = shell::list_pane(self.store.read(cx).list_width())
            .child(
                div()
                    .flex_shrink_0()
                    .px_4()
                    .pt_3p5()
                    .pb_3()
                    .border_b_1()
                    .border_color(theme::HAIRLINE)
                    .child(
                        div()
                            .text_size(px(theme::font_list_title()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child("Summary"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::TEXT_DIM)
                            .child("Yesterday's work across every tracked repository"),
                    ),
            )
            .child(rows);

        let title = self
            .selected
            .as_deref()
            .map_or_else(|| "No summaries yet".to_string(), day_name);
        let head = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .min_h(px(36.))
            .px(px(shell::DETAIL_PADDING))
            .py_4()
            .border_b_1()
            .border_color(theme::HAIRLINE)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(theme::font_screen_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::TEXT)
                    .child(title),
            )
            .child(self.summarize_button(busy, available));

        let body = div()
            .id("summary-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(shell::DETAIL_PADDING))
            .child(match self.text.clone() {
                Some(text) => div()
                    .max_w(px(shell::READING_WIDTH))
                    .child(
                        TextView::markdown(gpui::ElementId::Name("summary".into()), text)
                            .selectable(true),
                    )
                    .into_any_element(),
                None => div()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::TEXT_DIM)
                    .child(if available {
                        "Summarize yesterday to write the first one."
                    } else {
                        "Summaries run through warp-agent. Turn it on in Settings."
                    })
                    .into_any_element(),
            });

        shell::panes()
            .child(list)
            .child(shell::detail_pane().child(head).child(body))
    }
}
