//! The DevTools theme: the family palette and fonts from warpui-kit-core,
//! re-exported under the app's `theme::` paths, plus the app-only layout
//! constants, the fixed Suki accent mapping and the desktop text-scaling
//! follow (gsettings org.gnome.desktop.interface text-scaling-factor).

pub use warpui_kit_core::fonts::{
    code_font, normalize_font_scale, resolve_fonts, ui_font, CODE_FONT,
    FONT_COMPACT, FONT_LARGE, FONT_NORMAL, FONT_UI,
};
pub use warpui_kit_core::palette;
// every shared color const stays reachable as `theme::X`
pub use warpui_kit_core::palette::*;

// ---- gpui-kit theme mapping ------------------------------------------------

/// Map the DevTools palette onto gpui-kit's shadcn-style theme tokens. Call
/// after [`gpui_kit::init`], before opening windows (`Theme::update` refreshes
/// every window, so a settings save can re-run it).
pub fn apply_kit_theme(cx: &mut gpui::App) {
    let accent = palette::AccentColors::for_preset(palette::normalize_accent(Some("blue")));
    warpui_kit_core::apply_kit_theme(
        accent,
        &font_tokens(),
        palette::BACKGROUND,
        cx,
    );
}

/// The tokens the kit mapping and shared shells ride: the settings text-size
/// variant × the desktop text-scaling factor.
pub fn font_tokens() -> warpui_kit_core::FontTokens {
    let scale = match FACTOR_SCALE.load(std::sync::atomic::Ordering::Relaxed) {
        SCALE_COMPACT => Some(FONT_COMPACT),
        SCALE_LARGE => Some(FONT_LARGE),
        _ => None,
    };
    warpui_kit_core::font_tokens(scale, warpui_kit_core::system_factor())
}

/// Scale enum stored alongside the factor bits (`set_font_scale` needs the
/// string back when the kit mapping rebuilds the token ladder).
const SCALE_NORMAL: u8 = 0;
const SCALE_COMPACT: u8 = 1;
const SCALE_LARGE: u8 = 2;

static FACTOR_SCALE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(SCALE_NORMAL);

/// Applies the settings text-size variant; call before [`apply_kit_theme`] so
/// the first frame is already sized, and again on save.
pub fn set_font_scale(scale: Option<&str>) {
    let code = match warpui_kit_core::normalize_font_scale(scale) {
        FONT_COMPACT => SCALE_COMPACT,
        FONT_LARGE => SCALE_LARGE,
        _ => SCALE_NORMAL,
    };
    FACTOR_SCALE.store(code, std::sync::atomic::Ordering::Relaxed);
    warpui_kit_core::set_font_scale(scale);
}

// ---- token getters ---------------------------------------------------------
// The bases are the sizes the const era pinned; the kit rem base rides the
// same factor.

fn scaled(v: f32) -> f32 {
    (v * warpui_kit_core::font_factor() * 100.0).round() / 100.0
}

pub fn font_small() -> f32 {
    scaled(12.0)
}

pub fn font_body() -> f32 {
    scaled(13.0)
}

pub fn font_title() -> f32 {
    scaled(15.0)
}

pub fn icon_small() -> f32 {
    scaled(13.0)
}

pub fn icon_medium() -> f32 {
    scaled(15.0)
}

pub fn icon_large() -> f32 {
    scaled(17.0)
}

// ---- layout ----------------------------------------------------------------
// Shared chrome sizes. Views and the window position their overlays from
// these, so a change here keeps menus and popups anchored.

/// Custom titlebar height (window buttons + search field live here).
pub const TITLEBAR_H: f32 = 38.0;
/// One repos toolbar row (title row / filter row).
pub const TOOLBAR_H: f32 = 46.0;
/// Compact column-header strip above the repos table.
pub const TABLE_HEAD_H: f32 = 28.0;
/// Repos table row height (fits the two-line name + folder path cell).
pub const TABLE_ROW_H: f32 = 48.0;

// ---- desktop text scaling --------------------------------------------------

static SYSTEM_FACTOR_TX: std::sync::OnceLock<async_channel::Sender<f32>> = std::sync::OnceLock::new();

/// Follows the desktop text scaling GTK apps honor — gsettings
/// org.gnome.desktop.interface text-scaling-factor. The initial value applies
/// synchronously; the monitor feeds changes that re-run the kit theme (which
/// refreshes every window). Without gsettings the factor stays 1.0.
pub fn init_system_font_scale(cx: &mut gpui::App) {
    if let Some(factor) = read_system_factor() {
        set_system_factor(factor, cx);
    } else {
        tracing::debug!("system text scaling unreadable; keeping factor 1.0");
    }
    let (tx, rx) = async_channel::bounded::<f32>(16);
    if SYSTEM_FACTOR_TX.set(tx).is_err() {
        return;
    }
    spawn_factor_monitor();
    cx.spawn(async move |cx| {
        while let Ok(factor) = rx.recv().await {
            cx.update(|cx| set_system_factor(factor, cx));
        }
    })
    .detach();
}

/// gsettings exists only on Linux desktops; elsewhere the factor stays at
/// whatever the font-scale setting applies.
#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_factor_monitor() {
    match std::process::Command::new("gsettings")
        .args(["monitor", "org.gnome.desktop.interface"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::Builder::new()
                .name("gsettings".into())
                .spawn(move || {
                    use std::io::BufRead;
                    let Some(stdout) = child.stdout.take() else { return };
                    for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                        if let Some(factor) = parse_system_factor(&line)
                            && let Some(tx) = SYSTEM_FACTOR_TX.get()
                        {
                            let _ = tx.send_blocking(factor);
                        }
                    }
                    let _ = child.wait();
                })
                .ok();
        }
        Err(_) => {
            tracing::debug!("gsettings monitor unavailable; system text scale frozen at startup value");
        }
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn spawn_factor_monitor() {}

fn set_system_factor(factor: f32, cx: &mut gpui::App) {
    if warpui_kit_core::system_factor() != factor {
        warpui_kit_core::set_system_factor(factor);
        apply_kit_theme(cx);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_system_factor() -> Option<f32> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "text-scaling-factor"])
        .output()
        .ok()?;
    parse_system_factor(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn read_system_factor() -> Option<f32> {
    None
}

/// Accepts bare "1.1667" (gsettings get) and "key: 1.1667" (monitor) lines.
#[cfg(all(unix, not(target_os = "macos")))]
fn parse_system_factor(text: &str) -> Option<f32> {
    let key = "text-scaling-factor";
    let text = match text.find(key) {
        Some(idx) => &text[idx + key.len()..],
        None => text,
    };
    let text = text.trim_start_matches([':', ' ', '\t']).trim_end();
    text.parse::<f32>().ok().filter(|v| *v > 0.0)
}
