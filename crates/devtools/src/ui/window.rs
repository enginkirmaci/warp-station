//! The main window: custom titlebar + page content. Hosts the Repos page;
//! Settings swaps content and Notes opens a floating window (later waves).

use crate::store::Store;
use crate::theme;
use crate::ui::bottom_bar::BottomBar;
use crate::ui::clipboard_password::{copy_stored_password, ClipboardPasswordDrawer};
use crate::ui::icon;
use crate::ui::notes::NotesPage;
use crate::ui::nuget::NugetDrawer;
use crate::ui::repos::summary_button;
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

/// Which tool drawer docks to the right edge.
#[derive(Clone, Copy, PartialEq)]
enum DrawerKind {
    Formatters,
    Nuget,
    ClipboardPassword,
}

pub struct MainWindow {
    pub store: Entity<Store>,
    page: Page,
    repos_page: Entity<crate::ui::repos::ReposPage>,
    settings_page: Entity<crate::ui::settings::SettingsPage>,
    bottom_bar: Entity<BottomBar>,
    global_search: Entity<InputState>,
    notes_window: Option<gpui::AnyWindowHandle>,
    /// The open tool drawer, if any (one at a time; entities are lazy).
    active_drawer: Option<DrawerKind>,
    formatters: Option<Entity<crate::ui::formatters::FormattersDrawer>>,
    nuget_drawer: Option<Entity<NugetDrawer>>,
    clipboard_drawer: Option<Entity<ClipboardPasswordDrawer>>,
    tools_menu_open: bool,
    /// Window-level focus for the app shortcuts. The window body is not made
    /// of text fields, so without this root handle Escape/Ctrl+K only fire
    /// after an input has been clicked.
    focus_handle: gpui::FocusHandle,
}

impl MainWindow {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let repos_page = cx.new(|cx| crate::ui::repos::ReposPage::new(store.clone(), window, cx));
        let settings_page = cx.new(|cx| crate::ui::settings::SettingsPage::new(store.clone(), window, cx));
        let bottom_bar = cx.new(|cx| BottomBar::new(store.clone(), window, cx));
        let global_search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search repositories and notes")
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
        let focus_handle = cx.focus_handle();
        let this = Self {
            store,
            // E2E hook: DEVTOOLS_PAGE=settings opens Settings on launch
            page: match std::env::var("DEVTOOLS_PAGE").as_deref() {
                Ok("settings") => Page::Settings,
                _ => Page::Repos,
            },
            repos_page,
            settings_page,
            bottom_bar,
            global_search,
            notes_window: None,
            // E2E hook: DEVTOOLS_TOOLS_OPEN=formatters|nuget|clipboard docks
            // that drawer on launch
            active_drawer: match std::env::var("DEVTOOLS_TOOLS_OPEN").as_deref() {
                Ok("formatters") => Some(DrawerKind::Formatters),
                Ok("nuget") => Some(DrawerKind::Nuget),
                Ok("clipboard") => Some(DrawerKind::ClipboardPassword),
                _ => None,
            },
            formatters: None,
            nuget_drawer: None,
            clipboard_drawer: None,
            // E2E hook: DEVTOOLS_TOOLS_MENU=1 opens the tools dropdown on launch
            tools_menu_open: std::env::var("DEVTOOLS_TOOLS_MENU").as_deref() == Ok("1"),
            focus_handle,
        };
        // keep the root focus handle live so Escape / Ctrl+K work before the
        // first input click; inputs take focus back when the user clicks them
        window.focus(&this.focus_handle, cx);
        this
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
            .track_focus(&self.focus_handle)
            // any press outside the bar's kebab menu dismisses it; the menu
            // itself and its trigger stop propagation so their own handlers
            // remain the only ones that see the press
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                gpui_kit::base::GlobalState::suppress_text_selection(cx);
                this.bottom_bar.update(cx, |bar, cx| {
                    bar.dismiss_kebab(cx);
                });
            }))
            // Ctrl+K and Ctrl+F focus the global-search field; Ctrl+Shift+V
            // copies the stored Clipboard Password (fallback for when the
            // system-wide grab in net.rs did not land); Escape dismisses the
            // topmost layer (tools menu, drawer, page dialog, settings, then
            // search text). Any other typed character starts a fresh global
            // search when no text input owns the focus.
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let key = event.keystroke.key.as_str();
                if modifiers.control && modifiers.shift && key == "v" {
                    let message = copy_stored_password(&this.store, cx);
                    this.store.update(cx, |s, cx| {
                        s.notice = Some((message.to_string(), std::time::Instant::now()));
                        cx.notify();
                    });
                    cx.stop_propagation();
                    return;
                }
                if modifiers.control && !modifiers.shift && matches!(key, "f" | "k") {
                    this.global_search.update(cx, |state, cx| {
                        state.focus(window, cx);
                        // typing after the shortcut replaces the previous term
                        state.select_all(window, cx);
                    });
                    cx.stop_propagation();
                    return;
                }
                if key == "escape" {
                    if this.tools_menu_open {
                        this.tools_menu_open = false;
                        cx.notify();
                        cx.stop_propagation();
                        return;
                    }
                    if this.active_drawer.take().is_some() {
                        cx.notify();
                        cx.stop_propagation();
                        return;
                    }
                    if this.bottom_bar.update(cx, |bar, cx| bar.dismiss_kebab(cx)) {
                        cx.stop_propagation();
                        return;
                    }
                    if this.page == Page::Repos
                        && this.repos_page.update(cx, |page, cx| page.dismiss_overlay(cx))
                    {
                        cx.stop_propagation();
                        return;
                    }
                    if this.page == Page::Settings {
                        this.page = Page::Repos;
                        cx.notify();
                        cx.stop_propagation();
                        return;
                    }
                    if !this.global_search.read(cx).value().trim().is_empty() {
                        this.global_search
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.store.update(cx, |s, cx| s.run_global_search("", cx));
                        cx.stop_propagation();
                    }
                }

                // type-anywhere search: a plain printable character (no
                // ctrl/meta — those are shortcuts) starts a fresh term when
                // no text input owns the focus path; every kit text surface
                // carries the "Input" key context, so a focused field keeps
                // its characters. Control characters are excluded so Enter,
                // Tab and friends keep their normal meaning.
                if !modifiers.control
                    && !modifiers.platform
                    && !modifiers.function
                    && !window
                        .context_stack()
                        .iter()
                        .any(|context| context.contains("Input"))
                {
                    let typed = event.keystroke.key_char.clone().filter(|text| {
                        !text.is_empty() && !text.contains(|ch: char| ch.is_control())
                    });
                    if let Some(term) = typed {
                        this.global_search.update(cx, |state, cx| {
                            state.set_value(term.as_str(), window, cx);
                            state.focus(window, cx);
                        });
                        this.store
                            .update(cx, |s, cx| s.run_global_search(&term, cx));
                        cx.stop_propagation();
                    }
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
        if let Some(popup) = self.render_search_popup(window, cx) {
            root = root.child(popup);
        }
        if let Some(drawer) = self.render_drawer_overlay(window, cx) {
            root = root.child(drawer);
        }
        if let Some(menu) = self.render_tools_menu(cx) {
            root = root.child(menu);
        }
        root
    }
}

impl MainWindow {
    /// The tools dropdown under the titlebar tools button: one entry per
    /// tool surface (NuGet Package Manager, Formatters, Clipboard Password,
    /// Notes — the first and third gated by their settings flags). Dismissed
    /// by the full-window scrim or by picking an item.
    fn render_tools_menu(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.tools_menu_open {
            return None;
        }
        let settings = self.store.read(cx).settings();
        let open_drawer = |kind: DrawerKind| {
            cx.listener(move |this, _, _, cx| {
                this.tools_menu_open = false;
                this.active_drawer = Some(kind);
                cx.notify();
            })
        };
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
        let mut menu = div()
            .id("tools-menu")
            .absolute()
            .top(px(theme::TITLEBAR_H + 4.))
            // right edge of the menu = right edge of the tools button: 8px
            // titlebar padding + the two window buttons + the divider + the
            // inset utility pill (2px pad + settings + gap) + the tools button
            .right(px(if cfg!(windows) { 197. } else { 167. }))
            .w(px(224.))
            .occlude()
            .rounded_lg()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .shadow_lg()
            .p_1()
            .flex()
            .flex_col()
            .gap_0p5();
        if settings.nuget_local.enable_nuget {
            menu = menu.child(row(
                "tools-menu-nuget",
                "icons/icon-package.svg",
                "NuGet Package Manager",
                Box::new(open_drawer(DrawerKind::Nuget)),
            ));
        }
        menu = menu.child(row(
            "tools-menu-formatters",
            "icons/icon-text-format.svg",
            "Formatters",
            Box::new(open_drawer(DrawerKind::Formatters)),
        ));
        if settings.clipboard_password.enable_clipboard_password {
            menu = menu.child(row(
                "tools-menu-clipboard",
                "icons/icon-lock.svg",
                "Clipboard Password",
                Box::new(open_drawer(DrawerKind::ClipboardPassword)),
            ));
        }
        menu = menu.child(row(
            "tools-menu-notes",
            "icons/icon-note.svg",
            "Notes",
            Box::new(open_notes),
        ));
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
                .child(menu)
                .into_any_element(),
        )
    }

    /// The docked tool drawer overlay: a full-window scrim plus the right-edge
    /// panel; the drawer entity is created on first open.
    fn render_drawer_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let kind = self.active_drawer?;
        let store = self.store.clone();
        // closing via the drawer's header X routes back into the window
        let close_target = cx.entity();
        let on_close: crate::ui::DrawerCloseHandler = std::rc::Rc::new(move |_, cx| {
            close_target.update(cx, |this, cx| {
                this.active_drawer = None;
                cx.notify();
            });
        });
        let drawer = match kind {
            DrawerKind::Formatters => self
                .formatters
                .get_or_insert_with(|| {
                    let on_close = on_close.clone();
                    let store = store.clone();
                    cx.new(move |cx| {
                        crate::ui::formatters::FormattersDrawer::new(store, window, cx)
                            .with_on_close(move |window, cx| on_close(window, cx))
                    })
                })
                .clone()
                .into_any_element(),
            DrawerKind::Nuget => self
                .nuget_drawer
                .get_or_insert_with(|| {
                    let on_close = on_close.clone();
                    let store = store.clone();
                    cx.new(move |cx| {
                        NugetDrawer::new(store, window, cx)
                            .with_on_close(move |window, cx| on_close(window, cx))
                    })
                })
                .clone()
                .into_any_element(),
            DrawerKind::ClipboardPassword => self
                .clipboard_drawer
                .get_or_insert_with(|| {
                    let on_close = on_close.clone();
                    let store = store.clone();
                    cx.new(move |cx| {
                        ClipboardPasswordDrawer::new(store, window, cx)
                            .with_on_close(move |window, cx| on_close(window, cx))
                    })
                })
                .clone()
                .into_any_element(),
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("drawer-scrim")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.active_drawer = None;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(theme::TITLEBAR_H))
                        .right_0()
                        .bottom_0()
                        .child(drawer),
                )
                .into_any_element(),
        )
    }

    /// The global-search results popup under the titlebar field.
    fn render_search_popup(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let term = self.global_search.read(cx).value().to_string();
        if term.trim().is_empty() {
            return None;
        }
        // the field is centered at 420px; the popup hangs from the same x
        let left = ((window.viewport_size().width.as_f32() - 420.) / 2.).max(0.);
        let hits = self.store.read(cx).global_search.clone();
        if hits.is_empty() {
            // the popup shows a "No matches." footer while focused
            return Some(
                div()
                    .id("search-popup")
                    .debug_selector(|| "search-popup".into())
                    .absolute()
                    .top(px(40.))
                    .left(px(left))
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
                .debug_selector(|| "search-popup".into())
                .absolute()
                .top(px(40.))
                .left(px(left))
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
                .items_center()
                .justify_center()
                .p_4()
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
    tooltip: &'static str,
    show_tooltip: bool,
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
        .when(show_tooltip, |d| d.tooltip(crate::ui::tooltip_text(tooltip)))
        .child(icon(icon_path, px(theme::icon_medium()), theme::TEXT_MUTED))
}

/// An empty stretch of titlebar that moves the window when dragged.
fn titlebar_drag(id: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_1()
        .h_full()
        // Windows drags via the hit-test path (this hitbox answers HTCAPTION);
        // start_window_move is a no-op there. Linux keeps the explicit move.
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| {
            window.start_window_move();
        })
}

/// Both titlebar side columns share this width so the centered search field
/// sits at the window's true center. It fits the wider side on either
/// platform: the app name + notes + summary group on the left, and the tool
/// pill + SnapIt + window controls on the right (Windows only for SnapIt).
const TITLEBAR_SIDE_W: f32 = 252.0;

impl MainWindow {
    fn render_titlebar(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        // the two tool buttons share one inset pill; the window controls
        // stay flat on the titlebar so the two groups read differently
        let utility_group = div()
            .flex()
            .items_center()
            .gap_0p5()
            .rounded_md()
            .bg(theme::POPUP)
            .p_0p5();
        // fixed side column: matching the brand column's width keeps the
        // centered search truly centered
        let right_group = div()
            .debug_selector(|| "titlebar-right".into())
            .w(px(TITLEBAR_SIDE_W))
            .flex()
            .items_center()
            .justify_end()
            .child(
                utility_group
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
                            // the menu itself explains the tools; a tooltip
                            // hanging over it would just be noise
                            .when(show_tooltips && !self.tools_menu_open, |d| {
                                d.tooltip(crate::ui::tooltip_text("Tools — formatters, package manager, clipboard password, notes"))
                            })
                            .child(icon(
                                "icons/icon-tools.svg",
                                px(theme::icon_medium()),
                                if self.tools_menu_open { theme::ACCENT } else { theme::TEXT_MUTED },
                            ))
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
                            .when(show_tooltips, |d| {
                                d.tooltip(crate::ui::tooltip_text(if on_settings {
                                    "Back to repositories"
                                } else {
                                    "Settings"
                                }))
                            })
                            .child(icon(
                                "icons/icon-cog.svg",
                                px(theme::icon_medium()),
                                if on_settings { theme::ACCENT } else { theme::TEXT_MUTED },
                            ))
                    }),
            )
            .child(div().mx_1().h(px(16.)).w(px(1.)).bg(theme::HAIRLINE))
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
            .child(window_button(
                "minimize-window",
                "icons/icon-window-minimize.svg",
                theme::HOVER,
                "Minimize",
                show_tooltips,
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
                if window.is_maximized() { "Restore" } else { "Maximize" },
                show_tooltips,
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
                "Close",
                show_tooltips,
                |_, window, _| window.remove_window(),
            ));
        div()
            .h(px(theme::TITLEBAR_H))
            .flex()
            .items_center()
            .px_2()
            .gap_1()
            .bg(theme::STRONG_BACKGROUND)
            .border_b_1()
            .border_color(theme::BORDER_LIGHT)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                gpui_kit::base::GlobalState::suppress_text_selection(cx);
            })
            .child(
                // fixed side column: matching the right action group's width
                // keeps the centered search truly centered; the notes toggle
                // and the summary pill ride next to the app name
                div()
                    .debug_selector(|| "titlebar-brand".into())
                    .w(px(TITLEBAR_SIDE_W))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(28.))
                            .child(icon(
                                "icons/icon-logo-mark.svg",
                                px(theme::icon_large()),
                                theme::ACCENT,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_title()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child("Dev Tools"),
                    )
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
                            .when(show_tooltips, |d| {
                                d.tooltip(crate::ui::tooltip_text("Notes — markdown notes for the selected repo"))
                            })
                            .child(icon(
                                "icons/icon-note.svg",
                                px(theme::icon_medium()),
                                if self.notes_window.is_some() { theme::ACCENT } else { theme::TEXT_MUTED },
                            ))
                    })
                    .child(summary_button(&self.store, self.store.read(cx).summary_busy)),
            )
            // equal drag spacers on both sides of the search field
            .child(titlebar_drag("titlebar-drag-left"))
            .child(
                div()
                    .w(px(420.))
                    .min_w(px(160.))
                    .flex_shrink_1()
                    .child(
                        Input::new(&self.global_search)
                            .small()
                            .cleanable(true)
                            .prefix(icon(
                                "icons/icon-search.svg",
                                px(theme::icon_small()),
                                theme::TEXT_FAINT,
                            ))
                            .suffix(
                                div()
                                    .flex_shrink_0()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme::HAIRLINE)
                                    .px_1()
                                    .py_0p5()
                                    .text_size(px(theme::font_small() - 3.))
                                    .text_color(theme::TEXT_FAINT)
                                    .child("Ctrl F"),
                            ),
                    ),
            )
            .child(titlebar_drag("titlebar-drag-right"))
            .child(right_group)
            .into_any_element()
    }

    fn render_body(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.page {
            Page::Repos => {
                // The Add-repositories / Clone dialogs are page-level overlays.
                // While one is open the workspace bar would paint over the
                // dialog's footer (the scan-result card is taller than the
                // page slice above the bar), so give the page the full body.
                let dialog_open = self.repos_page.read(cx).dialog.is_some();
                let mut body = div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(div().flex_1().min_h_0().child(self.repos_page.clone()));
                if !dialog_open {
                    body = body.child(self.bottom_bar.clone());
                }
                body.into_any_element()
            }
            Page::Settings => div()
                .flex_1()
                .min_h_0()
                .child(self.settings_page.clone())
                .into_any_element(),
        }
    }
}
