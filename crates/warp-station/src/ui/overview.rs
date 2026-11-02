//! The Overview tab and the detail head above every tab: the repository's
//! identity, its one primary launch and the repo menu up top, then what needs a
//! person, where to open it, its status and what GitHub says about it, as cards
//! in two columns that stack when the pane gets narrow.

use crate::net::GitAction;
use crate::store::{Store, WorkspaceTab};
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::launchers::{self, Launcher};
use crate::ui::repos::ReposPage;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Entity, FontWeight, MouseButton, Rgba, SharedString, div, px,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};
use warp_station_core::rel_time;
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::AppSettings;
use warp_station_core::terminal;
use warpui_kit_core::motion;

/// The open "⋯" menu: the repo it opened on, because a moved selection closes
/// it and it must never act on another one, and whether Remove took its first
/// click.
pub struct RepoMenu {
    pub folder: String,
    pub remove_armed: bool,
}

/// What the detail head needs beyond the repo.
pub struct HeadState {
    /// The open tab holds no action of its own, so the launch is the primary.
    pub filled: bool,
    /// The repo already has a tab; the button goes to it.
    pub tabbed: bool,
    /// The repo is on screen as a tab, so there is nothing left to open.
    pub in_tab: bool,
    pub launch_menu_open: bool,
    /// Bumped by each press on the star, so only a press makes it jump.
    pub star_press: Option<u64>,
}

/// The Overview tab for the selected repo.
pub fn render(
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
) -> AnyElement {
    let body = div()
        .id("overview")
        .debug_selector(|| "overview".into())
        .size_full()
        .overflow_y_scroll();
    let Some(repo) = store.selected_entry() else {
        return body.child(empty_state()).into_any_element();
    };
    let folder = repo.folder_path.to_string_lossy().to_string();
    let settings = store.settings();
    let launchers = launchers::launchers(&settings, repo);
    let left = div()
        .flex_1()
        .min_w(px(320.))
        .flex()
        .flex_col()
        .gap_4()
        .child(files_section(store, repo, &settings, entities))
        .when(settings.repos.enable_git_hub, |column| {
            column.child(issues_section(store, repo, entities))
        })
        .when(settings.repos.enable_azure_dev_ops, |column| {
            column.children(azure_section(store, repo, entities))
        });
    let right = div()
        .flex_1()
        .min_w(px(320.))
        .flex()
        .flex_col()
        .gap_4()
        .child(status_section(repo, page, tag_draft, entities, &folder))
        .when(launchers.len() > 1, |column| {
            column.child(open_in_section(
                launchers.get(1..).unwrap_or_default(),
                &folder,
                entities,
            ))
        })
        .children(details_section(store));
    body.child(
        div()
            .max_w(px(warpui_kit_core::shell::GRID_WIDTH))
            .p(px(warpui_kit_core::shell::DETAIL_PADDING))
            .flex()
            .flex_wrap()
            .items_start()
            .gap_4()
            .child(left)
            .child(right),
    )
    .into_any_element()
}

pub(crate) fn empty_state() -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_MUTED)
        .child("No repositories yet")
        .into_any_element()
}

/// A square quiet button of the head.
fn head_icon(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .cursor_pointer()
        .flex_shrink_0()
        .size(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::LINE_STRONG)
        .bg(theme::PANEL)
        .flex()
        .items_center()
        .justify_center()
        .hover(|d| d.bg(theme::RAISED))
}

/// The head's top row: star, name, then the verbs. The open tab owns the one
/// filled action, so Open in VS Code only fills on Overview.
pub(crate) fn header(
    repo: &RepoEntry,
    launchers: &[Launcher],
    state: &HeadState,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    folder: &str,
) -> AnyElement {
    let favorite = repo.is_favorite();
    let star_press = state.star_press;
    let star_target = entities.clone();
    let star_folder = folder.to_string();
    let tab_target = entities.clone();
    let tab_folder = folder.to_string();
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .min_h(px(36.))
        .child(
            div()
                .id("insp-star")
                .debug_selector(|| "insp-star".into())
                .a11y_button(if favorite {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                })
                .cursor_pointer()
                .flex_shrink_0()
                .size(px(24.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .hover(|d| d.bg(theme::RAISED))
                .on_click(move |_, _, cx| {
                    star_target.update(cx, |s, cx| s.toggle_favorite(&star_folder, cx));
                })
                .child(match star_press.filter(|_| favorite) {
                    Some(press) => motion::pop(
                        SharedString::from(format!("insp-star-pop-{press}")),
                        icon(
                            "icons/icon-star-filled.svg",
                            px(theme::icon_medium()),
                            theme::ACCENT_INK,
                        ),
                    ),
                    None => icon(
                        if favorite {
                            "icons/icon-star-filled.svg"
                        } else {
                            "icons/icon-star.svg"
                        },
                        px(theme::icon_medium()),
                        if favorite {
                            theme::ACCENT_INK
                        } else {
                            theme::STAR_MUTED
                        },
                    )
                    .into_any_element(),
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_screen_title()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(repo.name.clone()),
        )
        .when(!state.in_tab, |row| {
            let label = if state.tabbed {
                "Go to tab"
            } else {
                "Open in tab"
            };
            row.child(
                div()
                    .id("detail-open-tab")
                    .debug_selector(|| "detail-open-tab".into())
                    .a11y_button(label)
                    .cursor_pointer()
                    .flex_shrink_0()
                    .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                    .px_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme::LINE_STRONG)
                    .bg(theme::PANEL)
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_control()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::TEXT)
                    .hover(|d| d.bg(theme::RAISED))
                    .on_click(move |_, _, cx| {
                        tab_target.update(cx, |s, cx| s.open_repo_tab(&tab_folder, cx));
                    })
                    .child(icon(
                        if state.tabbed {
                            "icons/icon-tab.svg"
                        } else {
                            "icons/icon-tab-plus.svg"
                        },
                        px(theme::icon_small()),
                        theme::TEXT_DIM,
                    ))
                    .child(label),
            )
        })
        .children(
            launchers
                .first()
                .map(|primary| primary_action(primary, state.filled, folder, entities)),
        )
        .when(launchers.len() > 1, |row| {
            row.child(launch_toggle(state.filled, page))
        })
        .child(
            head_icon("insp-more", "Repository menu")
                // keep the window's outside-click dismissal from seeing this
                // press: it must toggle, not close-then-reopen
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click({
                    let target = page.clone();
                    let path = folder.to_string();
                    move |_, _, cx| {
                        target.update(cx, |this, cx| this.toggle_repo_menu(&path, cx));
                    }
                })
                .child(icon(
                    "icons/icon-ellipsis-vertical.svg",
                    px(theme::icon_small()),
                    theme::TEXT_DIM,
                )),
        )
        .into_any_element()
}

/// The primary's split half: every other launcher.
fn launch_toggle(filled: bool, page: &Entity<ReposPage>) -> AnyElement {
    let target = page.clone();
    let (fill, hover, ink, edge) = if filled {
        (
            theme::ACCENT,
            theme::ACCENT_HOVER,
            theme::ON_ACCENT,
            theme::ACCENT,
        )
    } else {
        (
            theme::PANEL,
            theme::RAISED,
            theme::TEXT_DIM,
            theme::LINE_STRONG,
        )
    };
    div()
        .id("insp-launch-menu")
        .debug_selector(|| "insp-launch-menu".into())
        .a11y_button("Other apps")
        .cursor_pointer()
        .flex_shrink_0()
        .ml(px(-10.))
        .w(px(30.))
        .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .rounded_r(px(6.))
        .border_1()
        .border_l_0()
        .border_color(edge)
        .bg(fill)
        .flex()
        .items_center()
        .justify_center()
        .hover(move |d| d.bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            target.update(cx, |this, cx| this.toggle_launch_menu(cx));
        })
        .child(icon(
            "icons/icon-chevron-down.svg",
            px(theme::icon_tiny()),
            ink,
        ))
        .into_any_element()
}

/// The split button's menu: every launcher with its key.
pub(crate) fn launch_menu(
    launchers: &[Launcher],
    folder: &str,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
) -> AnyElement {
    let mut menu = menu_panel("insp-launch-list");
    for launcher in launchers {
        let target = entities.clone();
        let close = page.clone();
        let path = folder.to_string();
        let launch = launcher.launch.clone();
        menu = menu.child(
            div()
                .id(SharedString::from(format!("launch-{}", launcher.label)))
                .a11y_menu_item(launcher.label)
                .cursor_pointer()
                .h(px(32.))
                .px_2()
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .text_color(theme::TEXT)
                .hover(|d| d.bg(theme::LINE_STRONG))
                .on_click(move |_, _, cx| {
                    launch.run(&path, &target, cx);
                    close.update(cx, |this, cx| this.close_popups(cx));
                })
                .child(icon(
                    launcher.icon,
                    px(theme::icon_small()),
                    theme::TEXT_DIM,
                ))
                .child(div().flex_1().child(launcher.label))
                .children(
                    launcher
                        .key
                        .map(|key| keycap(key, theme::TEXT_DIM, theme::KEY_BORDER)),
                ),
        );
    }
    menu.into_any_element()
}

/// A menu's panel, hanging from the detail head's top right corner.
fn menu_panel(id: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .absolute()
        .top(px(56.))
        .right(px(warpui_kit_core::shell::DETAIL_PADDING))
        .w(px(300.))
        .p(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::LINE_STRONG)
        .bg(theme::OVERLAY)
        .shadow_lg()
        // presses inside must not reach the pane under it, or the window's
        // outside-click dismissal before the row's own click lands
        .occlude()
        .flex()
        .flex_col()
        .gap(px(2.))
}

type MenuAction = Box<dyn Fn(&mut App)>;

/// The header's "⋯" menu: the repo verbs, in a popup hanging from the header.
pub(crate) fn popup_menu(
    repo: &RepoEntry,
    state: &RepoMenu,
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    folder: &str,
) -> AnyElement {
    let archived = repo.is_archived();
    let mut menu = menu_panel("insp-repo-menu");

    // every row closes the menu behind its action
    let item = |id: &'static str, label: &'static str, action: MenuAction| {
        let close = page.clone();
        menu_row(id, label).on_click(move |_, _, cx| {
            action(cx);
            close.update(cx, |this, cx| this.close_repo_menu(cx));
        })
    };

    if let Some(url) = store.selected_github_url() {
        let target = entities.clone();
        let page_url = url.clone();
        menu = menu.child(item(
            "insp-menu-github",
            "Open in GitHub",
            Box::new(move |cx| {
                if let Err(err) = terminal::open_url(&page_url) {
                    target.update(cx, |s, _| s.notify_error(err));
                }
            }),
        ));
        let target = entities.clone();
        menu = menu.child(item(
            "insp-menu-copy-url",
            "Copy GitHub URL",
            Box::new(move |cx| copy_with_toast(url.clone(), "GitHub URL copied", &target, cx)),
        ));
    }
    {
        let target = entities.clone();
        let path = folder.to_string();
        menu = menu.child(item(
            "insp-menu-copy-path",
            "Copy folder path",
            Box::new(move |cx| copy_with_toast(path.clone(), "Folder path copied", &target, cx)),
        ));
    }
    {
        let target = entities.clone();
        menu = menu.child(item(
            "insp-menu-refresh",
            "Refresh repository",
            Box::new(move |cx| target.update(cx, |s, cx| s.refresh_panel(cx))),
        ));
    }
    {
        let target = entities.clone();
        let path = folder.to_string();
        let name = repo.name.clone();
        menu = menu.child(item(
            "insp-menu-archive",
            if archived { "Restore" } else { "Archive" },
            Box::new(move |cx| {
                target.update(cx, |s, cx| {
                    s.set_repo_archived(&path, !archived, cx);
                    s.notify_success(if archived {
                        format!("Restored {name}")
                    } else {
                        format!("Archived {name}")
                    });
                    cx.notify();
                });
            }),
        ));
    }
    menu.child(div().h(px(1.)).mx(px(4.)).my(px(2.)).bg(theme::HAIRLINE))
        .child(remove_row(state.remove_armed, entities, page))
        .into_any_element()
}

fn menu_row(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .cursor_pointer()
        .rounded(px(6.))
        .px_2()
        .h(px(32.))
        .flex()
        .items_center()
        .text_size(px(theme::font_control()))
        .text_color(theme::TEXT)
        .hover(|d| d.bg(theme::LINE_STRONG))
        .child(label)
}

/// Destructive, so it takes two clicks: the first arms the row, the second
/// removes. Closing the menu disarms it, whatever closed it.
fn remove_row(armed: bool, entities: &Entity<Store>, page: &Entity<ReposPage>) -> AnyElement {
    let target = entities.clone();
    let menu = page.clone();
    div()
        .id("insp-menu-remove")
        .debug_selector(|| "insp-menu-remove".into())
        .a11y_button(if armed {
            "Confirm remove from repositories"
        } else {
            "Remove from repositories"
        })
        .cursor_pointer()
        .rounded(px(6.))
        .px_2()
        .py_1p5()
        .flex()
        .flex_col()
        .gap_0p5()
        .text_size(px(theme::font_control()))
        .text_color(if armed {
            theme::DANGER
        } else {
            theme::TEXT_DIM
        })
        .when(armed, |row| row.bg(theme::tint(theme::DANGER)))
        .hover(|d| d.bg(theme::tint(theme::DANGER)).text_color(theme::DANGER))
        .on_click(move |_, _, cx| {
            if armed {
                target.update(cx, |s, cx| s.remove_from_repositories(cx));
                menu.update(cx, |this, cx| this.close_repo_menu(cx));
            } else {
                menu.update(cx, |this, cx| this.arm_repo_remove(cx));
            }
        })
        .child(if armed {
            "Remove from repositories?"
        } else {
            "Remove from repositories"
        })
        .when(armed, |row| {
            row.child(
                div()
                    .text_size(px(theme::font_mono_meta()))
                    .child("Click again to confirm"),
            )
        })
        .into_any_element()
}

/// Copies `text` and confirms it with a toast.
fn copy_with_toast(text: String, toast: &'static str, store: &Entity<Store>, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    store.update(cx, |s, cx| {
        s.notify_success(toast);
        cx.notify();
    });
}

/// `~/developments/foo` rather than the absolute path: the home prefix carries
/// no information.
pub(crate) fn display_path(repo: &RepoEntry) -> String {
    let path = repo.folder_path.to_string_lossy().to_string();
    for key in ["HOME", "USERPROFILE"] {
        let Some(home) = std::env::var_os(key) else {
            continue;
        };
        let home = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(home.as_ref()) {
            return format!("~{rest}");
        }
    }
    path
}

fn tag_row(
    repo: &RepoEntry,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
    entities: &Entity<Store>,
    folder: &str,
) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap(px(6.))
        .min_h(px(24.));
    for tag in repo.tags.iter().filter(|tag| !is_system_tag(tag)) {
        row = row.child(tag_chip(tag, entities, folder));
    }
    if let Some(input) = tag_draft {
        row = row.child(
            div()
                .w(px(120.))
                .child(Input::new(input).with_size(theme::field_size())),
        );
    } else {
        let target = page.clone();
        row = row.child(
            div()
                .id("insp-add-tag")
                .debug_selector(|| "insp-add-tag".into())
                .a11y_button("Add tag")
                .cursor_pointer()
                .h(px(24.))
                .flex()
                .items_center()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::ACCENT_INK)
                .on_click(move |_, _, cx| {
                    target.update(cx, |this, cx| this.begin_tag_edit(cx));
                })
                .child("Add tag"),
        );
    }
    row.into_any_element()
}

/// A user tag. The × stays quiet until the chip is hovered, but its target
/// runs the full chip height.
fn tag_chip(tag: &str, entities: &Entity<Store>, folder: &str) -> AnyElement {
    let target = entities.clone();
    let path = folder.to_string();
    let name = tag.to_string();
    let selector = format!("insp-tag-remove-{tag}");
    div()
        .group("insp-tag")
        .h(px(24.))
        .pl(px(8.))
        .flex()
        .items_center()
        .rounded(px(12.))
        .bg(theme::RAISED)
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_DIM)
        .child(SharedString::from(tag.to_string()))
        .child(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .a11y_button(format!("Remove tag {tag}"))
                .cursor_pointer()
                .w(px(18.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme::TEXT_MUTED)
                .opacity(0.45)
                .group_hover("insp-tag", |d| d.opacity(1.))
                .hover(|d| d.text_color(theme::TEXT))
                .on_click(move |_, _, cx| {
                    target.update(cx, |s, cx| s.toggle_repo_tag(&path, &name, cx));
                })
                .child("×"),
        )
        .into_any_element()
}

/// `favorites` and `archived` are derived flags, not user tags — showing them
/// as removable chips would invite removing them.
fn is_system_tag(tag: &str) -> bool {
    tag.eq_ignore_ascii_case(warp_station_core::repo_store::FAVORITES_TAG)
        || tag.eq_ignore_ascii_case(warp_station_core::repo_store::ARCHIVED_TAG)
}

/// One label and value of a card.
fn fact(label: &'static str, value: impl IntoElement) -> gpui::Div {
    div()
        .mx_4()
        .min_h(px(34.))
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .w(px(112.))
                .flex_shrink_0()
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_DIM)
                .child(label),
        )
        .child(div().flex_1().min_w_0().child(value))
}

/// Branch, how far it is from the remote, the two sync verbs and the tags.
fn status_section(
    repo: &RepoEntry,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
    entities: &Entity<Store>,
    folder: &str,
) -> AnyElement {
    let branch = repo
        .git_branch_name
        .clone()
        .unwrap_or_else(|| "detached".into());
    let ahead = repo.git_to_push_count;
    let behind = repo.git_to_pull_count;
    let fetched = repo
        .git_last_fetch_at
        .map(rel_time::format)
        .unwrap_or_else(|| "never fetched".into());

    let branch_line = div()
        .id("insp-branch")
        .debug_selector(|| "insp-branch".into())
        .flex()
        .items_center()
        .gap(px(5.))
        .font_family(theme::code_font())
        .text_size(px(theme::font_mono_meta()))
        .text_color(theme::TEXT_DIM)
        .when_some(repo.git_branch_name.clone(), |line, name| {
            let target = entities.clone();
            line.a11y_button(format!("Copy branch name {name}"))
                .cursor_pointer()
                .hover(|d| d.text_color(theme::TEXT))
                .on_click(move |_, _, cx| {
                    copy_with_toast(name.clone(), "Branch name copied", &target, cx);
                })
        })
        .child(icon(
            "icons/icon-source-branch.svg",
            px(theme::icon_tiny()),
            theme::BRANCH,
        ))
        .child(SharedString::from(branch));

    let sync_state = |path: &'static str, count: u32, word: &'static str, tint: Rgba| {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(px(theme::font_meta()))
            .font_weight(FontWeight::MEDIUM)
            .text_color(tint)
            .child(icon(path, px(theme::icon_tiny()), tint))
            .child(format!("{count} {word}"))
    };
    let sync = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .when(behind > 0, |row| {
            row.child(sync_state(
                "icons/icon-arrow-down.svg",
                behind,
                "to pull",
                theme::BEHIND,
            ))
        })
        .when(ahead > 0, |row| {
            row.child(sync_state(
                "icons/icon-arrow-up.svg",
                ahead,
                "to push",
                theme::AHEAD,
            ))
        })
        .when(ahead == 0 && behind == 0, |row| {
            row.child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::SUCCESS_BRIGHT)
                    .child("in sync"),
            )
        })
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_MUTED)
                .child(format!("fetched {fetched}")),
        );
    let verbs = div()
        .mx_4()
        .pt_1()
        .pb_2()
        .flex()
        .items_center()
        .gap_2()
        .child(small_button("insp-pull", "Pull".into(), {
            let target = entities.clone();
            move |_, _, cx| {
                target.update(cx, |s, cx| s.run_git(GitAction::Pull, cx));
            }
        }))
        .child(small_button("insp-push", "Push".into(), {
            let target = entities.clone();
            move |_, _, cx| {
                target.update(cx, |s, cx| s.run_git(GitAction::Push, cx));
            }
        }));

    card(section_title("Status".to_string()))
        .child(fact("Branch", branch_line))
        .child(fact("Sync", sync))
        .child(verbs)
        .child(fact(
            "Tags",
            tag_row(repo, page, tag_draft, entities, folder),
        ))
        .into_any_element()
}

fn small_button(
    id: &'static str,
    label: String,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label.clone())
        .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .px_3()
        .rounded(px(6.))
        .border_1()
        .border_color(theme::LINE_STRONG)
        .bg(theme::PANEL)
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::TEXT)
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|el| el.bg(theme::RAISED))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

/// The first launcher, the default editor unless the settings turned it off:
/// the head's split button, filled when the open tab has no action of its own.
fn primary_action(
    launcher: &Launcher,
    filled: bool,
    folder: &str,
    entities: &Entity<Store>,
) -> AnyElement {
    let target = entities.clone();
    let path = folder.to_string();
    let launch = launcher.launch.clone();
    let (fill, hover, ink, edge) = if filled {
        (
            theme::ACCENT,
            theme::ACCENT_HOVER,
            theme::ON_ACCENT,
            theme::ACCENT,
        )
    } else {
        (theme::PANEL, theme::RAISED, theme::TEXT, theme::LINE_STRONG)
    };
    div()
        .id("insp-primary")
        .debug_selector(|| "insp-primary".into())
        .a11y_button(launcher.label)
        .cursor_pointer()
        .flex_shrink_0()
        .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .px_3()
        .rounded_l(px(6.))
        .border_1()
        .border_color(edge)
        .bg(fill)
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(theme::font_control()))
        .font_weight(if filled {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .text_color(ink)
        .hover(move |el| el.bg(hover))
        .on_click(move |_, _, cx| launch.run(&path, &target, cx))
        .child(icon(launcher.icon, px(theme::icon_small()), ink))
        .child(launcher.label)
        .into_any_element()
}

/// Every other way to open the repo, keys included.
fn open_in_section(launchers: &[Launcher], folder: &str, entities: &Entity<Store>) -> AnyElement {
    let mut grid = div().mx_4().my_3().flex().flex_wrap().gap_2();
    for launcher in launchers {
        let target = entities.clone();
        let path = folder.to_string();
        let launch = launcher.launch.clone();
        grid = grid.child(
            div()
                .id(SharedString::from(format!(
                    "insp-launch-{}",
                    launcher.label
                )))
                .a11y_button(launcher.label)
                .cursor_pointer()
                .flex_1()
                .min_w(px(200.))
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .px_3()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::LINE_STRONG)
                .bg(theme::PANEL)
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::TEXT)
                .hover(|el| el.bg(theme::RAISED))
                .on_click(move |_, _, cx| launch.run(&path, &target, cx))
                .child(icon(
                    launcher.icon,
                    px(theme::icon_small()),
                    theme::TEXT_DIM,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(launcher.label),
                )
                .children(
                    launcher
                        .key
                        .map(|key| keycap(key, theme::TEXT_DIM, theme::KEY_BORDER)),
                ),
        );
    }
    card(section_title("Open in".to_string()))
        .child(grid)
        .into_any_element()
}

/// A click handler for a control in a card.
type Action = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App)>;

/// Goes to `tab`.
fn open_tab(entities: &Entity<Store>, tab: WorkspaceTab) -> Action {
    let target = entities.clone();
    Box::new(move |_, _, cx| target.update(cx, |s, cx| s.open_detail_tab(tab, cx)))
}

/// A card: the base surface on the detail's panel, its head on top.
fn card(head: gpui::Div) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .pb_2()
        .rounded(px(8.))
        .border_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::SURFACE)
        .child(head)
}

/// A card's head: its title in sentence case.
fn section_title(label: String) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .min_h(px(44.))
        .px_4()
        .mb_1()
        .border_b_1()
        .border_color(theme::BORDER_LIGHT)
        .child(
            div()
                .text_size(px(theme::font_name()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT)
                .child(label),
        )
}

fn section_head(
    label: String,
    id: &'static str,
    action: &'static str,
    on_click: Action,
) -> gpui::Div {
    section_title(label).child(
        div()
            .id(id)
            .debug_selector(move || id.into())
            .a11y_button(action)
            .cursor_pointer()
            .text_size(px(theme::font_control()))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme::ACCENT_INK)
            .hover(|el| el.text_color(theme::ACCENT_HOVER))
            .on_click(on_click)
            .child(action),
    )
}

/// How many uncommitted files the card lists.
const FILE_ROWS: usize = 3;

/// The selected repo's uncommitted files, capped: the card is a teaser for the
/// Changes tab, not a second file table.
fn files_section(
    store: &Store,
    repo: &RepoEntry,
    settings: &AppSettings,
    entities: &Entity<Store>,
) -> AnyElement {
    let folder = repo.folder_path.to_string_lossy().to_string();
    let loaded = store.changes_loaded_for.as_deref() == Some(folder.as_str());
    // None = the snapshot for this repo has not landed yet. Reporting "0
    // files · clean" while it is still in flight would be a lie, so the
    // section says it is loading instead.
    let files = if loaded {
        store.changes().map(|snap| snap.merged_files())
    } else {
        None
    };

    let head = match &files {
        Some(files) => format!("Uncommitted · {} files", files.len()),
        // the entry's count is already accurate; the file list is what the
        // snapshot adds, so the header does not have to wait for it
        None => format!("Uncommitted · {} files", repo.git_modified_count),
    };
    let mut section = card(section_head(
        head,
        "insp-commit",
        "Review",
        open_tab(entities, WorkspaceTab::Changes),
    ));

    let Some(files) = files else {
        return section.child(muted("loading changes…")).into_any_element();
    };

    if files.is_empty() {
        return section
            .child(
                div()
                    .mx_4()
                    .my_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::TEXT_DIM)
                    .child(icon(
                        "icons/icon-check.svg",
                        px(theme::icon_small()),
                        theme::SUCCESS_BRIGHT,
                    ))
                    .child("Working tree clean"),
            )
            .into_any_element();
    }

    for file in files.iter().take(FILE_ROWS) {
        section = section.child(file_row(file, settings, &folder, entities));
    }
    if files.len() > FILE_ROWS {
        section = section.child(more_link(
            "insp-more-files",
            format!("and {} more →", files.len() - FILE_ROWS),
            open_tab(entities, WorkspaceTab::Changes),
        ));
    }
    section.into_any_element()
}

fn file_row(
    file: &warp_station_core::model::GitChangedFile,
    settings: &AppSettings,
    folder: &str,
    entities: &Entity<Store>,
) -> AnyElement {
    let target = entities.clone();
    let path = format!("{folder}/{}", file.path);
    let exe = settings.repos.vscode_executable.clone();
    let profile = settings.repos.vscode_profile.clone();
    let status = file.display_status_code().to_string();
    let status_color = match file.display_status_code() {
        "M" => theme::AMBER_BRIGHT,
        "A" | "U" | "?" => theme::ADD,
        "D" => theme::DEL,
        _ => theme::TEXT_MUTED,
    };

    let file_id = SharedString::from(format!("insp-file-{}", file.path));
    let debug_path = file.path.clone();
    let mut row = div()
        .id(file_id)
        .debug_selector(move || format!("insp-file-{debug_path}"))
        .a11y_button(file.path.clone())
        .cursor_pointer()
        .hover(|d| d.bg(theme::HOVER_BG))
        .on_click(move |_, _, cx| {
            if let Err(err) = terminal::open_in_vscode_at(&path, 1, &exe, &profile) {
                target.update(cx, |s, _| s.notify_error(err));
            }
        })
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(32.))
        .px_4()
        .font_family(theme::code_font())
        .child(
            div()
                .w(px(12.))
                .text_size(px(theme::font_row_meta()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(status_color)
                .child(status),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_row_meta()))
                .text_color(theme::TEXT_DIM)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(file.path.clone()),
        );
    if let Some(additions) = file.additions {
        row = row.child(
            div()
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::ADD)
                .child(format!("+{additions}")),
        );
    }
    if let Some(deletions) = file.deletions {
        row = row.child(
            div()
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::DEL)
                .child(format!("−{deletions}")),
        );
    }
    row.into_any_element()
}

fn issues_section(store: &Store, repo: &RepoEntry, entities: &Entity<Store>) -> AnyElement {
    let folder = repo.folder_path.to_string_lossy().to_string();
    let snapshot = store.github_by_folder.get(&folder);
    let issue_count = snapshot.map(|snap| snap.issues.len()).unwrap_or(0);
    let pull_count = snapshot.map(|snap| snap.pulls.len()).unwrap_or(0);

    // the tab with something on it
    let tab = if issue_count == 0 && pull_count > 0 {
        WorkspaceTab::PullRequests
    } else {
        WorkspaceTab::Issues
    };
    let mut section = card(section_head(
        format!("GitHub · {issue_count} issues · {pull_count} PRs"),
        "insp-github-open",
        "Open",
        open_tab(entities, tab),
    ));

    let Some(snapshot) = snapshot else {
        return section
            .child(muted("GitHub data not loaded"))
            .into_any_element();
    };
    if snapshot.issues.is_empty() {
        return section.child(muted("No open issues")).into_any_element();
    }

    for issue in snapshot.issues.iter().take(3) {
        let target = entities.clone();
        let url = issue.url.clone();
        section = section.child(
            div()
                .id(SharedString::from(format!("insp-issue-{}", issue.number)))
                .debug_selector({
                    let number = issue.number;
                    move || format!("insp-issue-{number}")
                })
                .a11y_button(issue.title.clone())
                .cursor_pointer()
                .hover(|d| d.bg(theme::HOVER_BG))
                .on_click(move |_, _, cx| {
                    if let Err(err) = terminal::open_url(&url) {
                        target.update(cx, |s, _| s.notify_error(err));
                    }
                })
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(34.))
                .px_4()
                .child(icon(
                    "icons/icon-circle-dot.svg",
                    px(theme::icon_tiny()),
                    theme::ISSUE,
                ))
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::TEXT_MUTED)
                        .child(format!("#{}", issue.number)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::TEXT)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(issue.title.clone())),
                ),
        );
    }
    if snapshot.issues.len() > 3 {
        section = section.child(more_link(
            "insp-more-issues",
            format!("and {} more →", snapshot.issues.len() - 3),
            open_tab(entities, WorkspaceTab::Issues),
        ));
    }
    section.into_any_element()
}

/// Azure DevOps at a glance, for repos with a project behind them.
fn azure_section(store: &Store, repo: &RepoEntry, entities: &Entity<Store>) -> Option<AnyElement> {
    let folder = repo.folder_path.to_string_lossy();
    let snapshot = store
        .azure_by_folder
        .get(folder.as_ref())
        .filter(|snapshot| snapshot.available)?;
    let latest = if snapshot.pipelines.is_empty() {
        "No pipeline runs"
    } else if snapshot.pipeline_running() {
        "Latest run in progress"
    } else if snapshot.pipeline_failed() {
        "Latest run failed"
    } else if snapshot.pipeline_ok() {
        "Latest run succeeded"
    } else {
        "Latest run queued"
    };
    Some(
        card(section_head(
            format!(
                "Azure DevOps · {} PRs · {} work items",
                snapshot.pulls.len(),
                snapshot.work_items.len()
            ),
            "insp-azure-open",
            "Open",
            open_tab(entities, WorkspaceTab::Pipelines),
        ))
        .child(muted(latest))
        .into_any_element(),
    )
}

/// What GitHub says about the repo. Absent until the details request answers, and for repos not on GitHub.
fn details_section(store: &Store) -> Option<AnyElement> {
    let details = store.github_details.as_ref()?;
    let facts = [
        ("Owner", details.owner.clone()),
        ("Language", details.primary_language.clone()),
        ("License", details.license.clone()),
        ("Default", details.default_branch.clone()),
        ("Created", details.created_at.as_deref().map(format_created)),
    ];

    let mut grid = div().flex().flex_col();
    let mut empty = true;
    for (label, value) in facts {
        let Some(value) = value else { continue };
        empty = false;
        grid = grid.child(fact(
            label,
            div()
                .text_size(px(theme::font_body()))
                .text_color(theme::TEXT)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(value),
        ));
    }
    if empty && details.topics.is_empty() {
        return None;
    }

    let topics =
        div()
            .mx_4()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .pt_1()
            .children(details.topics.iter().map(|topic| {
                div()
                    .h(px(24.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(12.))
                    .bg(theme::RAISED)
                    .text_size(px(theme::font_row_meta()))
                    .text_color(theme::TEXT_DIM)
                    .child(SharedString::from(topic.clone()))
            }));

    Some(
        card(section_title("Details".to_string()))
            .debug_selector(|| "insp-details".into())
            .child(grid)
            .child(topics)
            .into_any_element(),
    )
}

/// gh returns ISO timestamps; the details show "27 Nov 2023".
fn format_created(iso: &str) -> String {
    chrono::NaiveDate::parse_from_str(&iso[..10.min(iso.len())], "%Y-%m-%d")
        .map(|d| d.format("%-d %b %Y").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

fn muted(label: &'static str) -> AnyElement {
    div()
        .mx_4()
        .my_1()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_DIM)
        .child(label)
        .into_any_element()
}

fn more_link(id: &'static str, label: String, on_click: Action) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label.clone())
        .cursor_pointer()
        .mx_4()
        .pt_1()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_DIM)
        .hover(|el| el.text_color(theme::TEXT))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

fn keycap(label: impl Into<SharedString>, color: Rgba, border: Rgba) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(20.))
        .min_w(px(20.))
        .px_1()
        .rounded(px(4.))
        .border_1()
        .border_color(border)
        .font_family(theme::code_font())
        .text_size(px(theme::font_micro()))
        .text_color(color)
        .child(label.into())
        .into_any_element()
}
