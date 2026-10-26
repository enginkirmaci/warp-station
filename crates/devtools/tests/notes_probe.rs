//! Headless probe for the Notes window: the carded sidebar, opening a note
//! from the tree, and the four view modes (Edit / Live / Split / Preview).

use devtools::store::Store;
use devtools::ui::notes::NotesPage;
use gpui::prelude::*;
use gpui::{px, size, Bounds, Modifiers, Point, Pixels, TestAppContext, VisualTestContext};

fn seed_notes(home: &std::path::Path) {
    let notes = home.join(".devtools/notes/alpha");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(notes.join("note.md"), "# Note\n\nHello **world**.\n").unwrap();
    std::fs::write(notes.join("other.md"), "# Other\n").unwrap();
}

fn fake_store(cx: &mut TestAppContext) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| Store::new(cx, net_tx))
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
fn notes_window_modes_and_tree(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!("devtools-notes-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    seed_notes(&home);
    // SAFETY: process-global env, set before Store::new; one scenario per test
    unsafe { std::env::set_var("HOME", &home) };

    let store = fake_store(cx);
    let handle = cx.open_window(size(px(1000.), px(700.)), |window, cx| {
        let store = store.clone();
        NotesPage::new(store, window, cx)
    });
    let cx = VisualTestContext::from_window(handle.into(), &cx).into_mut();
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // carded sidebar at the reference width
    let sidebar = bounds(cx, "notes-sidebar").expect("notes sidebar must render");
    assert_eq!(sidebar.size.width, px(300.), "sidebar card keeps its 300px width");

    // open the folder and then a note from the tree
    click(cx, "note-item-alpha");
    click(cx, "note-item-note.md");
    assert!(
        bounds(cx, "notes-toolbar").is_some(),
        "the note header strip appears once a note is open"
    );

    // default mode is Split
    assert!(bounds(cx, "notes-split").is_some(), "Split is the default pane");

    // Live: editor above, rendered markdown below
    click(cx, "notes-mode-Live");
    assert!(bounds(cx, "notes-live").is_some(), "Live pane renders");
    assert!(bounds(cx, "notes-live-edit").is_some(), "Live shows the editor");
    assert!(bounds(cx, "notes-live-preview").is_some(), "Live shows the preview");

    // Preview: rendered only, no formatting strip needed
    click(cx, "notes-mode-Preview");
    assert!(bounds(cx, "notes-preview-pane").is_some(), "Preview pane renders");

    // Edit: raw editor
    click(cx, "notes-mode-Edit");
    assert!(bounds(cx, "notes-edit").is_some(), "Edit pane renders");

    let _ = std::fs::remove_dir_all(&home);
}
