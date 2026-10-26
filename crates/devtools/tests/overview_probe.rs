//! Headless probes for the Overview sidebar and the bar's kebab menu: the
//! three stat tiles must share the sidebar width equally (no min-content
//! blowout at scaled fonts or narrow windows), the row must stay inside the
//! sidebar's content box, the kebab menu must keep its anchored geometry, and
//! a deferred menu must paint above the siblings that follow it in tree
//! order.

use devtools::store::Store;
use devtools::theme;
use devtools::ui::window::MainWindow;
use devtools_core::github::GitHubRepoDetails;
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::{
    canvas, deferred, div, point, px, size, Bounds, Context, Modifiers, Pixels, Render, Size,
    TestAppContext, VisualTestContext, Window,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

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
        git_has_remote: true,
        git_branch_published: true,
        git_status_loaded: true,
        github_pr_count: 0,
        github_issue_count: 0,
    }
}

fn fake_store(cx: &mut TestAppContext, bar_height: f32) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store.bar_height = bar_height;
        store.github_details = Some(GitHubRepoDetails {
            owner: Some("ex".into()),
            created_at: None,
            primary_language: None,
            license: None,
            topics: Vec::new(),
            default_branch: None,
            url: Some("https://github.com/ex/alpha".into()),
        });
        store.select_repo(Some("/probe/alpha".into()), cx);
        store
    })
}

fn open_bar<'a>(
    cx: &'a mut TestAppContext,
    window_size: Size<Pixels>,
) -> (gpui::Entity<Store>, &'a mut VisualTestContext) {
    let store = fake_store(cx, window_size.height.into());
    let handle = cx.open_window(window_size, |window, cx| {
        let store = store.clone();
        MainWindow::new(store, window, cx)
    });
    let vtc = VisualTestContext::from_window(handle.into(), cx).into_mut();
    vtc.run_until_parked();
    vtc.update(|window, cx| drop(window.draw(cx)));
    (store, vtc)
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing from the rendered frame"))
}

fn close(a: Pixels, b: Pixels) -> bool {
    // per-tile sub-pixel rounding (259/3 → 86.5/86/86.5) is expected
    (a - b).abs() < px(1.)
}

/// The Overview stat tiles are `flex_1` in a fixed-width sidebar: they must
/// come out equally wide at every window size and font scale, and the row
/// must never spill past the sidebar's content box (the release screenshot
/// showed the Pull Requests tile eating the Changes tile's width once the
/// system text scale grew the unwrapped label).
#[gpui::test]
fn sidebar_tiles_share_the_width(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    // isolate the probe stores from the developer's real ~/.devtools
    let tmp = std::env::temp_dir().join(format!("devtools-overview-probe-{}", std::process::id()));
    std::fs::create_dir_all(tmp.join(".devtools")).unwrap();
    // SAFETY: process-global env, set once before any Store::new; all probe
    // scenarios live in this one test so nothing races the value.
    unsafe { std::env::set_var("HOME", &tmp) };

    for (label, w, h, scale) in [
        ("narrow-normal", 260., 238., None),
        ("narrow-large", 260., 238., Some(theme::FONT_LARGE)),
        ("wide-normal", 1280., 820., None),
        ("wide-large", 1280., 820., Some(theme::FONT_LARGE)),
    ] {
        theme::set_font_scale(scale);
        let (_store, cx) = open_bar(cx, size(px(w), px(h)));
        let sidebar = bounds(cx, "overview-sidebar");
        let row = bounds(cx, "tiles-row");
        let changes = bounds(cx, "tile-changes");
        let prs = bounds(cx, "tile-prs");
        let issues = bounds(cx, "tile-issues");
        println!(
            "{label}: sidebar {:?} row {:?} changes {:?} prs {:?} issues {:?}",
            (sidebar.origin.x, sidebar.size.width),
            (row.origin.x, row.size.width),
            (changes.origin.x, changes.size.width),
            (prs.origin.x, prs.size.width),
            (issues.origin.x, issues.size.width),
        );

        assert!(
            close(changes.size.width, prs.size.width) && close(prs.size.width, issues.size.width),
            "{label}: tiles must share the width equally (changes {:?} prs {:?} issues {:?})",
            changes.size.width,
            prs.size.width,
            issues.size.width,
        );

        // the row stays inside the sidebar's p_3 content box
        let content_left = sidebar.origin.x + px(12.);
        let content_right = sidebar.origin.x + sidebar.size.width - px(12.);
        assert!(
            row.origin.x >= content_left - px(0.5)
                && row.origin.x + row.size.width <= content_right + px(0.5),
            "{label}: tiles row overflows the sidebar content box ({row:?} vs {content_left}..{content_right})",
        );
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

/// The kebab menu anchors to the header (right 12, top 34) and must exist in
/// the frame with that geometry after its button is pressed.
#[gpui::test]
fn kebab_menu_opens_with_anchored_geometry(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let tmp = std::env::temp_dir().join(format!("devtools-kebab-probe-{}", std::process::id()));
    std::fs::create_dir_all(tmp.join(".devtools")).unwrap();
    // SAFETY: see sidebar_tiles_share_the_width; one scenario per test fn and
    // the env value is set before any Store::new runs.
    unsafe { std::env::set_var("HOME", &tmp) };

    theme::set_font_scale(Some(theme::FONT_LARGE));
    let (_store, cx) = open_bar(cx, size(px(260.), px(238.)));
    let header = bounds(cx, "bar-header");
    let kebab = bounds(cx, "bar-kebab");
    cx.simulate_click(
        point(kebab.origin.x + kebab.size.width / 2., kebab.origin.y + kebab.size.height / 2.),
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let menu = bounds(cx, "kebab-menu");
    println!(
        "kebab: header {:?} menu {:?}",
        (header.origin, header.size),
        (menu.origin, menu.size),
    );
    assert!(
        close(menu.origin.x + menu.size.width, header.origin.x + header.size.width - px(12.)),
        "menu right edge must anchor 12px inside the header right edge",
    );
    assert!(close(menu.size.width, px(210.)), "menu keeps its 210px width");

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Structural canary for the menu layering: the bar paints its header (with
/// the absolutely-positioned kebab menu as the header's last child) before
/// the tags row, tabs row and panel. Without a deferred draw the later,
/// opaque panel siblings would paint over the open menu. This replica pins
/// the required paint order: panel first, menu last (on top).
#[gpui::test]
fn deferred_menu_paints_above_following_siblings(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    struct Layering(Rc<RefCell<Vec<&'static str>>>);
    impl Render for Layering {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let menu_order = self.0.clone();
            let panel_order = self.0.clone();
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(40.))
                        .relative()
                        .child(deferred(
                            div()
                                .absolute()
                                .top(px(30.))
                                .left(px(0.))
                                .size(px(40.))
                                .bg(gpui::black())
                                .child(canvas(
                                    |_, _, _| {},
                                    move |_, _, _, _| menu_order.borrow_mut().push("menu"),
                                )),
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .bg(gpui::black())
                        .child(canvas(
                            |_, _, _| {},
                            move |_, _, _, _| panel_order.borrow_mut().push("panel"),
                        )),
                )
        }
    }

    let order: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let (_view, cx) = cx.add_window_view(|_, _| Layering(order.clone()));
    // the window's first paint already ran; measure one clean frame
    order.borrow_mut().clear();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert_eq!(
        order.borrow().as_slice(),
        ["panel", "menu"],
        "the deferred menu must paint after (above) the following panel sibling",
    );
}
