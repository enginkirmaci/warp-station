//! Headless draw probe for the Settings page: seeded from the real settings
//! file, two draws without a panic means the card/field/switch tree is sound.

use devtools::store::Store;
use devtools::ui::settings::SettingsPage;
use gpui::prelude::*;
use gpui::TestAppContext;

#[gpui_kit::gpui::test]
fn settings_page_draws_all_sections(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let (_window_handle, cx) = cx.add_window_view(|window, cx| SettingsPage::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.update(|_, _| devtools_core::settings::SettingsService::open().get().repos.max_scan_depth) >= 1);
}
