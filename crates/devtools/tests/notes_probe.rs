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

/// Same-binary gpui tests share one process and HOME is process-global, so
/// the scenarios that seed their own notes tree serialize on this lock
/// (poison-tolerant: one failure must not fail the rest).
static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn hold_home() -> std::sync::MutexGuard<'static, ()> {
    HOME_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
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
    let _home = hold_home();
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

// ---- image paste ----------------------------------------------------------

/// A valid 1x1 RGBA PNG (magic + IHDR + IDAT + IEND).
const PNG_BYTES: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50,
    0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

struct PasteProbe {
    cx: VisualTestContext,
    page: gpui::Entity<NotesPage>,
    home: PathBuf,
    /// holds the HOME lock for the whole scenario, not just the setup
    _home: std::sync::MutexGuard<'static, ()>,
}

/// A window with the seeded alpha note open in the live editor, the editor
/// focused. Each caller passes its own temp-dir prefix — same-binary tests
/// share a process id, and one caller's cleanup must not wipe another's tree.
fn paste_probe(cx: &mut TestAppContext, prefix: &str) -> PasteProbe {
    let _home_guard = hold_home();
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    seed_notes(&home);
    // SAFETY: process-global env, set before Store::new; one scenario per test
    unsafe { std::env::set_var("HOME", &home) };

    let store = fake_store(cx);
    // the first open pins the scope to alpha: the tree roots at its folder
    cx.update(|cx| store.update(cx, |s, cx| s.select_repo(Some("/probe/alpha".into()), cx)));
    let mut page: Option<gpui::Entity<NotesPage>> = None;
    let handle = cx.open_window(size(px(1000.), px(700.)), |window, cx| {
        let store = store.clone();
        page = Some(cx.entity());
        NotesPage::new(store, window, cx)
    });
    let mut probe = PasteProbe {
        cx: VisualTestContext::from_window(handle.into(), cx),
        page: page.unwrap(),
        home,
        _home: _home_guard,
    };
    probe.cx.run_until_parked();
    probe.cx.update(|window, cx| drop(window.draw(cx)));
    click(&mut probe.cx, "note-item-note.md");
    probe.cx.update(|window, cx| {
        let editor = probe.page.read(cx).editor_handle().clone();
        editor.update(cx, |state, cx| state.focus(window, cx));
    });
    probe.cx.run_until_parked();
    probe
}

fn dispatch_paste(probe: &mut PasteProbe) {
    probe
        .cx
        .update(|window, cx| window.dispatch_action(Box::new(gpui_kit::base::input::Paste), cx));
    probe.cx.run_until_parked();
    probe.cx.update(|window, cx| drop(window.draw(cx)));
}

#[gpui::test]
fn pasting_a_clipboard_image_saves_an_asset_and_inserts_a_ref(tcx: &mut TestAppContext) {
    let mut probe = paste_probe(tcx, "devtools-notes-paste-img");
    probe.cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            PNG_BYTES.to_vec(),
        )))
    });
    dispatch_paste(&mut probe);

    probe.cx.update(|_, cx| {
        let page = probe.page.read(cx);
        let value = page.editor_value(cx);
        assert!(
            value.contains("![image](assets/paste-"),
            "image paste inserts a relative markdown ref: {value:?}"
        );
        assert!(
            value.starts_with("![image](assets/paste-"),
            "the ref lands at the caret and is complete: {value:?}"
        );
        assert!(page.is_dirty(cx), "pasting marks the note dirty");
    });

    let assets = probe.home.join(".devtools/notes/alpha/assets");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&assets)
        .expect("the assets folder is created next to the note")
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1, "exactly one asset per paste: {entries:?}");
    let path = entries.remove(0);
    assert_eq!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("png"),
        "{}",
        path.display()
    );
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("paste-"),
        "{}",
        path.display()
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        PNG_BYTES,
        "bytes land on disk unchanged"
    );

    let _ = std::fs::remove_dir_all(&probe.home);
}

#[gpui::test]
fn pasting_text_never_writes_an_asset(tcx: &mut TestAppContext) {
    let mut probe = paste_probe(tcx, "devtools-notes-paste-text");
    probe.cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("pasted text".into()))
    });
    dispatch_paste(&mut probe);

    probe.cx.update(|_, cx| {
        let value = probe.page.read(cx).editor_value(cx);
        assert!(
            value.contains("pasted text"),
            "text paste inserts as text: {value:?}"
        );
    });
    assert!(
        !probe.home.join(".devtools/notes/alpha/assets").exists(),
        "text paste writes no asset folder"
    );

    let _ = std::fs::remove_dir_all(&probe.home);
}

#[gpui::test]
fn a_mixed_clipboard_pastes_text_not_image(tcx: &mut TestAppContext) {
    // Windows delivers text+image as ONE item with multiple entries; the
    // text entry must win and no asset may be written
    let mut probe = paste_probe(tcx, "devtools-notes-paste-mix");
    probe.cx.update(|_, cx| {
        let mut item = gpui::ClipboardItem::new_string("mixed text wins".into());
        item.entries
            .push(gpui::ClipboardEntry::Image(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                PNG_BYTES.to_vec(),
            )));
        cx.write_to_clipboard(item)
    });
    dispatch_paste(&mut probe);

    probe.cx.update(|_, cx| {
        let value = probe.page.read(cx).editor_value(cx);
        assert!(
            value.contains("mixed text wins"),
            "the text entry is pasted: {value:?}"
        );
        assert!(
            !value.contains("![image]"),
            "the image entry is ignored: {value:?}"
        );
    });
    assert!(
        !probe.home.join(".devtools/notes/alpha/assets").exists(),
        "mixed paste writes no asset folder"
    );

    let _ = std::fs::remove_dir_all(&probe.home);
}
