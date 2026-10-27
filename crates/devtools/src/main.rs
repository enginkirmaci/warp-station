//! DevTools — the GPUI repository workbench.
// no console window behind the packaged Windows exe; debug builds keep it for logs
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use devtools::{assets, logging, store::Store, theme, ui::window::MainWindow};
use gpui::prelude::*;
use gpui::{px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use gpui_platform::application;

fn main() {
    // The Hyprland global-bind payload: ping the running instance's net
    // thread and exit before any logging/GPUI startup.
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(devtools_core::hotkey::COPY_PASSWORD_ARG) {
        if let Err(err) = devtools_core::hotkey::ping_copy_password() {
            eprintln!("devtools: clipboard password ping failed: {err:#}");
            std::process::exit(1);
        }
        return;
    }

    let _log_guard = logging::initialize();

    application()
        .with_assets(assets::EmbedAssets::new())
        .run(|cx: &mut App| {
            // kit components (input decorations, popovers) + our palette mapped
            // onto their theme tokens — must run before any kit component renders
            gpui_kit::init(cx);

            let (_net_handle, net_sender, event_rx) = devtools::net::spawn_net_thread();
            let store = cx.new(|cx| Store::new(cx, net_sender));
            theme::set_font_scale(store.read(cx).settings().general.font_scale.as_deref());
            theme::apply_kit_theme(cx);
            theme::init_system_font_scale(cx);

            // the companion watches this process (--parent) and exits when DevTools dies
            #[cfg(windows)]
            if store.read(cx).settings().snap_it.auto_start {
                if let Err(err) = devtools_core::snapit::start() {
                    tracing::warn!("SnapIt auto-start failed: {err}");
                }
            }

            // net events → store. Polled with a timer fallback: this gpui rev
            // does not service cross-thread channel wakeups on foreground tasks,
            // so a plain recv().await would sleep forever (async_channel recv is
            // cancel-safe, re-creating the future each lap is fine). A wake drains
            // the whole pending burst in one pass — one timer and one frame per
            // burst instead of one per event.
            let event_store = store.downgrade();
            cx.spawn(async move |cx| {
                loop {
                    let timeout = cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(200));
                    let mut recv = Box::pin(event_rx.recv());
                    match futures_util::future::select(recv.as_mut(), timeout).await {
                        futures_util::future::Either::Left((received, _)) => {
                            let Ok(first) = received else { break };
                            let Some(store) = event_store.upgrade() else { break };
                            store.update(cx, |s, cx| s.on_net_event(first, cx));
                            let mut batch = 1;
                            while batch < 64 {
                                match event_rx.try_recv() {
                                    Ok(event) => {
                                        store.update(cx, |s, cx| s.on_net_event(event, cx));
                                        batch += 1;
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                        futures_util::future::Either::Right((_, _)) => continue,
                    }
                }
            })
            .detach();

            // wand permission-ask countdown ticks once per second; the same
            // beat expires toasts past their ttl
            let wand_store = store.downgrade();
            cx.spawn(async move |cx| loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                if let Some(store) = wand_store.upgrade() {
                    store.update(cx, |s, cx| {
                        s.tick_wand(cx);
                        s.tick_notices(cx);
                    });
                } else {
                    break;
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // the bar's fixed 300px sidebar + repos list need a floor;
                    // below this the README column collapses and the Overview
                    // tiles/heading clip at the window edge
                    window_min_size: Some(size(px(720.), px(480.))),
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
                |window, cx| {
                    let window_store = store.clone();
                    cx.new(|cx| MainWindow::new(window_store, window, cx))
                },
            )
            .unwrap();

            cx.on_window_closed(|cx, _window_id| {
                // the notes window closing must not quit the app
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
