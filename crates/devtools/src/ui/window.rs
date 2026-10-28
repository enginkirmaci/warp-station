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
                    let kind = crate::store::NoticeKind::for_copy_message(message);
                    this.store.update(cx, |s, cx| {
                        s.notify(kind, message.to_string());
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
        if let Some(popup) = self.render_search_popup(window, cx) {
            root = root.child(popup);
        }
        if let Some(drawer) = self.render_drawer_overlay(window, cx) {
            root = root.child(drawer);
        }
        if let Some(menu) = self.render_tools_menu(cx) {
            root = root.child(menu);
        }
        if let Some(toasts) = self.render_toasts(cx) {
            root = root.child(toasts);
        }
        root
    }
}

impl MainWindow {
    /// The toast stack, bottom-right: severity glyph + full text + a close
    /// button. Painted last so it rides above drawers and menus; each card
    /// occludes so a click dismisses instead of falling through to whatever
    /// sits underneath. The per-second tick expires them (errors live
    /// longer than confirmations).
    fn render_toasts(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let notices = self.store.read(cx).notices.clone();
        if notices.is_empty() {
            return None;
        }
        let mut stack = div()
            .id("toast-stack")
            .debug_selector(|| "toast-stack".into())
            .absolute()
            .bottom_3()
            .right_3()
            .flex()
            .flex_col()
            .items_end()
            .gap_2();
        for notice in notices {
            let (glyph, color) = match notice.kind {
                crate::store::NoticeKind::Success => ("icons/icon-check.svg", theme::SUCCESS_BRIGHT),
                crate::store::NoticeKind::Info => ("icons/icon-info.svg", theme::BLUE_BRIGHT),
                crate::store::NoticeKind::Error => ("icons/icon-alert.svg", theme::DANGER),
            };
            let id = notice.id;
            let store = self.store.clone();
            stack = stack.child(
                div()
                    .id(("toast", id))
                    .debug_selector(move || format!("toast-{id}"))
                    .occlude()
                    .max_w(px(420.))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(if notice.kind == crate::store::NoticeKind::Error {
                        theme::tint(theme::DANGER)
                    } else {
                        gpui::Hsla::from(theme::HAIRLINE)
                    })
                    .bg(theme::POPUP)
                    .shadow_lg()
                    .p_2p5()
                    .child(icon(glyph, px(theme::icon_small()), color))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(theme::font_small()))
                            .text_color(if notice.kind == crate::store::NoticeKind::Error {
                                theme::TEXT
                            } else {
                                theme::TEXT_DIM
                            })
                            .child(notice.text.clone()),
                    )
                    .child(
                        div()
                            .id(("toast-close", id))
                            .debug_selector(move || format!("toast-close-{id}"))
                            .cursor_pointer()
                            .flex_shrink_0()
                            .rounded_md()
                            .p_0p5()
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                store.update(cx, |s, cx| s.dismiss_notice(id, cx));
                            })
                            .child(icon(
                                "icons/icon-close.svg",
                                px(theme::icon_small() - 4.),
                                theme::TEXT_FAINT,
                            )),
                    ),
            );
        }
        Some(stack.into_any_element())
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
        let (hits, pending, code_hits, code_searching) = {
            let store = self.store.read(cx);
            (
                store.global_search.clone(),
                store.global_search_pending,
                store.code_search_hits.clone(),
                store.code_searching,
            )
        };
        if hits.is_empty() && code_hits.is_empty() {
            // the popup shows a footer while focused; a hint beats a stale
            // "No matches." during the debounced background search — either
            // half of it (repo/note or the ripgrep sweep)
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
                    .child(if pending || code_searching { "Searching…" } else { "No matches." })
                    .into_any_element(),
            );
        }
        // one flat row budget shared by every kind: repo/note rows first,
        // the ripgrep hits fill what is left
        const POPUP_ROWS: usize = 12;
        let mut rows = div().flex().flex_col().gap_0p5();
        let mut shown = 0;
        for (index, hit) in hits.iter().enumerate() {
            if shown >= POPUP_ROWS {
                break;
            }
            shown += 1;
            let (title, detail, kind_color) = match hit {
                crate::store::GlobalSearchHit::Repo { name, folder } => {
                    (name.clone(), folder.clone(), theme::ACCENT)
                }
                crate::store::GlobalSearchHit::Note { title, excerpt, .. } => {
                    (title.clone(), excerpt.clone(), theme::BLUE_BRIGHT)
                }
            };
            let hit_for_click = hit.clone();
            let index_for_probe = index;
            rows = rows.child(
                div()
                    .id(("search-hit", index as u64))
                    .debug_selector(move || format!("search-hit-{index_for_probe}"))
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
        for (index, hit) in code_hits.iter().enumerate() {
            if shown >= POPUP_ROWS {
                break;
            }
            shown += 1;
            let hit_for_click = hit.clone();
            let file_name = hit
                .abs_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| hit.rel_path.clone());
            let dir = std::path::Path::new(&hit.rel_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            // the excerpt splits around the match so the hit itself rides the
            // accent; the offsets are byte offsets into the excerpt
            let safe_end = hit.match_end.min(hit.excerpt.len());
            let safe_start = hit.match_start.min(safe_end);
            let (before, matched, after) = (
                hit.excerpt[..safe_start].to_string(),
                hit.excerpt[safe_start..safe_end].to_string(),
                hit.excerpt[safe_end..].to_string(),
            );
            rows = rows.child(
                div()
                    .id(("search-code-hit", index as u64))
                    .debug_selector(move || format!("search-code-hit-{index}"))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .overflow_hidden()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_code_hit(hit_for_click.clone(), window, cx);
                    }))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(theme::font_small() - 1.))
                            .text_color(theme::PURPLE_BRIGHT)
                            .child("code"),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(
                                        div()
                                            .text_size(px(theme::font_small()))
                                            .text_color(theme::TEXT_DIM)
                                            .child(file_name),
                                    )
                                    .when(!dir.is_empty(), |title| {
                                        title.child(
                                            div()
                                                .text_size(px(theme::font_small() - 2.))
                                                .text_color(theme::TEXT_FAINT)
                                                .child(dir),
                                        )
                                    }),
                            )
                            // inline styling = one row of separately styled
                            // text segments; nowrap keeps the three pieces on
                            // the excerpt's single line
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(theme::font_small() - 2.))
                                    .child(div().text_color(theme::TEXT_FAINT).child(before))
                                    .child(div().text_color(theme::ACCENT).child(matched))
                                    .child(div().text_color(theme::TEXT_FAINT).child(after)),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .ml_auto()
                            .text_size(px(theme::font_small() - 2.))
                            .text_color(theme::TEXT_FAINT)
                            .child(format!(":{}", hit.line)),
                    ),
            );
        }
        let mut popup = div()
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
            .child(rows);
        if code_searching {
            // the sweep trails the repo/note half; one quiet line, no spinner
            popup = popup.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::font_small() - 2.))
                    .text_color(theme::TEXT_FAINT)
                    .child("searching code…"),
            );
        }
        Some(popup.into_any_element())
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

    /// A code hit opens the file at its line in VS Code (the configured
    /// profile; fallback: the system default editor), then the popup clears
    /// like every other activation. The launch is fire-and-forget — spawn
    /// only, so a slow editor never holds the click.
    fn activate_code_hit(
        &mut self,
        hit: devtools_core::code_search::CodeHit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = self.store.read(cx).settings();
        let plan = devtools_core::terminal::plan_code_open(
            &hit.abs_path.to_string_lossy(),
            hit.line,
            &settings.repos.vscode_executable,
            &settings.repos.vscode_profile,
        );
        if let Err(err) = devtools_core::terminal::open_code_plan(&plan) {
            self.store.update(cx, |s, _| s.notify_error(format!("open failed: {err}")));
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
                cx.new(|cx| {
                    let mut page = NotesPage::new(store.clone(), window, cx);
                    // E2E hook: DEVTOOLS_NOTE=<relative-path> opens that note
                    // right away (headless/live capture without clicks)
                    if let Ok(rel) = std::env::var("DEVTOOLS_NOTE") {
                        let root = devtools_core::notes::resolve_store_root(
                            &store.read(cx).settings().general.notes_store_path,
                        );
                        let path = root.join(rel.trim_start_matches(['/', '\\']));
                        for parent in path.ancestors().skip(1) {
                            page.expand_tree_path(parent.to_path_buf());
                        }
                        page.open_note(path, window, cx);
                    }
                    page
                })
            },
        ) {
            self.notes_window = Some(handle);
        }
        cx.notify();
    }
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
        .debug_selector(move || id.into())
        .cursor_pointer()
        .flex_shrink_0()
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

/// A titlebar status chip: an 8px state dot + label, transparent until
/// hovered, clicking toggles the tool. The dot colors follow the .NET
/// titlebar: green = running/watching, amber = NuGet idle, red = SnapIt
/// stopped.
fn status_chip(
    id: &'static str,
    label: &'static str,
    dot_color: gpui::Rgba,
    tooltip: &'static str,
    show_tooltip: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .cursor_pointer()
        .flex_shrink_0()
        .rounded_md()
        .h(px(26.))
        .px_2()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .hover(|s| s.bg(theme::HOVER))
        .when(show_tooltip, |d| d.tooltip(crate::ui::tooltip_text(tooltip)))
        .on_click(on_click)
        .child(div().size(px(8.)).rounded_full().bg(dot_color))
        .child(
            div()
                .text_size(px(theme::font_small() - 1.))
                .text_color(theme::TEXT_MUTED)
                .child(label),
        )
        .into_any_element()
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
/// sits at the window's true center. The right side is the wider one — status
/// chips + tool pill + window controls — so the width is computed from what
/// the settings actually enable; the old fixed guess sat below the real
/// content and the flex row crushed the window controls into a fraction of
/// their pitch.
fn titlebar_side_w(show_nuget: bool, show_warp: bool, show_windows_chips: bool) -> f32 {
    // status chip: px_2 padding, 8px dot, gap, ~6.5px per label character
    let chip = |label: &str| 30.0 + label.chars().count() as f32 * 6.5;
    let mut widths = Vec::new();
    if show_nuget {
        widths.push(chip("NuGet"));
    }
    if show_windows_chips {
        widths.push(chip("SnapIt"));
        widths.push(chip("Awake"));
    }
    if show_warp {
        widths.push(chip("warp"));
    }
    if !widths.is_empty() {
        widths.push(9.0); // the hairline divider: w 1 + mx_1
    }
    widths.push(58.0); // the tool pill: p_0p5 + two 26px buttons + gap_0p5
    widths.push(9.0);
    widths.push(78.0); // minimize / maximize / close at 26px each
    let gaps = (widths.len() - 1) as f32 * 4.0; // the row's gap_1
    (widths.iter().sum::<f32>() + gaps + 10.0).max(252.0)
}

impl MainWindow {
    fn render_titlebar(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let settings = self.store.read(cx).settings();
        let show_tooltips = settings.general.show_tooltips;
        let nuget_enabled = settings.nuget_local.enable_nuget;
        let warp_enabled = settings.warp_agent.enable_warp_agent;
        let nuget_watching = self.store.read(cx).nuget.watching;
        // the status chips fold away first when the window cannot fit the
        // full titlebar — the tool pill and the window controls never do
        let mut show_nuget = nuget_enabled;
        let mut show_warp = warp_enabled;
        let mut show_windows_chips = cfg!(windows);
        let viewport_w = window.viewport_size().width.as_f32();
        let full_side = titlebar_side_w(show_nuget, show_warp, show_windows_chips);
        if full_side * 2.0 + 160.0 /* search min width */ + 64.0 /* gutters, gaps, slack */
            > viewport_w
        {
            show_nuget = false;
            show_warp = false;
            show_windows_chips = false;
        }
        let side_w = titlebar_side_w(show_nuget, show_warp, show_windows_chips);
        // the two tool buttons share one inset pill; the window controls
        // stay flat on the titlebar so the two groups read differently
        let utility_group = div()
            .flex()
            .items_center()
            .gap_0p5()
            .flex_shrink_0()
            .rounded_md()
            .bg(theme::POPUP)
            .p_0p5();
        // fixed side column: matching the brand column's width keeps the
        // centered search truly centered
        let right_group = div()
            .debug_selector(|| "titlebar-right".into())
            .w(px(side_w))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_end()
            // status chips mirror the .NET titlebar: an 8px state dot + label,
            // click toggles the tool. NuGet amber when idle, green while the
            // watch runs; SnapIt red when stopped, green when running — and
            // Windows-only, like the tool itself. Keep awake is Windows-only
            // too: green while it holds the machine awake, muted when off.
            .when(show_nuget, |d| {
                let store = self.store.clone();
                d.child(status_chip(
                    "nuget-status",
                    "NuGet",
                    if nuget_watching {
                        theme::SUCCESS_BRIGHT
                    } else {
                        theme::AMBER_BRIGHT
                    },
                    if nuget_watching {
                        "NuGet watch: running — click to stop"
                    } else {
                        "NuGet watch: idle — click to start"
                    },
                    show_tooltips,
                    move |_, _, cx| {
                        store.update(cx, |s, _| {
                            if s.nuget.watching {
                                s.stop_nuget();
                            } else {
                                s.start_nuget();
                            }
                        });
                        cx.refresh_windows();
                    },
                ))
            })
            .when(show_windows_chips, |d| {
                let running = devtools_core::snapit::is_running();
                let store = self.store.clone();
                d.child(status_chip(
                    "snapit-status",
                    "SnapIt",
                    if running {
                        theme::SUCCESS_BRIGHT
                    } else {
                        theme::DANGER
                    },
                    if running {
                        "SnapIt: running — click to stop"
                    } else {
                        "SnapIt: stopped — click to start"
                    },
                    show_tooltips,
                    move |_, _, cx| {
                        let exe = store.read(cx).settings().snap_it.executable;
                        let result = if devtools_core::snapit::is_running() {
                            devtools_core::snapit::stop()
                        } else {
                            devtools_core::snapit::start(&exe)
                        };
                        if let Err(err) = result {
                            store.update(cx, |s, cx| {
                                s.notify_error(err);
                                cx.notify();
                            });
                        }
                        cx.refresh_windows();
                    },
                ))
            })
            // warp-agent door: green while a running instance answers, muted
            // when away; click opens (or launches) it on the General workspace
            .when(show_warp, |d| {
                let connected = self.store.read(cx).warp_connected;
                let store = self.store.clone();
                d.child(status_chip(
                    "warp-status",
                    "warp",
                    if connected { theme::SUCCESS_BRIGHT } else { theme::TEXT_MUTED },
                    if connected {
                        "warp-agent: connected — click to open it on General"
                    } else {
                        "warp-agent: not running — click to launch it"
                    },
                    show_tooltips,
                    move |_, _, cx| {
                        store.update(cx, |s, cx| s.open_in_warp("general".into(), cx));
                    },
                ))
            })
            .when(show_windows_chips, |d| {
                let awake = devtools_core::keepawake::is_enabled();
                let store = self.store.clone();
                d.child(status_chip(
                    "keepawake-status",
                    "Awake",
                    if awake {
                        theme::SUCCESS_BRIGHT
                    } else {
                        theme::TEXT_MUTED
                    },
                    if awake {
                        "Keep awake: on — system and display stay on, click to allow sleep"
                    } else {
                        "Keep awake: off — click to prevent sleep and display off"
                    },
                    show_tooltips,
                    move |_, _, cx| {
                        let enable = !devtools_core::keepawake::is_enabled();
                        match devtools_core::keepawake::set_enabled(enable) {
                            Ok(()) => store.update(cx, |s, cx| {
                                s.update_settings(|settings| {
                                    settings.keep_awake.enabled = enable;
                                });
                                cx.notify();
                            }),
                            Err(err) => store.update(cx, |s, cx| {
                                s.notify_error(err);
                                cx.notify();
                            }),
                        }
                        cx.refresh_windows();
                    },
                ))
            })
            // the chips group separates from the tool pill only when a chip
            // actually rendered
            .when(show_nuget || show_warp || show_windows_chips, |d| {
                d.child(div().mx_1().h(px(16.)).w(px(1.)).bg(theme::HAIRLINE))
            })
            .child(
                utility_group
                    .child({
                        div()
                            .id("tools-toggle")
                            .debug_selector(|| "tools-toggle".into())
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
                            .debug_selector(|| "settings-toggle".into())
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
                    .w(px(side_w))
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
                    // hairline between the app name and the tools that follow
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
                            .when(show_tooltips, |d| {
                                d.tooltip(crate::ui::tooltip_text("Notes — markdown notes for the selected repo"))
                            })
                            .child(icon(
                                "icons/icon-note.svg",
                                px(theme::icon_medium()),
                                if self.notes_window.is_some() { theme::ACCENT } else { theme::TEXT_MUTED },
                            ))
                    })
                    .when(self.store.read(cx).wand_available(), |left| {
                        left.child(summary_button(&self.store, self.store.read(cx).summary_busy))
                    }),
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
                                    .mx_1()
                                    .px_1p5()
                                    .py_0p5()
                                    .text_size(px(theme::font_small() - 3.))
                                    .text_color(theme::TEXT_FAINT)
                                    .child("Ctrl + F"),
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
