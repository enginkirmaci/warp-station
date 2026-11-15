//! File logging: readable lines into a daily-rolling file under ~/.warp/logs.
//! Best effort — an unwritable home keeps stderr logging.

use warp_station_core::paths::AppPaths;

pub fn initialize() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    warpui_kit_platform::logging::init(&AppPaths::logs_dir(), "warp-station-")
}

/// A panic must leave a line in the log: a desktop launch has no terminal, so
/// the default hook's stderr goes nowhere. This logs the thread, the
/// location, the payload and a captured backtrace, then runs the default hook
/// so a terminal launch still prints what it always did.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown location".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic");
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        tracing::error!("panic in {thread} at {location}: {payload}");
        tracing::error!(
            "panic backtrace:\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        default(info);
    }));
}
