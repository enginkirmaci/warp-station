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

/// The Git card's auto-fetch picker: four segmented options render seeded from
/// the settings, and picking 30m + Save persists AutoFetchMinutes to the
/// settings file on disk. The store is scratch-backed; the probe also runs
/// under a temp HOME because Save reconciles the autostart registration, and
/// in a tall window so the Git card (third card down) is clickable.
#[gpui_kit::gpui::test]
fn auto_fetch_row_picks_and_persists(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!("devtools-auto-fetch-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".devtools")).unwrap();
    // SAFETY: process-global env; set before the store reads settings; the
    // sibling test tolerates a default-settings read while this runs
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let scratch = home.join(".devtools").join("settings.json");
    std::fs::write(&scratch, "{}").expect("scratch settings");

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(devtools_core::settings::SettingsService::at(scratch.clone())),
        )
    });
    let handle = cx.open_window(gpui::size(gpui::px(1400.), gpui::px(2400.)), |window, cx| {
        SettingsPage::new(store, window, cx)
    });
    let cx = gpui::VisualTestContext::from_window(handle.into(), cx).into_mut();
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.debug_bounds("auto-fetch-row").is_some(), "the auto fetch row renders");
    for option in ["auto-fetch-off", "auto-fetch-15", "auto-fetch-30", "auto-fetch-60"] {
        assert!(cx.debug_bounds(option).is_some(), "{option} renders");
    }

    let pick = cx.debug_bounds("auto-fetch-30").expect("30m button");
    cx.simulate_click(pick.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let save = cx.debug_bounds("settings-save").expect("save button");
    cx.simulate_click(save.center(), gpui::Modifiers::default());
    cx.run_until_parked();

    let json = std::fs::read_to_string(&scratch).expect("saved settings");
    assert!(
        json.contains("\"AutoFetchMinutes\": 30"),
        "the interval persisted (got {json})"
    );

    let _ = std::fs::remove_dir_all(&home);
}
