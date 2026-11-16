//! The Merge section: one stopped operation at a time — its unmerged files on
//! the left, the Incoming and Current panes on the right. The session comes
//! from `warp_station_core::git::merge`; the editable Result and the staging
//! verbs are later phases, so this page only reads.

use crate::store::Store;
use crate::theme;
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, FontWeight, Render, SharedString, Window, div, px, uniform_list,
};
use std::path::Path;
use std::rc::Rc;
use warp_station_core::git::merge::{MergeFile, MergeOperation, MergeRow, MergeSession};
use warpui_kit_core::{motion, shell};

/// One source-pane row: the diff's mono line height.
const LINE_H: f32 = 21.0;
/// The line-number gutter's width.
const GUTTER_W: f32 = 46.0;

pub struct MergePage {
    store: Entity<Store>,
    /// The session snapshot the page draws; refreshed when the store's
    /// generation moves, so the page does not clone the session per frame.
    session: Option<MergeSession>,
    generation: u64,
    /// The file picked in the conflict list.
    selected: usize,
    /// The selected file's rows and stage lines, cloned once per pick.
    shown: Option<ShownFile>,
}

struct ShownFile {
    key: (u64, usize, String),
    rows: Rc<Vec<MergeRow>>,
    ours: Rc<Vec<String>>,
    theirs: Rc<Vec<String>>,
}

impl MergePage {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            session: None,
            generation: 0,
            selected: 0,
            shown: None,
        }
    }

    fn pick_file(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.selected == index {
            return;
        }
        self.selected = index;
        self.shown = None;
        cx.notify();
    }

    /// Clones the selected file's rows and stage lines once, not per frame.
    fn sync_shown(&mut self) {
        let file = self
            .session
            .as_ref()
            .and_then(|session| session.files.get(self.selected));
        let key = (
            self.generation,
            self.selected,
            file.map(|file| file.path().to_string()).unwrap_or_default(),
        );
        if self.shown.as_ref().is_some_and(|shown| shown.key == key) {
            return;
        }
        self.shown = match file {
            Some(MergeFile::Text(text)) => Some(ShownFile {
                key,
                rows: Rc::new(text.rows.clone()),
                ours: Rc::new(text.ours.clone()),
                theirs: Rc::new(text.theirs.clone()),
            }),
            _ => None,
        };
    }
}

impl Render for MergePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.store
            .update(cx, |store, cx| store.ensure_merge_loaded(cx));
        let (repos, target, generation) = {
            let store = self.store.read(cx);
            (
                store.merge_repos(),
                store.merge_target(),
                store.merge_generation,
            )
        };
        if generation != self.generation {
            self.generation = generation;
            self.session = self.store.read(cx).merge.clone();
            self.selected = 0;
            self.shown = None;
        }
        let Some(target) = target else {
            return div().size_full().into_any_element();
        };
        if self.session.is_none() {
            return loading(&target).into_any_element();
        }
        self.sync_shown();
        let Some(session) = self.session.as_ref() else {
            return loading(&target).into_any_element();
        };

        let header = self.header(&target, &repos, session, cx);
        let list = self.file_list(session, cx);
        let detail = self.detail(session, cx);
        div()
            .id("merge-page")
            .debug_selector(|| "merge-page".into())
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(header)
            .child(shell::panes().child(list).child(detail))
            .into_any_element()
    }
}

impl MergePage {
    fn header(
        &self,
        target: &str,
        repos: &[(String, MergeOperation)],
        session: &MergeSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .id("merge-header")
            .debug_selector(|| "merge-header".into())
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .h(px(46.))
            .flex_none()
            .border_b_1()
            .border_color(theme::hairline())
            .child(
                div()
                    .text_size(px(theme::font_body()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child("Merge"),
            )
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_dim())
                    .child(session.operation.title()),
            )
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(repo_name(target)),
            );
        let unresolved = session.unresolved();
        if unresolved > 0 {
            row = row.child(
                div()
                    .px_2()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .rounded(px(10.))
                    .bg(theme::tint(theme::WARN))
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::text())
                    .child(format!("{unresolved} unresolved")),
            );
        }
        row = row.child(div().flex_1());
        if repos.len() > 1 {
            for (folder, operation) in repos {
                let folder = folder.clone();
                let picked = folder.clone();
                let selected = folder == target;
                row = row.child(
                    div()
                        .id(SharedString::from(format!("merge-repo-{folder}")))
                        .debug_selector(|| "merge-repo-chip".into())
                        .cursor_pointer()
                        .px_2()
                        .h(px(20.))
                        .flex()
                        .items_center()
                        .rounded(px(10.))
                        .text_size(px(theme::font_micro()))
                        .when(selected, |chip| {
                            chip.bg(theme::tint(theme::accent()))
                                .text_color(theme::text())
                        })
                        .when(!selected, |chip| {
                            chip.bg(theme::panel()).text_color(theme::text_dim())
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = 0;
                            this.shown = None;
                            this.store.update(cx, |store, cx| {
                                store.select_merge_folder(picked.clone(), cx);
                            });
                        }))
                        .child(format!("{} · {}", repo_name(&folder), operation.title())),
                );
            }
        }
        row.into_any_element()
    }

    fn file_list(&self, session: &MergeSession, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div()
            .id("merge-files")
            .debug_selector(|| "merge-files".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for (index, file) in session.files.iter().enumerate() {
            rows = rows.child(self.file_row(index, file, cx));
        }
        shell::list_pane(self.store.read(cx).list_width())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .h(px(34.))
                    .flex_none()
                    .border_b_1()
                    .border_color(theme::hairline())
                    .child(
                        div()
                            .text_size(px(theme::font_micro()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text_muted())
                            .child("UNMERGED FILES"),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_micro()))
                            .text_color(theme::text_muted())
                            .child(session.files.len().to_string()),
                    ),
            )
            .child(rows)
            .into_any_element()
    }

    fn file_row(&self, index: usize, file: &MergeFile, cx: &mut Context<Self>) -> AnyElement {
        let selected = index == self.selected;
        let (detail, attention) = match file {
            MergeFile::Text(text) => {
                let unresolved = text.unresolved();
                (format!("{unresolved} unresolved"), unresolved > 0)
            }
            MergeFile::WholeFile(whole) => (whole.reason.label().to_string(), true),
        };
        shell::list_row(SharedString::from(format!("merge-file-{index}")), selected)
            .debug_selector(|| "merge-file-row".into())
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text())
                    .child(file_name(file.path())),
            )
            .child(
                div()
                    .text_size(px(theme::font_micro()))
                    .when(attention, |line| line.text_color(theme::WARN))
                    .when(!attention, |line| line.text_color(theme::text_muted()))
                    .child(detail),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.pick_file(index, cx)))
            .into_any_element()
    }

    fn detail(&self, session: &MergeSession, cx: &mut Context<Self>) -> AnyElement {
        let file = session.files.get(self.selected);
        match file {
            Some(MergeFile::Text(text)) => {
                let Some(shown) = self.shown.as_ref() else {
                    return empty_detail("Reading the conflict…");
                };
                let labels = session.operation.side_labels();
                let ours = shown.ours.clone();
                let theirs = shown.theirs.clone();
                let rows = shown.rows.clone();
                let id = SharedString::from(format!("merge-panes-{}-{}", text.path, rows.len()));
                let list = uniform_list(
                    id,
                    rows.len(),
                    cx.processor(move |_this, range: std::ops::Range<usize>, _window, _cx| {
                        range
                            .map(|index| pane_row(index, &rows, &ours, &theirs))
                            .collect::<Vec<_>>()
                    }),
                );
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(pane_head(labels, text.ours.len(), text.theirs.len()))
                    .child(
                        div()
                            .debug_selector(|| "merge-panes".into())
                            .flex_1()
                            .min_h_0()
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_mono_meta()))
                            .child(list.size_full()),
                    )
                    .into_any_element()
            }
            Some(MergeFile::WholeFile(whole)) => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(file_name(&whole.path)),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(whole.reason.label()),
                )
                .child(
                    div()
                        .text_size(px(theme::font_micro()))
                        .text_color(theme::text_muted())
                        .child("Take ours / take theirs lands with the staging phase."),
                )
                .into_any_element(),
            None => empty_detail("No file selected"),
        }
    }
}

fn pane_head(labels: (&'static str, &'static str), ours: usize, theirs: usize) -> AnyElement {
    let half = |label: &'static str, lines: usize| {
        div()
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .px_2()
            .h_full()
            .text_size(px(theme::font_micro()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::text_muted())
            .child(format!("{label} · {lines} lines"))
    };
    div()
        .flex()
        .flex_row()
        .flex_none()
        .h(px(28.))
        .items_center()
        .border_b_1()
        .border_color(theme::hairline())
        .child(half(labels.0, ours))
        .child(div().w(px(1.)).h_full().bg(theme::hairline()))
        .child(half(labels.1, theirs))
        .into_any_element()
}

fn pane_row(index: usize, rows: &[MergeRow], ours: &[String], theirs: &[String]) -> AnyElement {
    let Some(row) = rows.get(index).copied() else {
        return div().into_any_element();
    };
    let ours_text = row
        .ours
        .and_then(|line| ours.get(line as usize))
        .map(String::as_str)
        .unwrap_or("");
    let theirs_text = row
        .theirs
        .and_then(|line| theirs.get(line as usize))
        .map(String::as_str)
        .unwrap_or("");
    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(LINE_H))
        .w_full()
        .when(row.block.is_some(), |line| {
            line.bg(theme::tint(theme::WARN))
        })
        .child(gutter(row.ours))
        .child(code_cell(ours_text))
        .child(div().w(px(1.)).h_full().bg(theme::hairline()))
        .child(gutter(row.theirs))
        .child(code_cell(theirs_text))
        .into_any_element()
}

fn gutter(line: Option<u32>) -> AnyElement {
    div()
        .w(px(GUTTER_W))
        .flex_none()
        .flex()
        .justify_end()
        .pr_2()
        .text_color(theme::text_muted())
        .child(line.map(|line| format!("{}", line + 1)).unwrap_or_default())
        .into_any_element()
}

fn code_cell(text: &str) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .px_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(theme::text())
        .child(text.to_string())
        .into_any_element()
}

fn loading(target: &str) -> AnyElement {
    div()
        .id("merge-page")
        .debug_selector(|| "merge-page".into())
        .size_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_4()
                .h(px(46.))
                .border_b_1()
                .border_color(theme::hairline())
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Merge"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(repo_name(target)),
                ),
        )
        .child(
            div()
                .debug_selector(|| "merge-loading".into())
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .child(motion::spinner(
                    "merge-loading",
                    "icons/icon-merge.svg",
                    px(16.),
                    theme::text_muted(),
                ))
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child("Reading the stopped operation…"),
                ),
        )
        .into_any_element()
}

fn empty_detail(text: &str) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_muted())
        .child(text.to_string())
        .into_any_element()
}

fn repo_name(folder: &str) -> String {
    Path::new(folder)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| folder.to_string())
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}
