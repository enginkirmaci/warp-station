//! Headless probe for the Notes window: the carded sidebar, opening a note
//! from the tree, the four view modes — Live (Obsidian-style single
//! surface), Split, Preview, Edit — and the pinned scope: the tree re-roots
//! only through the rows' notes icon (or the up button), never a repo row
//! press.

use devtools::store::Store;
use devtools::ui::notes::NotesPage;
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::{px, size, Bounds, Modifiers, Point, Pixels, TestAppContext, VisualTestContext};
use std::path::PathBuf;

fn seed_notes(home: &std::path::Path) {
    let notes = home.join(".devtools/notes/alpha");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(notes.join("note.md"), "# Note\n\nHello **world**.\n").unwrap();
    std::fs::write(notes.join("other.md"), "# Other\n").unwrap();
    let beta = home.join(".devtools/notes/beta");
    std::fs::create_dir_all(&beta).unwrap();
    std::fs::write(beta.join("beta.md"), "# Beta\n").unwrap();
}

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
        git_modified_count: 0,
        git_to_push_count: 0,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: false,
        git_branch_published: false,
        git_status_loaded: true,
        github_pr_count: 0,
        github_issue_count: 0,
    }
}

fn fake_store(cx: &mut TestAppContext) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.replace_repos(vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")]);
        store
    })
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(selector)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = bounds(cx, selector)
        .unwrap_or_else(|| panic!("{selector} missing from the frame"));
    let center = Point {
        x: b.origin.x + b.size.width / 2.,
        y: b.origin.y + b.size.height / 2.,
    };
    cx.simulate_click(center, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

#[gpui::test]
fn notes_window_modes_tree_and_pinned_scope(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!("devtools-notes-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    seed_notes(&home);
    // SAFETY: process-global env, set before Store::new; one scenario per test
    unsafe { std::env::set_var("HOME", &home) };

    let store = fake_store(cx);
    // a repo row press happened before the window opened: the first open
    // pins to that repo (the title-bar toggle's "notes for the selected repo")
    cx.update(|cx| {
        store.update(cx, |s, cx| s.select_repo(Some("/probe/alpha".into()), cx))
    });
    let mut page: Option<gpui::Entity<NotesPage>> = None;
    let handle = cx.open_window(size(px(1000.), px(700.)), |window, cx| {
        let store = store.clone();
        page = Some(cx.entity());
        NotesPage::new(store, window, cx)
    });
    let cx = VisualTestContext::from_window(handle.into(), cx).into_mut();
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // carded sidebar at the reference width
    let sidebar = bounds(cx, "notes-sidebar").expect("notes sidebar must render");
    assert_eq!(sidebar.size.width, px(300.), "sidebar card keeps its 300px width");

    // first open roots at the selected repo's notes folder
    assert!(
        bounds(cx, "note-item-note.md").is_some(),
        "first open pins to the selected repo's notes",
    );
    assert!(
        bounds(cx, "notes-up-dir").is_some(),
        "repo-scoped notes show the up button",
    );
    assert!(
        bounds(cx, "note-item-beta.md").is_none(),
        "other repos' notes stay hidden while scoped",
    );

    // up leaves the repo folder for the whole store — one folder per repo
    click(cx, "notes-up-dir");
    assert!(bounds(cx, "note-item-alpha").is_some(), "up shows the store's repo folders");
    assert!(bounds(cx, "note-item-beta").is_some(), "up shows every repo folder");
    assert!(bounds(cx, "notes-up-dir").is_none(), "store root has no up button");

    // open the folder and then a note from the tree
    click(cx, "note-item-alpha");
    click(cx, "note-item-note.md");
    assert!(
        bounds(cx, "notes-toolbar").is_some(),
        "the note header strip appears once a note is open"
    );

    // a repo row press must not re-root the tree — only the notes icon does
    cx.update(|_, cx| {
        store.update(cx, |s, cx| s.select_repo(Some("/probe/beta".into()), cx))
    });
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(bounds(cx, "note-item-beta").is_some(), "row presses never re-root the tree");
    assert!(
        bounds(cx, "note-item-beta.md").is_none(),
        "the tree is still at the store root after a row press",
    );
    assert!(bounds(cx, "notes-toolbar").is_some(), "the open note survives row presses");

    // the rows' notes icon pins the scope to its repo
    cx.update(|_, cx| {
        store.update(cx, |s, cx| s.request_notes_scope("beta".into(), cx))
    });
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(bounds(cx, "note-item-beta.md").is_some(), "the notes icon re-roots to its repo");
    assert!(bounds(cx, "note-item-alpha").is_none(), "the previous scope is gone");
    assert!(bounds(cx, "notes-up-dir").is_some(), "repo scope shows the up button again");

    // back to the store root for the mode walk
    click(cx, "notes-up-dir");

    // default mode is Live: the Obsidian-style single surface — the editor
    // itself renders the markdown, so there is no separate preview pane
    assert!(bounds(cx, "notes-live").is_some(), "Live is the default pane");
    assert!(bounds(cx, "notes-live-edit").is_none(), "Live has no separate editor pane");
    assert!(bounds(cx, "notes-live-preview").is_none(), "Live has no separate preview pane");

    // Split: raw editor beside the rendered preview
    click(cx, "notes-mode-Split");
    assert!(bounds(cx, "notes-split").is_some(), "Split pane renders");
    assert!(bounds(cx, "notes-split-preview").is_some(), "Split shows the preview");

    // Preview: rendered only, no formatting strip needed
    click(cx, "notes-mode-Preview");
    assert!(bounds(cx, "notes-preview-pane").is_some(), "Preview pane renders");

    // Edit: raw editor
    click(cx, "notes-mode-Edit");
    assert!(bounds(cx, "notes-edit").is_some(), "Edit pane renders");

    // back to Live for the same single surface
    click(cx, "notes-mode-Live");
    assert!(bounds(cx, "notes-live").is_some(), "Live pane returns");

    // typing markdown on the Live surface drives the live highlighter on
    // every edit (its parse must not panic and the buffer must hold)
    let live = bounds(cx, "notes-live").unwrap();
    cx.simulate_click(
        Point::new(live.origin.x + live.size.width / 2., live.origin.y + live.size.height / 2.),
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.simulate_input("# Heading\n\n**bold** and `code`\n- [x] done\n");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(bounds(cx, "notes-live").is_some(), "Live pane survives markdown typing");
    cx.update(|_, cx| {
        let value = page.as_ref().unwrap().read(cx).editor_value(cx);
        assert!(value.contains("# Heading"), "typed markdown reached the buffer: {value:?}");
        assert!(value.contains("**bold**"), "typed bold markers intact: {value:?}");
    });

    let _ = std::fs::remove_dir_all(&home);
}
