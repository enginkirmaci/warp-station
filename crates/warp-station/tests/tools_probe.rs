//! Headless draw probes for the tool drawers: two draws without a panic means
//! the drawer trees are sound (mirrors the settings/settings_probe harness).

use warp_station::store::Store;
use warp_station::ui::clipboard_password::ClipboardPasswordDrawer;
use warp_station::ui::nuget::NugetDrawer;
use gpui::TestAppContext;
use gpui::prelude::*;

#[gpui_kit::gpui::test]
fn clipboard_password_drawer_draws(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let (_handle, cx) =
        cx.add_window_view(|window, cx| ClipboardPasswordDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));
}

#[gpui_kit::gpui::test]
fn nuget_drawer_draws(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let probe_store = store.clone();
    let (_handle, cx) = cx.add_window_view(|window, cx| NugetDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    // no watch is running just from opening the drawer
    assert!(!cx.update(|_, cx| probe_store.read(cx).nuget.watching));
}

#[gpui_kit::gpui::test]
fn main_window_draws_with_tools_menu(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    // SAFETY: single-threaded test setup, read once at MainWindow::new
    unsafe { std::env::set_var("WARP_STATION_TOOLS_MENU", "1") };
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let (_handle, cx) =
        cx.add_window_view(|window, cx| warp_station::ui::window::MainWindow::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));
    unsafe { std::env::remove_var("WARP_STATION_TOOLS_MENU") };
}
