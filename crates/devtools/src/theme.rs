//! The DevTools theme: the app's own palette (transcribed from the Split
//! inspector mockup), re-exported under the app's `theme::` paths, plus the
//! app-only layout constants and the desktop text-scaling follow (gsettings
//! org.gnome.desktop.interface text-scaling-factor).
//!
//! The palette is app-local on purpose. `warpui-kit-core` still owns the
//! family ramp, but warp-agent and warp-notes consume it too, so this app's
//! redesign must not move their colours; the local constants below shadow the
//! shared ones (`pub use ...::*` is shadowed by an explicit item) while the
//! rest of the family palette stays reachable for anything not redesigned yet.

pub use warpui_kit_core::fonts::{
    CODE_FONT, FONT_COMPACT, FONT_LARGE, FONT_NORMAL, FONT_UI, normalize_font_scale, resolve_fonts,
};
pub use warpui_kit_core::palette;
// the family constants stay reachable as `theme::X`; the ones below override
// the redesigned surfaces, text and accents
pub use warpui_kit_core::palette::*;

use gpui::Rgba;

// ---- fonts -----------------------------------------------------------------
// The design is set in IBM Plex. The faces ship inside the binary and are
// registered at startup, so these resolve to them whenever gpui accepted the
// files; the family chain from warpui-kit-core is the fallback (and the only
// option in probe processes, which skip the font registration).

static APP_UI_FONT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
static APP_CODE_FONT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// The UI family everything is laid out in.
pub fn ui_font() -> &'static str {
    APP_UI_FONT
        .get()
        .and_then(|font| font.as_deref())
        .unwrap_or_else(warpui_kit_core::fonts::ui_font)
}

/// The mono family for code chips, patches and diffs.
pub fn code_font() -> &'static str {
    APP_CODE_FONT
        .get()
        .and_then(|font| font.as_deref())
        .unwrap_or_else(warpui_kit_core::fonts::code_font)
}

/// Resolve the brand families against what the text system really has; call
/// after the bundled fonts are added and before [`apply_kit_theme`].
pub fn resolve_brand_fonts(cx: &gpui::App) {
    let installed = cx.text_system().all_font_names();
    let pick = |family: &str| {
        installed
            .iter()
            .any(|name| name == family)
            .then(|| family.to_string())
    };
    let _ = APP_UI_FONT.set(pick("IBM Plex Sans"));
    let _ = APP_CODE_FONT.set(pick("IBM Plex Mono"));
    tracing::info!(
        "fonts: {} families installed, ui {:?}, code {:?}",
        installed.len(),
        ui_font(),
        code_font()
    );
}

/// const-friendly rgba with gpui's 0xRRGGBBAA byte order.
pub const fn rgba(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 24) & 0xFF) as f32 / 255.,
        g: ((hex >> 16) & 0xFF) as f32 / 255.,
        b: ((hex >> 8) & 0xFF) as f32 / 255.,
        a: (hex & 0xFF) as f32 / 255.,
    }
}

// ---- surfaces --------------------------------------------------------------
// One near-black ramp: page/shell < card < field, every step two or three
// points apart, so a card reads as raised without a heavy border.

pub const BACKGROUND: Rgba = rgba(0x0C0E11FF);
pub const STRONG_BACKGROUND: Rgba = BACKGROUND;
pub const CARD: Rgba = rgba(0x0F1216FF);
pub const POPUP: Rgba = rgba(0x12161CFF);
pub const INPUT: Rgba = rgba(0x12161CFF);
pub const CHIP: Rgba = rgba(0x151A21FF);
pub const HAIRLINE: Rgba = rgba(0x20262EFF);
pub const BORDER_LIGHT: Rgba = rgba(0x1A1F26FF);
/// Stronger separators: the rail's sync card and the tag pills.
pub const LINE_STRONG: Rgba = rgba(0x262D37FF);
/// The selected list row: one accent-tinted step above the card ramp.
pub const SELECTED: Rgba = rgba(0x141B25FF);
/// Row/pill hover fill: the mockup's opaque `#151a21` instead of a white wash,
/// so hover reads the same over any surface step.
pub const HOVER_BG: Rgba = rgba(0x151A21FF);
pub const HOVER: gpui::Hsla = gpui::hsla(0.0, 0.0, 1.0, 0.045);

// ---- chrome steps the mockup names explicitly ------------------------------
// The titlebar, the segmented control's trough, the folder band, the sync
// card and the footer all sit on their own step of the same ramp; sampling
// them out of the export keeps the layers identical.

pub const TOPBAR_BG: Rgba = rgba(0x0B0D10FF);
pub const SEARCH_BG: Rgba = rgba(0x090B0EFF);
pub const SEARCH_BORDER: Rgba = rgba(0x252C36FF);
pub const GHOST_BG: Rgba = rgba(0x0F1318FF);
pub const GHOST_BORDER: Rgba = rgba(0x2A313BFF);
pub const GHOST_BORDER_HOVER: Rgba = rgba(0x343D49FF);
pub const SEG_BG: Rgba = rgba(0x0A0C0FFF);
pub const SEG_BORDER: Rgba = rgba(0x232A33FF);
pub const SEG_ACTIVE: Rgba = rgba(0x1A2330FF);
pub const GROUP_BG: Rgba = rgba(0x0D1014FF);
pub const SYNC_BG: Rgba = rgba(0x0B0E12FF);
pub const FOOT_BG: Rgba = rgba(0x0A0C0FFF);
pub const KEY_BG: Rgba = rgba(0x11151AFF);
pub const KEY_BORDER: Rgba = rgba(0x2A323DFF);
pub const KEY_BORDER_BOTTOM: Rgba = rgba(0x3A4451FF);
pub const KEY_FG: Rgba = rgba(0xC0C9D5FF);
pub const ROW_LINE: Rgba = rgba(0x181D24FF);
pub const ROWS_RULE: Rgba = rgba(0x1F252DFF);
pub const MUTED_ICON: Rgba = rgba(0x8D97A4FF);
pub const ACCENT_HOVER: Rgba = rgba(0x245BC7FF);
pub const ACCENT_BORDER: Rgba = rgba(0x3B78E8FF);
pub const CLOSE_HOVER: Rgba = rgba(0xA33B3BFF);

// ---- text ------------------------------------------------------------------
// Three steps plus the faintest hint: hierarchy comes from weight and size,
// not from a long grey ladder.

pub const TEXT: Rgba = rgba(0xE9ECF1FF);
pub const TEXT_DIM: Rgba = rgba(0xA7AFBBFF);
pub const TEXT_MUTED: Rgba = rgba(0x818B99FF);
pub const TEXT_FAINT: Rgba = rgba(0x6E7987FF);

// ---- accents ---------------------------------------------------------------
// One blue for the single filled action, a lighter blue for links/focus, amber
// for "needs attention", the two diff tints.

pub const ACCENT: Rgba = rgba(0x2A68DFFF);
pub const ON_ACCENT: Rgba = rgba(0xFFFFFFFF);
pub const BLUE_BRIGHT: Rgba = rgba(0x78A9FFFF);
pub const AMBER_BRIGHT: Rgba = rgba(0xEFB264FF);
pub const WARN: Rgba = rgba(0xEFB264FF);
pub const GOLD_BRIGHT: Rgba = rgba(0xEFB264FF);
pub const STAR_MUTED: Rgba = rgba(0x7F8997FF);
pub const STAR_ON: Rgba = rgba(0xC8D2E0FF);
pub const SEG_ACTIVE_FG: Rgba = rgba(0xDCE7F8FF);
pub const SUCCESS_BRIGHT: Rgba = rgba(0x64D2A2FF);
pub const SUCCESS_DEEP: Rgba = rgba(0x3AA97BFF);
pub const DANGER: Rgba = rgba(0xF08A8AFF);
pub const ADD: Rgba = SUCCESS_BRIGHT;
pub const DEL: Rgba = DANGER;
pub const PURPLE_BRIGHT: Rgba = rgba(0xA78BFAFF);

pub const DIFF_ADD_BG: gpui::Hsla = gpui::hsla(0.42, 0.45, 0.45, 0.18);
pub const DIFF_DEL_BG: gpui::Hsla = gpui::hsla(0.0, 0.65, 0.68, 0.16);

// ---- gpui-kit theme mapping ------------------------------------------------

/// Map the app palette onto gpui-kit's shadcn-style theme tokens. Call after
/// [`gpui_kit::init`], before opening windows (`Theme::update` refreshes every
/// window, so a settings save can re-run it).
///
/// This mirrors `warpui_kit_core::apply_kit_theme` but reads the local
/// palette: the shared mapping is pinned to the family ramp, which this app no
/// longer uses.
pub fn apply_kit_theme(cx: &mut gpui::App) {
    use gpui_kit::component::{Theme, ThemeMode};

    resolve_fonts(cx);
    // the bundled IBM Plex faces win over the family chain when present
    resolve_brand_fonts(cx);
    // A mode flip inside the color-edit closure makes the kit reload its
    // registered theme's colors AFTER the closure, wiping every override —
    // flip first, then edit.
    Theme::update(cx, |t| t.mode = ThemeMode::Dark);
    let tokens = font_tokens();
    Theme::update(cx, |t| {
        t.background = BACKGROUND.into();
        t.foreground = TEXT.into();
        t.primary = ACCENT.into();
        t.primary_foreground = ON_ACCENT.into();
        t.secondary = CHIP.into();
        t.secondary_foreground = TEXT.into();
        t.muted = INPUT.into();
        t.muted_foreground = TEXT_MUTED.into();
        t.accent = HOVER;
        t.accent_foreground = TEXT.into();
        t.danger = DANGER.into();
        t.danger_foreground = ON_ACCENT.into();
        t.border = HAIRLINE.into();
        // the kit paints an input's border with `input` and its fill with
        // `input` mixed over the surface, so a fill-coloured token made the
        // border vanish on popups; the hairline gives fields a real edge
        t.input = HAIRLINE.into();
        t.ring = BLUE_BRIGHT.into();
        t.selection = palette::selection(&accent_colors()).into();
        t.caret = TEXT.into();
        t.link = BLUE_BRIGHT.into();
        t.popover = POPUP.into();
        t.popover_foreground = TEXT.into();
        t.sidebar = BACKGROUND.into();
        t.sidebar_foreground = TEXT.into();
        t.sidebar_border = HAIRLINE.into();

        // controls on 6px corners, panels on 8 — one radius per class
        t.radius = gpui::px(6.);
        t.radius_lg = gpui::px(8.);

        t.font_family = gpui::SharedString::from(ui_font());
        t.mono_font_family = gpui::SharedString::from(code_font());
        // the rem base: kit components size text/padding in rem — ride the
        // same factor as our own tokens (font_small is the 12px base)
        let factor = tokens.font_small / 12.0;
        t.font_size = gpui::px(16. * factor);
        t.mono_font_size = gpui::px(13. * factor);

        t.highlight_theme = palette::highlight_theme();

        // scrollbars ride the same ramp: no stock light-grey thumb on a
        // near-black page
        t.tokens.scrollbar = (gpui::Hsla {
            a: 0.0,
            ..gpui::Hsla::from(TEXT_MUTED)
        })
        .into();
        t.tokens.scrollbar_thumb = gpui::Hsla::from(TEXT_FAINT).into();
        t.tokens.scrollbar_thumb_hover = gpui::Hsla::from(TEXT_MUTED).into();

        // tables blend into the page: no zebra, no visible grid
        t.table = CARD.into();
        t.table_head = CARD.into();
        t.table_head_foreground = TEXT_FAINT.into();
        t.table_even = CARD.into();
        t.table_hover = HOVER;
        t.table_active = palette::tint(ACCENT);
        t.table_active_border = ACCENT.into();
        t.table_row_border = BORDER_LIGHT.into();
        t.table_foot = CARD.into();
    });
}

/// The one accent family the app paints with; the preset system is not wired
/// to the redesign yet, so this is the mockup's blue.
fn accent_colors() -> palette::AccentColors {
    palette::AccentColors {
        accent: ACCENT,
        dim: BLUE_BRIGHT,
        on: ON_ACCENT,
        success_dim: SUCCESS_DEEP,
    }
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

/// The redesigned screens' scale: 10 label / 11 meta / 18 rail title. They
/// ride the same factor, so the settings text size still reaches them.
pub fn font_label() -> f32 {
    scaled(10.0)
}

pub fn font_meta() -> f32 {
    scaled(11.0)
}

pub fn font_rail_title() -> f32 {
    scaled(18.0)
}

// The export's exact scale on the main screen: 8 section label / 9 mono meta /
// 9.5 row meta / 10 controls / 10.5 sub / 11.5 launcher / 12.5 repo name /
// 13 brand / 20 titles.
pub fn font_micro() -> f32 {
    scaled(8.0)
}

pub fn font_mono_meta() -> f32 {
    scaled(9.0)
}

pub fn font_row_meta() -> f32 {
    scaled(9.5)
}

pub fn font_control() -> f32 {
    scaled(10.0)
}

pub fn font_sub() -> f32 {
    scaled(10.5)
}

pub fn font_launcher() -> f32 {
    scaled(11.5)
}

pub fn font_repo_name() -> f32 {
    scaled(12.5)
}

pub fn font_brand() -> f32 {
    scaled(13.0)
}

pub fn font_screen_title() -> f32 {
    scaled(20.0)
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
pub const TITLEBAR_H: f32 = 52.0;
/// One repos toolbar row (title row / filter row).
pub const TOOLBAR_H: f32 = 46.0;
/// Compact column-header strip above the repos table.
pub const TABLE_HEAD_H: f32 = 28.0;
/// Repos table row height (fits the two-line name + folder path cell).
pub const TABLE_ROW_H: f32 = 48.0;

// ---- desktop text scaling --------------------------------------------------

static SYSTEM_FACTOR_TX: std::sync::OnceLock<async_channel::Sender<f32>> =
    std::sync::OnceLock::new();

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
                    let Some(stdout) = child.stdout.take() else {
                        return;
                    };
                    for line in std::io::BufReader::new(stdout)
                        .lines()
                        .map_while(Result::ok)
                    {
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
            tracing::debug!(
                "gsettings monitor unavailable; system text scale frozen at startup value"
            );
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
