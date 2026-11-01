//! The repo detail rail: everything the floating drawer used to hold, docked
//! beside the list instead of over it. Identity and tags at the top, then the
//! sync card, the one primary launch above the secondary launchers, and the
//! uncommitted files, GitHub issues and repo details closing the column.

use crate::net::GitAction;
use crate::store::{Store, WorkspaceTab};
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::launchers::{self, Launcher};
use crate::ui::repos::ReposPage;
use devtools_core::rel_time;
use devtools_core::repo_store::RepoEntry;
use devtools_core::settings::AppSettings;
use devtools_core::terminal;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Entity, FontWeight, MouseButton, Rgba, SharedString, div, px,
    relative,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};

/// Rail width. The mockup asks for roughly a third of a 1280 window; a fixed
/// rail keeps the list from reflowing as the inspector's content changes.
pub const RAIL_W: f32 = 540.;

/// The open "⋯" menu: the repo it opened on, because a moved selection closes
/// it and it must never act on another one, and whether Remove took its first
/// click.
pub struct RepoMenu {
    pub folder: String,
    pub remove_armed: bool,
}

/// The right rail for the selected repo. The page keeps a row selected, so it
/// only falls back to its empty state while the list has nothing to select.
pub fn render(
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
    repo_menu: Option<&RepoMenu>,
) -> AnyElement {
    let mut rail = div()
        .id("repos-inspector")
        .debug_selector(|| "repos-inspector".into())
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .w(px(RAIL_W))
        .min_h_0()
        .overflow_y_scroll()
        .border_l_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::CARD)
        .px(px(18.))
        .pt(px(15.))
        .pb(px(14.));

    let Some(repo) = store.selected_entry() else {
        return rail.child(empty_state()).into_any_element();
    };

    let folder = repo.folder_path.to_string_lossy().to_string();
    let settings = store.settings();
    let launchers = launchers::launchers(&settings, repo);
    rail = rail
        .child(header(repo, entities, page, &folder))
        .child(path_line(repo, entities))
        .child(tag_row(repo, page, tag_draft, entities, &folder))
        .child(sync_card(repo, entities))
        .children(
            launchers
                .first()
                .map(|primary| primary_action(primary, &folder, entities)),
        )
        .child(launcher_rows(
            launchers.get(1..).unwrap_or_default(),
            &folder,
            entities,
        ))
        .child(files_section(store, repo, &settings, entities))
        .when(settings.repos.enable_git_hub, |rail| {
            rail.child(issues_section(store, repo, entities))
        })
        .when(settings.repos.enable_azure_dev_ops, |rail| {
            rail.children(azure_section(store, repo, entities))
        })
        .children(details_section(store))
        // last, so it paints over the sections it hangs above
        .children(repo_menu.map(|menu| popup_menu(repo, menu, store, entities, page, &folder)));

    rail.into_any_element()
}

fn empty_state() -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_MUTED)
        .child("No repositories yet")
        .into_any_element()
}

fn header(
    repo: &RepoEntry,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    folder: &str,
) -> AnyElement {
    let favorite = repo.is_favorite();
    let star_target = entities.clone();
    let star_folder = folder.to_string();
    div()
        .flex()
        .items_center()
        .gap(px(6.))
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
                .size(px(20.))
                .rounded(px(3.))
                .flex()
                .items_center()
                .justify_center()
                .hover(|d| d.bg(theme::HOVER_BG))
                .on_click(move |_, _, cx| {
                    star_target.update(cx, |s, cx| s.toggle_favorite(&star_folder, cx));
                })
                .child(icon(
                    if favorite {
                        "icons/icon-star-filled.svg"
                    } else {
                        "icons/icon-star.svg"
                    },
                    px(13.),
                    if favorite {
                        theme::STAR_ON
                    } else {
                        theme::STAR_MUTED
                    },
                )),
        )
        .child(
            div()
                .id("insp-more")
                .debug_selector(|| "insp-more".into())
                .a11y_button("Repository menu")
                .cursor_pointer()
                .size(px(20.))
                .rounded(px(3.))
                .flex()
                .items_center()
                .justify_center()
                .hover(|d| d.bg(theme::HOVER_BG))
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
                    px(13.),
                    theme::MUTED_ICON,
                )),
        )
        .into_any_element()
}

type MenuAction = Box<dyn Fn(&mut App)>;

/// The header's "⋯" menu: the repo verbs, in a popup hanging from the header,
/// so opening it leaves the rail where it is.
fn popup_menu(
    repo: &RepoEntry,
    state: &RepoMenu,
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    folder: &str,
) -> AnyElement {
    let archived = repo.is_archived();
    let mut menu = div()
        .id("insp-repo-menu")
        .debug_selector(|| "insp-repo-menu".into())
        .absolute()
        .top(px(42.))
        .right(px(18.))
        .w(px(220.))
        .p(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::POPUP)
        .shadow_md()
        // presses inside must not reach the rail under it, or the window's
        // outside-click dismissal before the row's own click lands
        .occlude()
        .flex()
        .flex_col()
        .gap(px(2.));

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
        .rounded(px(4.))
        .px(px(8.))
        .py(px(5.))
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_DIM)
        .hover(|d| d.bg(theme::HOVER_BG).text_color(theme::TEXT))
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
        .rounded(px(4.))
        .px(px(8.))
        .py(px(5.))
        .flex()
        .flex_col()
        .gap_0p5()
        .text_size(px(theme::font_row_meta()))
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

fn path_line(repo: &RepoEntry, entities: &Entity<Store>) -> AnyElement {
    // the ellipsis needs a definite width: flex_1 inside a row gives it the
    // rail's content width, which a bare column child does not
    div()
        .pt_1()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::TEXT_MUTED)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(display_path(repo)),
        )
        .child(
            div()
                .id("insp-readme")
                .debug_selector(|| "insp-readme".into())
                .a11y_button("Show README")
                .cursor_pointer()
                .flex_shrink_0()
                .font_family(theme::code_font())
                .text_size(px(theme::font_micro()))
                .text_color(theme::MUTED_ICON)
                .hover(|el| el.text_color(theme::TEXT))
                .on_click(open_tab(entities, WorkspaceTab::Readme))
                .child("README ↗"),
        )
        .into_any_element()
}

/// `~/developments/foo` rather than the absolute path: the rail has one line
/// and the home prefix carries no information.
fn display_path(repo: &RepoEntry) -> String {
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
        .gap(px(5.))
        .mt(px(8.))
        .min_h(px(22.));
    for tag in repo.tags.iter().filter(|tag| !is_system_tag(tag)) {
        row = row.child(tag_chip(tag, entities, folder));
    }
    if let Some(input) = tag_draft {
        row = row.child(div().w(px(120.)).child(Input::new(input).small()));
    } else {
        let target = page.clone();
        row = row.child(
            div()
                .id("insp-add-tag")
                .debug_selector(|| "insp-add-tag".into())
                .a11y_button("Add tag")
                .cursor_pointer()
                .h(px(20.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(3.))
                .rounded(px(4.))
                .border_1()
                .border_dashed()
                .border_color(theme::GHOST_BORDER)
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::TEXT_MUTED)
                .hover(|d| {
                    d.border_color(theme::GHOST_BORDER_HOVER)
                        .text_color(theme::TEXT)
                })
                .on_click(move |_, _, cx| {
                    target.update(cx, |this, cx| this.begin_tag_edit(cx));
                })
                .child("+ tag"),
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
        .h(px(20.))
        .pl(px(7.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .border_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::SYNC_BG)
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
/// as removable chips was the drawer's worst offence.
fn is_system_tag(tag: &str) -> bool {
    tag.eq_ignore_ascii_case(devtools_core::repo_store::FAVORITES_TAG)
        || tag.eq_ignore_ascii_case(devtools_core::repo_store::ARCHIVED_TAG)
}

/// Branch, how far it is from the remote, and the two sync verbs.
fn sync_card(repo: &RepoEntry, entities: &Entity<Store>) -> AnyElement {
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

    let mut note = String::new();
    if ahead > 0 {
        note.push_str(&format!("{ahead} to push"));
    }
    if behind > 0 {
        if !note.is_empty() {
            note.push_str(" · ");
        }
        note.push_str(&format!("{behind} to pull"));
    }
    if note.is_empty() {
        note.push_str("in sync");
    }
    note.push_str(&format!(" · fetched {fetched}"));

    let branch_line = div()
        .id("insp-branch")
        .debug_selector(|| "insp-branch".into())
        .flex()
        .items_center()
        .gap(px(4.))
        .font_family(theme::code_font())
        .text_size(px(11.))
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
            px(12.),
            theme::TEXT_MUTED,
        ))
        .child(SharedString::from(branch))
        .when(ahead > 0, |line| {
            line.child(
                div()
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::AMBER_BRIGHT)
                    .child(format!("↑{ahead}")),
            )
        });

    div()
        .mt(px(12.))
        .px(px(10.))
        .py(px(8.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::LINE_STRONG)
        .bg(theme::SYNC_BG)
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .min_w_0()
                .child(branch_line)
                .child(
                    div()
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_mono_meta()))
                        .text_color(theme::TEXT_MUTED)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(note),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(small_button("insp-pull", "Pull".into(), {
                    let target = entities.clone();
                    move |_, _, cx| {
                        target.update(cx, |s, cx| s.run_git(GitAction::Pull, cx));
                    }
                }))
                .child(small_button(
                    "insp-push",
                    if ahead > 0 {
                        format!("Push ↑{ahead}")
                    } else {
                        "Push".into()
                    },
                    {
                        let target = entities.clone();
                        move |_, _, cx| {
                            target.update(cx, |s, cx| s.run_git(GitAction::Push, cx));
                        }
                    },
                )),
        )
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
        .h(px(23.))
        .px(px(7.))
        .rounded(px(4.))
        .border_1()
        .border_color(theme::GHOST_BORDER)
        .bg(theme::SYNC_BG)
        .text_size(px(theme::font_mono_meta()))
        .text_color(theme::TEXT_DIM)
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|el| el.bg(theme::HOVER_BG).text_color(theme::TEXT))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

/// The one filled control in the app: the first launcher, the default editor
/// unless the settings turned it off.
fn primary_action(launcher: &Launcher, folder: &str, entities: &Entity<Store>) -> AnyElement {
    let target = entities.clone();
    let path = folder.to_string();
    let launch = launcher.launch.clone();
    div()
        .id("insp-primary")
        .debug_selector(|| "insp-primary".into())
        .a11y_button(launcher.label)
        .cursor_pointer()
        .mt(px(9.))
        .h(px(38.))
        .px(px(10.))
        .rounded(px(5.))
        .border_1()
        .border_color(theme::ACCENT_BORDER)
        .bg(theme::ACCENT)
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::ON_ACCENT)
        .hover(|el| el.bg(theme::ACCENT_HOVER))
        .on_click(move |_, _, cx| launch.run(&path, &target, cx))
        .child(icon(launcher.icon, px(14.), theme::ON_ACCENT))
        .child(launcher.label)
        .child(div().flex_1())
        .child(keycap(
            "↵",
            theme::rgba(0xFFFFFFFF),
            theme::rgba(0xFFFFFF47),
        ))
        .into_any_element()
}

/// Secondary verbs: one row each, keycap flush right.
fn launcher_rows(launchers: &[Launcher], folder: &str, entities: &Entity<Store>) -> AnyElement {
    let mut list = div().flex().flex_col().pt(px(8.));
    for launcher in launchers {
        let target = entities.clone();
        let path = folder.to_string();
        let launch = launcher.launch.clone();
        list = list.child(
            div()
                .id(SharedString::from(format!(
                    "insp-launch-{}",
                    launcher.label
                )))
                .a11y_button(launcher.label)
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(29.))
                .px(px(4.))
                .rounded(px(4.))
                .text_color(theme::TEXT_DIM)
                .hover(|el| el.bg(theme::HOVER_BG).text_color(theme::TEXT))
                .on_click(move |_, _, cx| launch.run(&path, &target, cx))
                .child(icon(launcher.icon, px(14.), theme::MUTED_ICON))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(theme::font_body()))
                        .child(launcher.label),
                )
                .children(
                    launcher
                        .key
                        .map(|key| keycap(key, theme::TEXT_MUTED, theme::KEY_BORDER)),
                ),
        );
    }
    list.into_any_element()
}

/// A click handler the rail hands to a shell-free control.
type Action = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App)>;

/// Opens the workspace sheet on `tab`.
fn open_tab(entities: &Entity<Store>, tab: WorkspaceTab) -> Action {
    let target = entities.clone();
    Box::new(move |_, _, cx| target.update(cx, |s, cx| s.open_sheet(tab, cx)))
}

/// A section's rule and uppercase label.
fn section_title(label: String) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .mt(px(11.))
        .pt(px(8.))
        .border_t_1()
        .border_color(theme::ROWS_RULE)
        .pb(px(2.))
        .child(
            div()
                .font_family(theme::code_font())
                .text_size(px(theme::font_micro()))
                .text_color(theme::MUTED_ICON)
                .child(label.to_uppercase()),
        )
}

fn section_head(
    label: String,
    id: &'static str,
    action: &'static str,
    on_click: Action,
) -> AnyElement {
    section_title(label)
        .child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .a11y_button(action)
                .cursor_pointer()
                .font_family(theme::code_font())
                .text_size(px(theme::font_micro()))
                .text_color(theme::MUTED_ICON)
                .hover(|el| el.text_color(theme::TEXT))
                .on_click(on_click)
                .child(action),
        )
        .into_any_element()
}

/// The selected repo's uncommitted files, capped at three like the mockup:
/// the section is a teaser for the Changes view, not a second file table.
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
    let mut section = div().flex().flex_col().child(section_head(
        head,
        "insp-commit",
        "Commit… C",
        open_tab(entities, WorkspaceTab::Changes),
    ));

    let Some(files) = files else {
        return section.child(muted("loading changes…")).into_any_element();
    };

    if files.is_empty() {
        return section
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(theme::font_row_meta()))
                    .text_color(theme::TEXT_MUTED)
                    .child(icon("icons/icon-check.svg", px(12.), theme::SUCCESS_BRIGHT))
                    .child("Working tree clean"),
            )
            .into_any_element();
    }

    for file in files.iter().take(3) {
        section = section.child(file_row(file, settings, &folder, entities));
    }
    if files.len() > 3 {
        section = section.child(more_link(
            "insp-more-files",
            format!("and {} more →", files.len() - 3),
            open_tab(entities, WorkspaceTab::Changes),
        ));
    }
    section.into_any_element()
}

fn file_row(
    file: &devtools_core::model::GitChangedFile,
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
        .gap(px(6.))
        .h(px(27.))
        .border_b_1()
        .border_color(theme::ROW_LINE)
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
    let mut section = div().flex().flex_col().child(section_head(
        format!("GitHub · {issue_count} issues · {pull_count} PRs"),
        "insp-github-open",
        "open ↗",
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
                .gap(px(7.))
                .h(px(23.))
                .border_b_1()
                .border_color(theme::ROW_LINE)
                .child(
                    div()
                        .w(px(40.))
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::TEXT_MUTED)
                        .child(format!("#{}", issue.number)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .text_color(theme::TEXT_DIM)
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
        div()
            .flex()
            .flex_col()
            .child(section_head(
                format!(
                    "Azure DevOps · {} PRs · {} work items",
                    snapshot.pulls.len(),
                    snapshot.work_items.len()
                ),
                "insp-azure-open",
                "open ↗",
                open_tab(entities, WorkspaceTab::Pipelines),
            ))
            .child(muted(latest))
            .into_any_element(),
    )
}

/// What GitHub says about the repo, in two columns so the rail stays short.
/// Absent until the details request answers, and for repos not on GitHub.
fn details_section(store: &Store) -> Option<AnyElement> {
    let details = store.github_details.as_ref()?;
    let facts = [
        ("Owner", details.owner.clone()),
        ("Language", details.primary_language.clone()),
        ("License", details.license.clone()),
        ("Default", details.default_branch.clone()),
        ("Created", details.created_at.as_deref().map(format_created)),
    ];

    let mut grid = div().flex().flex_wrap().pt(px(2.));
    let mut empty = true;
    for (label, value) in facts {
        let Some(value) = value else { continue };
        empty = false;
        grid = grid.child(
            div()
                .w(relative(0.5))
                .h(px(20.))
                .pr(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(theme::font_row_meta()))
                .child(
                    div()
                        .w(px(56.))
                        .flex_shrink_0()
                        .text_color(theme::TEXT_MUTED)
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(theme::TEXT_DIM)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(value),
                ),
        );
    }
    if empty && details.topics.is_empty() {
        return None;
    }

    let topics =
        div()
            .flex()
            .flex_wrap()
            .gap(px(4.))
            .pt(px(4.))
            .children(details.topics.iter().map(|topic| {
                div()
                    .h(px(18.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .bg(theme::SYNC_BG)
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::TEXT_DIM)
                    .child(SharedString::from(topic.clone()))
            }));

    Some(
        div()
            .debug_selector(|| "insp-details".into())
            .flex()
            .flex_col()
            .child(section_title("Details".to_string()))
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
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_MUTED)
        .child(label)
        .into_any_element()
}

fn more_link(id: &'static str, label: String, on_click: Action) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label.clone())
        .cursor_pointer()
        .pt(px(4.))
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_MUTED)
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
        .h(px(17.))
        .min_w(px(18.))
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(border)
        .text_size(px(theme::font_label()))
        .text_color(color)
        .child(label.into())
        .into_any_element()
}
