//! Headless draw probe for the Settings page: seeded from the real settings
//! file, two draws without a panic means the card/field/switch tree is sound.

use devtools::net::{NetCommand, TemplateOp};
use devtools::store::Store;
use devtools::ui::settings::SettingsPage;
use gpui::prelude::*;
use gpui::TestAppContext;

/// Serializes the two HOME-swapping probes against each other (the plain
/// draws test never writes and tolerates either HOME).
static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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

/// The OpenCode card's prompt-template manager. Templates live under the
/// scratch HOME: the three rows render un-customized, a body modified
/// straight on disk badges after the page re-opens, Edit round-trips
/// TemplateIo Read and swaps in the editor (a TemplateLoaded hand-set on the
/// store seeds it), Cancel returns to the rows, and Reset is two-click armed
/// with the confirm landing on the net channel.
#[gpui_kit::gpui::test]
fn prompt_templates_list_edit_reset(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!("devtools-template-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".devtools")).unwrap();
    // SAFETY: process-global env; serialized against the sibling HOME probe
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let handle = cx.open_window(gpui::size(gpui::px(1400.), gpui::px(2600.)), |window, cx| {
        SettingsPage::new(store.clone(), window, cx)
    });
    let mut wcx = gpui::VisualTestContext::from_window(handle.into(), cx);
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));

    // the three registry rows render; none is customized yet
    for selector in [
        "template-row-commit-message",
        "template-row-readme",
        "template-row-daily-summary-all",
    ] {
        assert!(wcx.debug_bounds(selector).is_some(), "{selector} renders");
    }
    assert!(wcx.debug_bounds("template-customized-readme").is_none());

    // a body modified straight on disk badges once a fresh page re-opens
    std::fs::create_dir_all(home.join(".devtools").join("opencode")).unwrap();
    std::fs::write(home.join(".devtools").join("opencode").join("readme.md"), "custom {repo_name}")
        .expect("seed a customized body");
    let reopened = cx.open_window(gpui::size(gpui::px(1400.), gpui::px(2600.)), |window, cx| {
        SettingsPage::new(store.clone(), window, cx)
    });
    let mut wcx = gpui::VisualTestContext::from_window(reopened.into(), cx);
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    assert!(wcx.debug_bounds("template-customized-readme").is_some(), "disk edit badges");
    assert!(wcx.debug_bounds("template-customized-commit-message").is_none());

    // Edit swaps the section for the embedded editor and asks for the body
    click(&mut wcx, "template-edit-readme");
    assert!(wcx.debug_bounds("template-editor-readme").is_some(), "the editor view renders");
    assert!(wcx.debug_bounds("template-row-readme").is_none(), "the rows are gone");
    assert!(
        matches!(
            last_command(&net_rx),
            Some(NetCommand::TemplateIo { id, op: TemplateOp::Read }) if id == "readme"
        ),
        "Edit sends a readme TemplateIo Read"
    );

    // the async gap: the editor draws empty, then TemplateLoaded landing on
    // the store seeds the buffer (two draws, no panic)
    wcx.update(|window, cx| drop(window.draw(cx)));
    store.update(&mut wcx, |s, _| {
        s.template_loaded = Some(("readme".into(), "custom {repo_name}".into(), true));
    });
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    wcx.update(|window, cx| drop(window.draw(cx)));

    // Cancel exits without writing anything
    click(&mut wcx, "template-cancel-readme");
    assert!(wcx.debug_bounds("template-editor-readme").is_none(), "cancel exits the editor");
    assert!(wcx.debug_bounds("template-row-readme").is_some(), "the rows are back");

    // Reset is two-click armed; only the confirm reaches the net channel
    click(&mut wcx, "template-reset-readme");
    assert!(net_rx.is_empty(), "the arming click sends nothing");
    click(&mut wcx, "template-reset-readme");
    assert!(
        matches!(
            last_command(&net_rx),
            Some(NetCommand::TemplateIo { id, op: TemplateOp::Reset }) if id == "readme"
        ),
        "the confirm sends a readme TemplateIo Reset"
    );

    let _ = std::fs::remove_dir_all(&home);
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} missing"));
    cx.simulate_click(
        gpui::Point {
            x: bounds.origin.x + bounds.size.width / 2.,
            y: bounds.origin.y + bounds.size.height / 2.,
        },
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

/// Drains the net channel and returns the last command (None when idle).
fn last_command(net_rx: &async_channel::Receiver<NetCommand>) -> Option<NetCommand> {
    let mut last = None;
    while let Ok(command) = net_rx.try_recv() {
        last = Some(command);
    }
    last
}
