//! DevTools — the Rust/GPUI port of the Avalonia repository workbench.
// no console window behind the packaged Windows exe; debug builds keep it for logs
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod logging;
mod store;
mod theme;
mod ui;

use gpui::prelude::*;
use gpui::{px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use gpui_platform::application;
use store::Store;
use ui::window::MainWindow;

fn main() {
    let _log_guard = logging::initialize();

    application()
        .with_assets(assets::EmbedAssets::new())
        .run(|cx: &mut App| {
            // kit components (input decorations, popovers) + our palette mapped
            // onto their theme tokens — must run before any kit component renders
            gpui_kit::init(cx);

            let store = cx.new(|_| Store::new());
            theme::apply_kit_theme(cx);

            let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // empty by default in GPUI — needed so Hyprland/launchers match
                    // StartupWMClass=devtools in the desktop entry
                    app_id: Some("devtools".into()),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Dev Tools".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    let window_store = store.clone();
                    cx.new(|cx| MainWindow::new(window_store, cx))
                },
            )
            .unwrap();

            cx.on_window_closed(|cx, _window_id| cx.quit()).detach();
            cx.activate(true);
        });
}
