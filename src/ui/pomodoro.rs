//! The Pomodoro section: the live state of the sibling warp-pomodoro timer —
//! its countdown, phase, cycle and day counts — with the three verbs Station
//! owns. Start/Pause/Reset ride the app's door; when the app is closed, Start
//! launches it with `--start`, and "Open Pomodoro" always opens it for the
//! settings and history that live there. The page polls the snapshot once a
//! second only while its section is on screen, off the UI thread; a closed app
//! is a state, never an error toast.

use crate::store::Store;
use crate::theme;
use crate::ui::{neutral_button, primary_button};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use std::path::PathBuf;
use std::time::Duration;
use warp_station_core::pomodoro::{self, PomodoroAnswer, PomodoroCommand, PomodoroStatus};
use warpui_kit_core::shell;

/// How often the live snapshot is read while the section is on screen. One lap
/// is one door call (about a millisecond), or one `state.json` read for a
/// closed app, which rewrites that file only on its own edges — so this beat is
/// the page's, not the writer's.
const POLL: Duration = Duration::from_secs(1);

/// The timer card's reading width: the family's `.measure` for running
/// content, the same column the settings page uses.
const CARD_W: f32 = shell::READING_WIDTH;

/// The app's own folder. Probes and a scratch HOME redirect it with
/// `WARP_STATION_POMODORO_DIR`, read once when the window builds its page, so
/// no probe reads the developer's `~/.warp/pomodoro`.
fn app_dir() -> PathBuf {
    std::env::var_os("WARP_STATION_POMODORO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(warp_station_core::paths::AppPaths::pomodoro_dir)
}

pub struct PomodoroPage {
    store: Entity<Store>,
    /// `~/.warp/pomodoro` (or the probe's stand-in), read once.
    dir: PathBuf,
    /// The last snapshot read; `None` until the first poll answers.
    status: Option<PomodoroStatus>,
    /// The section is on screen. The poll loop runs only while this is true.
    active: bool,
    /// Bumped whenever the loop is parked, so an in-flight lap stops instead
    /// of overwriting a newer one.
    generation: u64,
    /// A door command or a launch is in flight.
    busy: bool,
}

impl PomodoroPage {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            dir: app_dir(),
            status: None,
            active: false,
            generation: 0,
            busy: false,
        }
    }

    /// The window tells the page when its section is on screen: the poll loop
    /// starts on the rising edge and parks on the falling one. Called from the
    /// window's render, so it is a no-op unless the state actually changed.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.generation = self.generation.wrapping_add(1);
        cx.notify();
        if !active {
            return;
        }
        let generation = self.generation;
        let dir = self.dir.clone();
        cx.spawn(async move |this, cx| {
            loop {
                // the door (or the file) is read on the background executor:
                // a 400 ms connect timeout must never run on the UI thread
                let read_dir = dir.clone();
                let read = cx
                    .background_executor()
                    .spawn(async move { pomodoro::read_status_in(&read_dir) })
                    .await;
                let alive = this
                    .update(cx, |page, cx| {
                        if !page.active || page.generation != generation {
                            return false;
                        }
                        page.apply_status(read, cx);
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
                cx.background_executor().timer(POLL).await;
            }
        })
        .detach();
    }

    fn apply_status(&mut self, next: Option<PomodoroStatus>, cx: &mut Context<Self>) {
        if self.status != next {
            self.status = next;
            cx.notify();
        }
    }

    fn apply_snapshot(&mut self, snapshot: pomodoro::PomodoroSnapshot, cx: &mut Context<Self>) {
        let next = Some(PomodoroStatus {
            snapshot,
            live: true,
        });
        self.apply_status(next, cx);
    }

    /// Start or resume the timer. A live door takes the command; a closed app
    /// is launched with `--start`, which is the same transition from the page.
    fn primary_clicked(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match &self.status {
            Some(status) if status.live => {
                let command = if status.snapshot.running {
                    PomodoroCommand::Pause
                } else {
                    PomodoroCommand::Start
                };
                self.command(command, cx);
            }
            _ => self.launch(true, cx),
        }
    }

    fn reset_clicked(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.live() {
            return;
        }
        self.command(PomodoroCommand::Reset, cx);
    }

    fn open_clicked(&mut self, cx: &mut Context<Self>) {
        self.launch(false, cx);
    }

    fn live(&self) -> bool {
        self.status.as_ref().is_some_and(|status| status.live)
    }

    fn running(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status.live && status.snapshot.running)
    }

    /// One door command, off the UI thread; its snapshot answer lands at once
    /// instead of waiting for the next poll.
    fn command(&mut self, command: PomodoroCommand, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        cx.notify();
        let dir = self.dir.clone();
        cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { pomodoro::send_in(&dir, command) })
                .await;
            let _ = this.update(cx, |page, cx| {
                page.busy = false;
                if let Ok(PomodoroAnswer::Snapshot(snapshot)) = answer {
                    page.apply_snapshot(snapshot, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A detached launch: `start` asks the app for `--start`, false only opens
    /// its window. A missing binary shows the page's hint, never a toast.
    fn launch(&mut self, start: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        cx.notify();
        let executable = self.store.read(cx).settings().pomodoro.executable;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { pomodoro::launch(&executable, start) })
                .await;
            let _ = this.update(cx, |page, cx| {
                page.busy = false;
                if result.is_err() {
                    // the hint row already names the executable; no toast
                    warp_station_core::exe::invalidate();
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for PomodoroPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let executable = self.store.read(cx).settings().pomodoro.executable;
        let executable = executable.trim().to_string();
        let installed = warp_station_core::exe::locate(&executable).is_some();
        let snapshot = self.status.as_ref().map(|status| status.snapshot.clone());
        let live = self.live();
        let running = self.running();
        let busy = self.busy;

        let word = if running {
            "Running"
        } else if live {
            "Paused"
        } else {
            "Not running"
        };
        let dot = if running {
            theme::SUCCESS_BRIGHT
        } else if live {
            theme::AMBER_BRIGHT
        } else {
            theme::line_strong()
        };
        let last_seen = self
            .status
            .as_ref()
            .filter(|status| !status.live)
            .and_then(|status| last_seen(&status.snapshot.updated_at));

        let mut status_row = div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(theme::font_meta()))
            .child(div().size_2().rounded_full().bg(dot))
            .child(
                div()
                    .debug_selector(move || format!("pomodoro-status-{word}"))
                    .text_color(if running {
                        theme::text()
                    } else {
                        theme::text_dim()
                    })
                    .child(word),
            );
        if let Some(seen) = last_seen {
            status_row = status_row.child(
                div()
                    .debug_selector(|| "pomodoro-last-seen".into())
                    .text_color(theme::text_muted())
                    .child(format!("· last seen {seen}")),
            );
        }

        let head = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(theme::font_screen_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child("Pomodoro"),
            )
            .child(status_row);

        let mut card = div()
            .w_full()
            .max_w(px(CARD_W))
            .rounded_lg()
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::panel())
            .p_4()
            .flex()
            .flex_col()
            .gap_4();

        match snapshot {
            Some(snapshot) => {
                let remaining = remaining_label(snapshot.remaining_seconds);
                let total = snapshot.long_break_every.clamp(1, 12);
                let done = snapshot.completed_in_cycle.min(total);
                let mut dots = div().flex().items_center().gap_1p5();
                for dot in 0..total {
                    dots = dots.child(div().size_2().rounded_full().bg(if dot < done {
                        theme::accent()
                    } else {
                        theme::line_strong()
                    }));
                }
                card = card
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .debug_selector({
                                        let label = remaining.clone();
                                        move || format!("pomodoro-remaining-{label}")
                                    })
                                    .font_family(theme::code_font())
                                    .text_size(px(theme::font_screen_title()))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if running {
                                        theme::text()
                                    } else {
                                        theme::text_dim()
                                    })
                                    .child(remaining),
                            )
                            .child(
                                div()
                                    .debug_selector(move || {
                                        format!("pomodoro-phase-{}", snapshot.phase.label())
                                    })
                                    .flex_shrink_0()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(theme::raised())
                                    .text_size(px(theme::font_meta()))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme::text_dim())
                                    .child(snapshot.phase.label()),
                            ),
                    )
                    .child(
                        div().flex().items_center().gap_3().child(dots).child(
                            div()
                                .debug_selector(move || {
                                    format!("pomodoro-cycle-Cycle {done} of {total}")
                                })
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .child(format!("Cycle {done} of {total}")),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_6()
                            .child(stat("Today", snapshot.today_work, "pomodoro-today"))
                            .child(stat("Last 7 days", snapshot.week_work, "pomodoro-week")),
                    );
            }
            None => {
                card = card
                    .child(
                        div()
                            .debug_selector(|| "pomodoro-empty".into())
                            .text_size(px(theme::font_body()))
                            .text_color(theme::text_dim())
                            .child("No session yet."),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child("Start launches the timer with a work session."),
                    );
            }
        }

        let primary_label = if running { "Pause" } else { "Start" };
        let primary_enabled = !busy && (live || installed);
        let reset_enabled = !busy && live;
        let open_enabled = !busy && installed;
        card = card.child(
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .child(primary_button(
                    "pomodoro-primary",
                    primary_label,
                    primary_enabled,
                    cx.listener(|this, _, _, cx| this.primary_clicked(cx)),
                ))
                .child(neutral_button(
                    "pomodoro-reset",
                    "Reset",
                    reset_enabled,
                    cx.listener(|this, _, _, cx| this.reset_clicked(cx)),
                ))
                .child(neutral_button(
                    "pomodoro-open",
                    "Open Pomodoro",
                    open_enabled,
                    cx.listener(|this, _, _, cx| this.open_clicked(cx)),
                )),
        );

        let mut footer = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .debug_selector(|| "pomodoro-footer".into())
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(
                        "Durations, the long-break cycle, history and notifications live in warp-pomodoro.",
                    ),
            );
        if !installed {
            footer = footer.child(
                div()
                    .debug_selector(|| "pomodoro-missing-exe".into())
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_dim())
                    .child(format!(
                        "{executable} was not found — set the Pomodoro executable in Settings."
                    )),
            );
        }

        div()
            .id("pomodoro-page")
            .debug_selector(|| "pomodoro-page".into())
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_4()
            .p(px(shell::DETAIL_PADDING))
            .child(head)
            .child(card)
            .child(footer)
    }
}

/// One readout: a quiet label over its count.
fn stat(label: &'static str, value: u32, selector: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(label),
        )
        .child(
            div()
                .debug_selector(move || format!("{selector}-{value}"))
                .text_size(px(theme::font_body()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::text())
                .child(SharedString::from(value.to_string())),
        )
}

/// `mm:ss`, or `h:mm:ss` once a custom phase runs past an hour.
fn remaining_label(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let rest = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

/// The snapshot's `updated_at` as a wall clock; an unreadable stamp is left
/// out rather than shown raw.
fn last_seen(updated_at: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(updated_at)
        .ok()
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_countdown_reads_minutes_and_seconds() {
        assert_eq!(remaining_label(1443), "24:03");
        assert_eq!(remaining_label(0), "0:00");
        assert_eq!(remaining_label(59), "0:59");
        assert_eq!(remaining_label(600), "10:00");
    }

    #[test]
    fn a_long_phase_reads_hours() {
        assert_eq!(remaining_label(3600), "1:00:00");
        assert_eq!(remaining_label(3723), "1:02:03");
    }

    #[test]
    fn a_stale_stamp_formats_and_a_bad_one_is_dropped() {
        assert_eq!(
            last_seen("2026-11-15T20:45:14+03:00")
                .as_deref()
                .map(str::len),
            Some(5)
        );
        assert_eq!(last_seen("not a date"), None);
    }
}
