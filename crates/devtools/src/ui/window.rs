//! The main window: custom titlebar + page content. Hosts the Repos page;
//! Settings swaps content and Notes opens a floating window (later waves).

use crate::store::Store;
use crate::theme;
use crate::ui::bottom_bar::BottomBar;
use crate::ui::icon;
use crate::ui::notes::NotesPage;
use gpui::prelude::*;
use gpui::{div, px, size, App, Bounds, Context, Entity, FontWeight, MouseButton, Render, Window, WindowControlArea};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::Sizable as _;

/// Which page fills the window body (the bottom bar rides only with Repos).
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Repos,
    Settings,
}

pub struct MainWindow {
    pub store: Entity<Store>,
    page: Page,
    repos_page: Entity<crate::ui::repos::ReposPage>,
    settings_page: Entity<crate::ui::settings::SettingsPage>,
    bottom_bar: Entity<BottomBar>,
    global_search: Entity<InputState>,
    notes_window: Option<gpui::AnyWindowHandle>,
    formatters_open: bool,
    formatters: Option<Entity<crate::ui::formatters::FormattersDrawer>>,
    tools_menu_open: bool,
}

impl MainWindow {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let repos_page = cx.new(|cx| crate::ui::repos::ReposPage::new(store.clone(), window, cx));
        let settings_page = cx.new(|cx| crate::ui::settings::SettingsPage::new(store.clone(), window, cx));
        let bottom_bar = cx.new(|cx| BottomBar::new(store.clone(), window, cx));
        let global_search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search repos and notes  (Ctrl+K)")
        });
        let search_store = store.clone();
        cx.subscribe(&global_search, move |_, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let term = field.read(cx).value().to_string();
                search_store.update(cx, |store, cx| store.run_global_search(&term, cx));
            }
        })
        .detach();
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            page: Page::Repos,
            repos_page,
            settings_page,
            bottom_bar,
            global_search,
            notes_window: None,
            formatters_open: false,
            formatters: None,
            // E2E hook: DEVTOOLS_TOOLS_MENU=1 opens the tools dropdown on launch
            tools_menu_open: std::env::var("DEVTOOLS_TOOLS_MENU").as_deref() == Ok("1"),
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if self.store.read(cx).notes_requested {
            self.store.update(cx, |s, _| s.notes_requested = false);
            self.open_notes_window(window, cx);
        }
        if self.settings_page.read(cx).back_requested {
            self.settings_page.update(cx, |page, _| page.back_requested = false);
            self.page = Page::Repos;
        }
        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::BACKGROUND)
            .text_color(theme::TEXT)
            .font_family(theme::ui_font())
            // Ctrl+K focuses the global-search field (the titlebar's hint)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "k" && event.keystroke.modifiers.control {
                    this.global_search.update(cx, |state, cx| state.focus(window, cx));
                    cx.stop_propagation();
                }
            }))
            // mounts the window's selection machinery: without this layer the
            // kit's global mouse handlers never register and selectable
            // TextViews (README, notes preview) ignore drags entirely
            .child(gpui_kit::base::TextSelectionLayer)
            .child(self.render_titlebar(window, cx))
            .child(self.render_body(window, cx))
            .relative();
        if let Some(ask) = self.render_wand_ask(cx) {
            root = root.child(ask);
        }
        if let Some(popup) = self.render_search_popup(cx) {
            root = root.child(popup);
        }
        if self.formatters_open
            && let Some(drawer) = &self.formatters {
                let scrim_store = self.store.clone();
                root = root.child(
                    div()
                        .id("formatters-scrim")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.formatters_open = false;
                            cx.notify();
                            let _ = scrim_store;
                        }))
                        .child(
                            div()
                                .absolute()
                                .top(px(38.))
                                .right_0()
                                .bottom_0()
                                .child(drawer.clone()),
                        ),
                );
            }
        if let Some(menu) = self.render_tools_menu(cx) {
            root = root.child(menu);
        }
        root
    }
}

impl MainWindow {
    /// The tools dropdown under the titlebar tools button: one entry per
    /// tool surface (Formatters drawer, Notes window). Dismissed by the
    /// full-window scrim or by picking an item.
    fn render_tools_menu(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.tools_menu_open {
            return None;
        }
        let open_formatters = cx.listener(|this, _, window, cx| {
            this.tools_menu_open = false;
            this.formatters_open = true;
            if this.formatters.is_none() {
                this.formatters = Some(cx.new(|cx| {
                    crate::ui::formatters::FormattersDrawer::new(this.store.clone(), window, cx)
                }));
            }
            cx.notify();
        });
        let open_notes = cx.listener(|this, _, window, cx| {
            this.tools_menu_open = false;
            this.open_notes_window(window, cx);
        });
        type ClickHandler = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
        let row = |id: &'static str,
                   icon_path: &'static str,
                   label: &'static str,
                   on_click: ClickHandler| {
            div()
                .id(id)
                .cursor_pointer()
                .rounded_md()
                .px_2()
                .py_1p5()
                .flex()
                .items_center()
                .gap_2()
                .hover(|s| s.bg(theme::HOVER))
                .on_click(on_click)
                .child(icon(icon_path, px(theme::icon_small()), theme::TEXT_MUTED))
                .child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT)
                        .child(label),
                )
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("tools-menu-scrim")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tools_menu_open = false;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("tools-menu")
                        .absolute()
                        .top(px(42.))
                        // right edge of the menu = right edge of the tools
                        // button: 8px titlebar padding + three 26px window
                        // buttons + settings (+ SnapIt on Windows), 4px gaps
                        .right(px(if cfg!(windows) { 158. } else { 128. }))
                        .w(px(200.))
                        .occlude()
                        .rounded_lg()
                        .border_1()
                        .border_color(theme::HAIRLINE)
                        .bg(theme::POPUP)
                        .shadow_lg()
                        .p_1()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(row(
                            "tools-menu-formatters",
                            "icons/icon-text-format.svg",
                            "Formatters",
                            Box::new(open_formatters),
                        ))
                        .child(row(
                            "tools-menu-notes",
                            "icons/icon-note.svg",
                            "Notes",
                            Box::new(open_notes),
                        )),
                )
                .into_any_element(),
        )
    }

    /// The global-search results popup under the titlebar field.
    fn render_search_popup(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {        let term = self.global_search.read(cx).value().to_string();
        if term.trim().is_empty() {
            return None;
        }
        let hits = self.store.read(cx).global_search.clone();
        if hits.is_empty() {
            // the popup shows a "No matches." footer while focused
            return Some(
                div()
                    .id("search-popup")
                    .absolute()
                    .top(px(40.))
                    .left(px(150.))
                    .w(px(420.))
                    .occlude()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .bg(theme::POPUP)
                    .shadow_lg()
                    .p_2()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_FAINT)
                    .child("No matches.")
                    .into_any_element(),
            );
        }
        let mut rows = div().flex().flex_col().gap_0p5();
        for (index, hit) in hits.iter().take(12).enumerate() {
            let (title, detail, kind_color) = match hit {
                crate::store::GlobalSearchHit::Repo { name, folder } => {
                    (name.clone(), folder.clone(), theme::ACCENT)
                }
                crate::store::GlobalSearchHit::Note { title, excerpt, .. } => {
                    (title.clone(), excerpt.clone(), theme::BLUE_BRIGHT)
                }
            };
            let hit_for_click = hit.clone();
            rows = rows.child(
                div()
                    .id(("search-hit", index as u64))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_search_hit(&hit_for_click, window, cx);
                    }))
                    .child(
                        div()
                            .text_size(px(theme::font_small() - 1.))
                            .text_color(kind_color)
                            .child(match hit {
                                crate::store::GlobalSearchHit::Repo { .. } => "repo",
                                crate::store::GlobalSearchHit::Note { .. } => "note",
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(theme::font_small())).text_color(theme::TEXT_DIM).child(title))
                            .child(
                                div()
                                    .text_size(px(theme::font_small() - 2.))
                                    .text_color(theme::TEXT_FAINT)
                                    .child(detail),
                            ),
                    ),
            );
        }
        Some(
            div()
                .id("search-popup")
                .absolute()
                .top(px(40.))
                .left(px(150.))
                .w(px(420.))
                .occlude()
                .rounded_lg()
                .border_1()
                .border_color(theme::HAIRLINE)
                .bg(theme::POPUP)
                .shadow_lg()
                .p_1()
                .child(rows)
                .into_any_element(),
        )
    }

    fn activate_search_hit(
        &mut self,
        hit: &crate::store::GlobalSearchHit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match hit {
            crate::store::GlobalSearchHit::Repo { folder, .. } => {
                self.page = Page::Repos;
                // open on Overview without the row-press toggle semantics
                let folder = folder.clone();
                self.store.update(cx, |s, cx| s.open_repo_tab(&folder, crate::store::BottomBarTab::Overview, cx));
            }
            crate::store::GlobalSearchHit::Note { path, .. } => {
                self.store.update(cx, |s, cx| {
                    s.pending_note_open = Some(path.clone());
                    // the notes page drains this on its next render; the
                    // notify is what makes an already-open notes window
                    // re-render (its store observer fires on it)
                    cx.notify();
                });
                self.open_notes_window(window, cx);
            }
        }
        self.global_search.update(cx, |state, cx| state.set_value("", window, cx));
        self.store.update(cx, |s, cx| s.run_global_search("", cx));
        cx.notify();
    }

    /// Opens (or focuses) the floating notes window.
    pub fn open_notes_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = &self.notes_window {
            if cx.windows().contains(handle) {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                return;
            }
            self.notes_window = None;
        }
        let _ = window;
        let store = self.store.clone();
        let bounds = Bounds::centered(None, size(px(980.), px(700.)), cx);
        if let Ok((handle, _)) = gpui_kit::open_window(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                app_id: Some("devtools".into()),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Notes".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| NotesPage::new(store.clone(), window, cx))
            },
        ) {
            self.notes_window = Some(handle);
        }
        cx.notify();
    }
}

impl MainWindow {
    /// The opencode permission popup: allow once / allow all / reject, with
    /// the auto-reject countdown.
    fn render_wand_ask(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let (wand_id, ask_id, title, secs) = self.store.read(cx).wand_ask.clone()?;
        let store = self.store.clone();
        let store_all = self.store.clone();
        let store_reject = self.store.clone();
        let _ = (&wand_id, &ask_id);
        Some(
            div()
                .id("wand-ask-overlay")
                .absolute()
                .inset_0()
                .occlude()
                .bg(gpui::black().opacity(0.45))
                .flex()
                .items_start()
                .justify_center()
                .pt(px(90.))
                .child(
                    div()
                        .id("wand-ask-card")
                        .occlude()
                        .min_w(px(420.))
                        .rounded_xl()
                        .border_1()
                        .border_color(theme::tint(theme::WARN))
                        .bg(theme::POPUP)
                        .shadow_lg()
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .on_click(|_, _, _| {})
                        .child(
                            div()
                                .text_size(px(theme::font_title()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("OpenCode needs permission"),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_small()))
                                .text_color(theme::TEXT_DIM)
                                .child(title.clone()),
                        )
                        .child(
                            div().flex().flex_row().justify_end().gap_2()
                                .child(ask_button("Reject", theme::DANGER, move |_, _, cx| {
                                    store_reject.update(cx, |s, cx| s.respond_wand("reject", cx));
                                }))
                                .child(ask_button("Allow all", theme::ACCENT, move |_, _, cx| {
                                    store_all.update(cx, |s, cx| s.respond_wand("always", cx));
                                }))
                                .child(ask_button("Allow once", theme::SUCCESS_BRIGHT, move |_, _, cx| {
                                    store.update(cx, |s, cx| s.respond_wand("once", cx));
                                })),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_small() - 1.))
                                .text_color(theme::TEXT_FAINT)
                                .child(format!("auto-reject in {secs}s")),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn ask_button(
    label: &str,
    color: gpui::Rgba,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(gpui::SharedString::from(format!("wand-ask-{label}")))
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::font_small()))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::tint(color))
        .text_color(color)
        .hover(|s| s.opacity(0.85))
        .on_click(on_click)
        .into_any_element()
}

pub(crate) fn window_button(
    id: &'static str,
    icon_path: &'static str,
    hover_bg: gpui::Hsla,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl gpui::IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .size(px(26.))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |s| s.bg(hover_bg))
        .on_click(on_click)
        .child(icon(icon_path, px(theme::icon_medium()), theme::TEXT_MUTED))
}

impl MainWindow {
    fn render_titlebar(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .h(px(38.))
            .flex()
            .items_center()
            .px_2()
            .gap_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                gpui_kit::base::GlobalState::suppress_text_selection(cx);
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(30.))
                    .child(icon("icons/icon-logo-mark.svg", px(theme::icon_large()), theme::TEXT)),
            )
            .child(
                div()
                    .text_size(px(theme::font_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Dev Tools"),
            )
            .child(div().mx_1().h(px(16.)).w(px(1.)).bg(theme::HAIRLINE))
            .child({
                div()
                    .id("notes-toggle")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.notes_window.is_some(), |d| d.bg(theme::tint(theme::ACCENT)))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_notes_window(window, cx);
                    }))
                    .child(icon(
                        "icons/icon-note.svg",
                        px(theme::icon_medium()),
                        if self.notes_window.is_some() { theme::ACCENT } else { theme::TEXT_MUTED },
                    ))
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .child(div().w(px(480.)).child(Input::new(&self.global_search).small())),
            )
            .child(
                div()
                    .id("titlebar-drag")
                    .flex_1()
                    .h_full()
                    // Windows drags via the hit-test path (this hitbox answers
                    // HTCAPTION); start_window_move is a no-op there. Linux
                    // keeps the explicit move call.
                    .window_control_area(WindowControlArea::Drag)
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.start_window_move();
                    }),
            )
            .child({
                div()
                    .id("tools-toggle")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.tools_menu_open, |d| d.bg(theme::tint(theme::ACCENT)))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tools_menu_open = !this.tools_menu_open;
                        cx.notify();
                    }))
                    .child(icon(
                        "icons/icon-tools.svg",
                        px(theme::icon_medium()),
                        if self.tools_menu_open { theme::ACCENT } else { theme::TEXT_MUTED },
                    ))
            })
            .when(cfg!(windows), |d| {
                let running = devtools_core::snapit::is_running();
                d.child({
                    div()
                        .id("snapit-toggle")
                        .cursor_pointer()
                        .rounded_md()
                        .size(px(26.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(running, |d| d.bg(theme::tint(theme::ACCENT)))
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = if devtools_core::snapit::is_running() {
                                devtools_core::snapit::stop()
                            } else {
                                devtools_core::snapit::start()
                            };
                            if let Err(err) = result {
                                this.store.update(cx, |s, cx| {
                                    s.notice = Some((err, std::time::Instant::now()));
                                    cx.notify();
                                });
                            }
                            cx.notify();
                        }))
                        .child(icon(
                            "icons/icon-grid.svg",
                            px(theme::icon_medium()),
                            if running { theme::ACCENT } else { theme::TEXT_MUTED },
                        ))
                })
            })
            .child({
                let on_settings = self.page == Page::Settings;
                div()
                    .id("settings-toggle")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(on_settings, |d| d.bg(theme::tint(theme::ACCENT)))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.page = if this.page == Page::Settings { Page::Repos } else { Page::Settings };
                        cx.notify();
                    }))
                    .child(icon(
                        "icons/icon-cog.svg",
                        px(theme::icon_medium()),
                        if on_settings { theme::ACCENT } else { theme::TEXT_MUTED },
                    ))
            })
            .child(window_button(
                "minimize-window",
                "icons/icon-window-minimize.svg",
                theme::HOVER,
                |_, window, _| window.minimize_window(),
            ))
            .child(window_button(
                "zoom",
                if window.is_maximized() {
                    "icons/icon-window-restore.svg"
                } else {
                    "icons/icon-window-maximize.svg"
                },
                theme::HOVER,
                cx.listener(|_, _, window, cx| {
                    window.zoom_window();
                    // the compositor toggles asynchronously — nudge a
                    // re-render so the maximize/restore glyph keeps up
                    cx.notify();
                }),
            ))
            .child(window_button(
                "close-window",
                "icons/icon-window-close.svg",
                theme::tint(theme::DANGER),
                |_, window, _| window.remove_window(),
            ))
            .into_any_element()
    }

    fn render_body(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.page {
            Page::Repos => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(div().flex_1().min_h_0().child(self.repos_page.clone()))
                .child(self.bottom_bar.clone())
                .into_any_element(),
            Page::Settings => div()
                .flex_1()
                .min_h_0()
                .child(self.settings_page.clone())
                .into_any_element(),
        }
    }
}
