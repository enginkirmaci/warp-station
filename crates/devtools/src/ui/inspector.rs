//! The repo detail rail: everything the floating drawer used to hold, docked
//! beside the list instead of over it. Identity and tags at the top, then the
//! sync card, the one primary launch above five secondary launchers, and the
//! uncommitted files and GitHub issues closing the column.
//!
//! Visual only for now: the rail follows the selection, its buttons land in
//! the wiring pass.

use crate::net::GitAction;
use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::repos::ReposPage;
use devtools_core::rel_time;
use devtools_core::repo_store::RepoEntry;
use devtools_core::settings::AppSettings;
use devtools_core::terminal;
use gpui::prelude::*;
use gpui::{AnyElement, Entity, FontWeight, Rgba, SharedString, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};

/// Rail width. The mockup asks for roughly a third of a 1280 window; a fixed
/// rail keeps the list from reflowing as the inspector's content changes.
pub const RAIL_W: f32 = 540.;

/// The right rail for the selected repo; with nothing selected it previews
/// the first projected repo, so the pane never opens as a bare border.
pub fn render(
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
    repo_menu_open: bool,
) -> AnyElement {
    let mut rail = div()
        .id("repos-inspector")
        .debug_selector(|| "repos-inspector".into())
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

    let Some(repo) = focused_entry(store) else {
        return rail.child(empty_state()).into_any_element();
    };

    let folder = repo.folder_path.to_string_lossy().to_string();
    let settings = store.settings();
    rail = rail
        .child(header(repo, entities, page, &folder))
        .when(repo_menu_open, |rail| {
            rail.child(repo_menu(repo, entities, page, &folder))
        })
        .child(path_line(repo))
        .child(tag_row(repo, page, tag_draft, entities, &folder))
        .child(sync_card(repo, entities, &folder))
        .child(primary_action(&settings, &folder, entities))
        .child(launchers(&settings, &folder, entities, &repo.name))
        .child(files_section(store, repo, &settings, entities))
        .child(issues_section(store, repo, entities));

    rail.into_any_element()
}

/// The selected repo, or the first row while nothing is selected yet — the
/// rail is the app's detail surface, so it opens on something, and the list
/// paints the same row as selected.
pub fn focused_entry(store: &Store) -> Option<&RepoEntry> {
    if let Some(repo) = store.selected_entry() {
        return Some(repo);
    }
    store
        .projected
        .iter()
        .find_map(|row| row.repo_index())
        .and_then(|ix| store.repos.get(ix))
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
                .on_click({
                    let target = page.clone();
                    move |_, _, cx| {
                        target.update(cx, |this, cx| this.toggle_repo_menu(cx));
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

/// The header's "⋯" menu: the repo verbs that used to hide in the table row.
fn repo_menu(
    repo: &RepoEntry,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    folder: &str,
) -> AnyElement {
    let archived = repo.is_archived();
    let favorite = repo.is_favorite();
    let mut menu = div()
        .id("insp-repo-menu")
        .debug_selector(|| "insp-repo-menu".into())
        .mt(px(6.))
        .p(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::POPUP)
        .flex()
        .flex_col()
        .gap(px(2.));

    let item = |id: &'static str, label: &'static str| {
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
    };

    {
        let target = entities.clone();
        let path = folder.to_string();
        let close = page.clone();
        menu = menu.child(
            item("insp-menu-folder", "Open folder").on_click(move |_, _, cx| {
                let _ = terminal::open_folder(&path);
                close.update(cx, |this, cx| this.close_repo_menu(cx));
                let _ = &target;
            }),
        );
    }
    {
        let target = entities.clone();
        let path = folder.to_string();
        let close = page.clone();
        menu = menu.child(
            item(
                "insp-menu-star",
                if favorite {
                    "Remove favorite"
                } else {
                    "Add favorite"
                },
            )
            .on_click(move |_, _, cx| {
                target.update(cx, |s, cx| s.toggle_favorite(&path, cx));
                close.update(cx, |this, cx| this.close_repo_menu(cx));
            }),
        );
    }
    {
        let target = entities.clone();
        let path = folder.to_string();
        let close = page.clone();
        menu = menu.child(
            item(
                "insp-menu-archive",
                if archived { "Restore" } else { "Archive" },
            )
            .on_click(move |_, _, cx| {
                target.update(cx, |s, cx| s.set_repo_archived(&path, !archived, cx));
                close.update(cx, |this, cx| this.close_repo_menu(cx));
            }),
        );
    }
    menu.into_any_element()
}

fn path_line(repo: &RepoEntry) -> AnyElement {
    // the ellipsis needs a definite width: flex_1 inside a row gives it the
    // rail's content width, which a bare column child does not
    div()
        .pt_1()
        .flex()
        .items_center()
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
    _entities: &Entity<Store>,
    _folder: &str,
) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap(px(5.))
        .mt(px(8.))
        .min_h(px(22.));
    for tag in repo.tags.iter().filter(|tag| !is_system_tag(tag)) {
        row = row.child(
            div()
                .h(px(20.))
                .px(px(7.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(theme::HAIRLINE)
                .bg(theme::SYNC_BG)
                .text_size(px(theme::font_row_meta()))
                .text_color(theme::TEXT_DIM)
                .child(SharedString::from(tag.clone())),
        );
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

/// `favorites` and `archived` are derived flags, not user tags — showing them
/// as removable chips was the drawer's worst offence.
fn is_system_tag(tag: &str) -> bool {
    tag.eq_ignore_ascii_case(devtools_core::repo_store::FAVORITES_TAG)
        || tag.eq_ignore_ascii_case(devtools_core::repo_store::ARCHIVED_TAG)
}

/// Branch, how far it is from the remote, and the two sync verbs.
fn sync_card(repo: &RepoEntry, entities: &Entity<Store>, folder: &str) -> AnyElement {
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
        .flex()
        .items_center()
        .gap(px(4.))
        .font_family(theme::code_font())
        .text_size(px(11.))
        .text_color(theme::TEXT_DIM)
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
                    let path = folder.to_string();
                    move |_, _, cx| {
                        // the rail may be previewing a repo the store has not
                        // selected yet; the git verbs always act on the row
                        // the rail is showing
                        target.update(cx, |s, cx| {
                            s.ensure_repo_selected(&path, cx);
                            s.run_git(GitAction::Pull, cx);
                        });
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
                        let path = folder.to_string();
                        move |_, _, cx| {
                            target.update(cx, |s, cx| {
                                s.ensure_repo_selected(&path, cx);
                                s.run_git(GitAction::Push, cx);
                            });
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

/// The one filled control in the app: launch the default editor.
fn primary_action(settings: &AppSettings, folder: &str, entities: &Entity<Store>) -> AnyElement {
    let target = entities.clone();
    let path = folder.to_string();
    let exe = settings.repos.vscode_executable.clone();
    let profile = settings.repos.vscode_profile.clone();
    div()
        .id("insp-primary")
        .debug_selector(|| "insp-primary".into())
        .a11y_button("Open in VS Code")
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
        .on_click(move |_, _, cx| {
            if let Err(err) = terminal::open_in_vscode(&path, &exe, &profile) {
                target.update(cx, |s, _| s.notify_error(err));
            }
        })
        .child(icon("icons/icon-code.svg", px(14.), theme::ON_ACCENT))
        .child("Open in VS Code")
        .child(div().flex_1())
        .child(keycap(
            "↵",
            theme::rgba(0xFFFFFFFF),
            theme::rgba(0xFFFFFF47),
        ))
        .into_any_element()
}

/// Secondary verbs: one row each, keycap flush right, all wired to the same
/// launchers the table row's icons use.
fn launchers(
    settings: &AppSettings,
    folder: &str,
    entities: &Entity<Store>,
    repo_name: &str,
) -> AnyElement {
    type Handler = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App)>;
    let mut items: Vec<(&'static str, &'static str, &'static str, Handler)> = Vec::new();

    {
        let target = entities.clone();
        let path = folder.to_string();
        items.push((
            "icons/icon-app-window.svg",
            "Open in warp-agent",
            "A",
            Box::new(move |_, _, cx| {
                target.update(cx, |s, cx| s.open_in_warp(path.clone(), cx));
            }),
        ));
    }
    {
        let target = entities.clone();
        let path = folder.to_string();
        let term = settings.repos.terminal_executable.clone();
        let exe = if settings.repos.opencode_executable.trim().is_empty() {
            "opencode".to_string()
        } else {
            settings.repos.opencode_executable.clone()
        };
        items.push((
            "icons/icon-command.svg",
            "Launch OpenCode CLI",
            "O",
            Box::new(move |_, _, cx| {
                if let Err(err) =
                    terminal::launch_argv_in_terminal(&path, std::slice::from_ref(&exe), &term)
                {
                    target.update(cx, |s, _| s.notify_error(err));
                }
            }),
        ));
    }
    {
        let path = folder.to_string();
        items.push((
            "icons/icon-folder.svg",
            "Open folder",
            "F",
            Box::new(move |_, _, _| {
                let _ = terminal::open_folder(&path);
            }),
        ));
    }
    {
        let path = folder.to_string();
        let term = settings.repos.terminal_executable.clone();
        items.push((
            "icons/icon-terminal.svg",
            "Open terminal",
            "T",
            Box::new(move |_, _, _| {
                let _ = terminal::open_terminal(&path, &term);
            }),
        ));
    }
    {
        let target = entities.clone();
        let exe = settings.warp_notes.executable.clone();
        let root = devtools_core::notes::resolve_store_root(&settings.general.notes_store_path);
        let name = repo_name.to_string();
        items.push((
            "icons/icon-note.svg",
            "Notes for this repo",
            "N",
            Box::new(move |_, _, cx| {
                if let Err(err) = devtools_core::warp_notes::launch(&exe, &root, Some(&name), None)
                {
                    target.update(cx, |s, _| s.notify_error(err));
                }
            }),
        ));
    }

    let mut list = div().flex().flex_col().pt(px(8.));
    for (icon_path, label, key, handler) in items {
        list = list.child(
            div()
                .id(SharedString::from(format!("insp-launch-{key}")))
                .a11y_button(label)
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(29.))
                .px(px(4.))
                .rounded(px(4.))
                .text_color(theme::TEXT_DIM)
                .hover(|el| el.bg(theme::HOVER_BG).text_color(theme::TEXT))
                .on_click(handler)
                .child(icon(icon_path, px(14.), theme::MUTED_ICON))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(theme::font_launcher()))
                        .child(label),
                )
                .child(keycap(key, theme::TEXT_MUTED, theme::KEY_BORDER)),
        );
    }
    list.into_any_element()
}

/// A click handler the rail hands to a shell-free control (the commit action
/// sets a request flag the window drains).
type Action = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App)>;

fn section_head(label: String, action: &'static str, on_click: Option<Action>) -> AnyElement {
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
        .child({
            let mut button = div()
                .id(SharedString::from(format!("insp-section-{action}")))
                .a11y_button(action)
                .font_family(theme::code_font())
                .text_size(px(theme::font_micro()))
                .text_color(theme::MUTED_ICON)
                .hover(|el| el.text_color(theme::TEXT))
                .child(action);
            if let Some(handler) = on_click {
                button = button.cursor_pointer().on_click(handler);
            }
            button
        })
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
        "Commit… C",
        Some(Box::new({
            let target = entities.clone();
            move |_, _, cx| {
                target.update(cx, |s, _| s.changes_requested = true);
            }
        })),
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
        section = section.child(more_link(format!("and {} more →", files.len() - 3)));
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

    let mut section = div().flex().flex_col().child(section_head(
        format!("GitHub · {issue_count} issues · {pull_count} PRs"),
        "open ↗",
        None,
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
        section = section.child(more_link(format!(
            "and {} more →",
            snapshot.issues.len() - 3
        )));
    }
    section.into_any_element()
}

fn muted(label: &'static str) -> AnyElement {
    div()
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_MUTED)
        .child(label)
        .into_any_element()
}

fn more_link(label: String) -> AnyElement {
    div()
        .pt(px(4.))
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_MUTED)
        .hover(|el| el.text_color(theme::TEXT))
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
