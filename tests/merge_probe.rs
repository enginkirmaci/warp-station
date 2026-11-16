//! The Merge section draws its header, conflict list and aligned source panes
//! from an injected session; a real session comes from the core read model,
//! so this probe keeps the page honest about the shape it consumes.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use warp_station::store::Store;
use warp_station::ui::merge::MergePage;
use warp_station::ui::window::MainWindow;
use warp_station_core::git::merge::{
    ConflictBlock, LineSpan, MergeFile, MergeOperation, MergeRow, MergeSession, TextMerge,
};

/// Points HOME at a scratch folder once, so no probe reads or writes the
/// developer's real `~/.warp`.
fn scratch_home() {
    static HOME: std::sync::Once = std::sync::Once::new();
    HOME.call_once(|| {
        let home =
            std::env::temp_dir().join(format!("warp-station-merge-probe-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".warp")).expect("scratch home");
        // SAFETY: process-global env; every probe here calls this first.
        // USERPROFILE too: Windows resolves the data root from it first.
        unsafe {
            std::env::set_var("HOME", &home);
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", &home);
        };
    });
}

/// Scratch settings for one probe, keyed like the other page probes so
/// parallel tests never share a file.
fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-merge-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

/// One conflicted file in the shape the core reader returns for a plain merge.
fn session() -> MergeSession {
    MergeSession {
        operation: MergeOperation::Merge,
        files: vec![MergeFile::Text(TextMerge {
            path: "a.txt".into(),
            ours: vec!["main".into()],
            theirs: vec!["feature".into()],
            result: vec![
                "<<<<<<< HEAD".into(),
                "main".into(),
                "=======".into(),
                "feature".into(),
                ">>>>>>> feature".into(),
            ],
            blocks: vec![ConflictBlock {
                result: LineSpan { start: 0, len: 5 },
                ours: Some(LineSpan { start: 0, len: 1 }),
                theirs: Some(LineSpan { start: 0, len: 1 }),
                ours_text: vec!["main".into()],
                theirs_text: vec!["feature".into()],
                base_text: Vec::new(),
            }],
            rows: vec![MergeRow {
                ours: Some(0),
                theirs: Some(0),
                block: Some(0),
            }],
        })],
    }
}

#[gpui_kit::gpui::test]
fn the_merge_page_draws_its_files_and_panes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("draw")));
    cx.update_entity(&store, |store: &mut Store, _| {
        let folder = "/tmp/merge-probe".to_string();
        store
            .merge_ops
            .insert(folder.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder.clone());
        store.merge = Some(session());
        store.merge_loaded_for = Some(folder);
        store.merge_generation = 1;
    });

    let (_handle, cx) = cx.add_window_view(|_window, cx| MergePage::new(store.clone(), cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.debug_bounds("merge-page").is_some(), "the page draws");
    assert!(
        cx.debug_bounds("merge-header").is_some(),
        "the header draws"
    );
    assert!(
        cx.debug_bounds("merge-file-row").is_some(),
        "the conflict row draws"
    );
    assert!(
        cx.debug_bounds("merge-panes").is_some(),
        "the two source panes draw"
    );
}

#[gpui_kit::gpui::test]
fn the_rail_item_follows_the_stopped_operation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    cx.update_entity(&store, |store: &mut Store, cx| {
        store
            .merge_ops
            .insert("/probe/alpha".to_string(), MergeOperation::Merge);
        cx.notify();
    });

    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    let rail = cx
        .debug_bounds("rail-merge")
        .expect("the rail item exists while an operation is stopped");
    cx.simulate_click(rail.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("merge-page").is_some(),
        "the Merge page opens from the rail item"
    );
    assert!(
        cx.debug_bounds("merge-loading").is_some(),
        "the page says it is reading the operation"
    );

    // the operation ends elsewhere: the next status pass drops the flag
    cx.update_entity(&store, |store: &mut Store, cx| {
        store.merge_ops.clear();
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("rail-merge").is_none(),
        "the rail item leaves when nothing is stopped"
    );
    assert!(
        cx.debug_bounds("merge-page").is_none(),
        "and the section falls back to Repos"
    );
}
