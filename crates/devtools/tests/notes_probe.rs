//! Headless probe for the Notes window: the carded sidebar, opening a note
//! from the tree, the single Obsidian-style live editor (no mode switcher),
//! task checkboxes toggling on click, Enter continuing lists — and the
//! pinned scope: the tree re-roots only through the rows' notes icon (or
//! the up button), never a repo row press.

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
    std::fs::write(notes.join("task.md"), "- [ ] alpha task\n").unwrap();
    std::fs::write(notes.join("list.md"), "- alpha\n").unwrap();
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
fn notes_live_editor_tree_and_pinned_scope(cx: &mut TestAppContext) {
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

    // back to the store root for the editor walk
    click(cx, "notes-up-dir");

    // single surface: the live editor IS the pane, the mode switcher is gone
    assert!(bounds(cx, "notes-live").is_some(), "the live editor pane renders");
    for mode in ["notes-mode-Edit", "notes-mode-Live", "notes-mode-Split", "notes-mode-Preview"] {
        assert!(bounds(cx, mode).is_none(), "the {mode} mode switcher is gone");
    }

    // clicking a task checkbox toggles it in place — the box is the `[ ]`
    // span of the first line, toggling keeps its offsets stable
    click(cx, "note-item-alpha");
    click(cx, "note-item-task.md");
    let box_center = {
        let b = cx
            .update(|_, cx| {
                page.as_ref().unwrap().read(cx).editor_handle().read(cx).range_to_bounds(&(2..5))
            })
            .expect("the task box has layout bounds");
        Point::new(b.origin.x + b.size.width / 2., b.origin.y + b.size.height / 2.)
    };
    cx.simulate_click(box_center, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|_, cx| {
        let value = page.as_ref().unwrap().read(cx).editor_value(cx);
        assert!(value.contains("- [x]"), "clicking the box checks the task: {value:?}");
    });
    cx.simulate_click(box_center, Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let value = page.as_ref().unwrap().read(cx).editor_value(cx);
        assert!(value.contains("- [ ]"), "clicking again unchecks it: {value:?}");
    });

    // Enter inside a list item continues the list. The real key path is the
    // kit's Enter action (the harness's simulated-IME keystroke would also
    // inject a literal '\n' before deferred events run), so dispatch the
    // action itself.
    click(cx, "note-item-list.md");
    cx.update(|window, cx| {
        let editor = page.as_ref().unwrap().read(cx).editor_handle().clone();
        editor.update(cx, |state, cx| {
            state.focus(window, cx);
            state.set_selected_range(7..7, cx); // end of "- alpha"
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter { secondary: false, shift: false }),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|_, cx| {
        let value = page.as_ref().unwrap().read(cx).editor_value(cx);
        assert!(value.contains("- alpha\n- "), "Enter continues the list: {value:?}");
    });

    // typing markdown on the live surface drives the live highlighter on
    // every edit (its parse must not panic and the buffer must hold)
    click(cx, "note-item-note.md");
    let live = bounds(cx, "notes-live").unwrap();
    cx.simulate_click(
        Point::new(live.origin.x + live.size.width / 2., live.origin.y + live.size.height / 2.),
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.simulate_input("# Heading\n\n**bold** and `code`\n- [x] done\n");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(bounds(cx, "notes-live").is_some(), "the live pane survives markdown typing");
    cx.update(|_, cx| {
        let value = page.as_ref().unwrap().read(cx).editor_value(cx);
        assert!(value.contains("# Heading"), "typed markdown reached the buffer: {value:?}");
        assert!(value.contains("**bold**"), "typed bold markers intact: {value:?}");
    });

    let _ = std::fs::remove_dir_all(&home);
}
