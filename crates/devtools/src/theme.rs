//! The DevTools palette: the SukiUI dark theme values from ThemeResources.axaml
//! plus the ChipPalette hues, mapped onto gpui-kit's theme tokens so kit
//! components (Input/Textarea/TextView/popovers) render in our colors.

use gpui::{hsla, App, Hsla, Rgba};

/// const-friendly rgba: gpui's `rgba()` is not const in this rev.
const fn c(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 24) & 0xFF) as f32 / 255.0,
        g: ((hex >> 16) & 0xFF) as f32 / 255.0,
        b: ((hex >> 8) & 0xFF) as f32 / 255.0,
        a: (hex & 0xFF) as f32 / 255.0,
    }
}

// ---- font resolution -------------------------------------------------------
// The brand families (Inter, DejaVu Sans Mono) are not installed everywhere;
// naming a missing family makes gpui fall back per text run and skips the
// kit's own probe (an explicitly-set family is used as-is). Resolve once at
// startup against the real font list so window, kit and code blocks agree.

const UI_FONT_CANDIDATES: &[&str] = &["Inter", "Adwaita Sans", "Segoe UI", "SF Pro Text", "Cantarell", "DejaVu Sans"];
const CODE_FONT_CANDIDATES: &[&str] = &["DejaVu Sans Mono", "Noto Sans Mono", "JetBrainsMono Nerd Font", "Liberation Mono", "Cascadia Mono", "Menlo", "Consolas", "Monaco"];
/// Virtual family gpui always resolves; the last resort for both stacks.
const SYSTEM_FONT: &str = ".SystemUIFont";

static UI_FONT_RESOLVED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static CODE_FONT_RESOLVED: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Pick the first installed family per stack; call once at startup, before
/// any text is laid out.
pub fn resolve_fonts(cx: &App) {
    let installed = cx.text_system().all_font_names();
    let pick = |candidates: &[&str]| {
        candidates
            .iter()
            .find(|c| installed.iter().any(|name| name == *c))
            .map(|c| c.to_string())
            .unwrap_or_else(|| SYSTEM_FONT.to_string())
    };
    let _ = UI_FONT_RESOLVED.set(pick(UI_FONT_CANDIDATES));
    let _ = CODE_FONT_RESOLVED.set(pick(CODE_FONT_CANDIDATES));
    tracing::info!("UI font {:?}, code font {:?}", ui_font(), code_font());
}

/// The UI family to lay everything out in.
pub fn ui_font() -> &'static str {
    UI_FONT_RESOLVED.get().map(|s| s.as_str()).unwrap_or(FONT_UI)
}

/// The mono family for code chips, patches and diffs.
pub fn code_font() -> &'static str {
    CODE_FONT_RESOLVED.get().map(|s| s.as_str()).unwrap_or(CODE_FONT)
}

// ---- gpui-kit theme mapping ------------------------------------------------

/// Map the DevTools palette onto gpui-kit's shadcn-style theme tokens. Call
/// after [`gpui_kit::init`], before opening windows (`Theme::update` refreshes
/// every window, so a future accent setting can re-run it).
pub fn apply_kit_theme(cx: &mut App) {
    use gpui_kit::component::{Theme, ThemeMode};

    resolve_fonts(cx);
    Theme::update(cx, |t| {
        t.mode = ThemeMode::Dark;
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
        t.input = INPUT.into();
        t.ring = ACCENT.into();
        t.selection = selection().into();
        t.caret = TEXT.into();
        t.link = ACCENT.into();
        t.popover = POPUP.into();
        t.popover_foreground = TEXT.into();
        t.sidebar = BACKGROUND.into();
        t.sidebar_foreground = TEXT.into();
        t.sidebar_border = HAIRLINE.into();

        t.radius = gpui::px(8.);
        t.radius_lg = gpui::px(12.);

        t.font_family = gpui::SharedString::from(ui_font());
        t.mono_font_family = gpui::SharedString::from(code_font());
        // the rem base: kit components size text/padding in rem, and every
        // window re-applies it on render (Root plugin prepare) — so this one
        // field scales Input/Textarea/Switch with the text-size variant
        t.font_size = gpui::px(16. * font_factor());
        t.mono_font_size = gpui::px(13. * font_factor());

        // the repos table blends into the page: no
        // near-black canvas, no zebra, no visible grid (the color fields
        // reconcile into the tokens)
        t.table = BACKGROUND.into();
        t.table_head = BACKGROUND.into();
        t.table_even = BACKGROUND.into();
        t.table_hover = HOVER;
        t.table_active = tint(ACCENT);
        t.table_active_border = ACCENT.into();
        t.table_row_border = BORDER_LIGHT.into();
        t.table_foot = BACKGROUND.into();
    });
}

// ---- surfaces (ThemeResources.axaml) ---------------------------------------

pub const BACKGROUND: Rgba = c(0x252525FF); // SukiBackground — window + sidebar
pub const STRONG_BACKGROUND: Rgba = c(0x292929FF); // SukiStrongBackground — bottom bar
pub const CARD: Rgba = c(0x222222FF); // SukiCardBackground — cards, table rows
pub const POPUP: Rgba = c(0x2B2B2BFF); // SukiPopupBackground — popovers/menus
pub const INPUT: Rgba = c(0x333333FF); // opaque glass — text fields
pub const CHIP: Rgba = c(0x3A3A3AFF); // neutral pill background
pub const HAIRLINE: Rgba = c(0x3F3F3FFF); // SukiBorderBrush
pub const BORDER_LIGHT: Rgba = c(0x333333FF);
pub const HOVER: Hsla = hsla(0.0, 0.0, 1.0, 0.08);

// ---- text ------------------------------------------------------------------

pub const TEXT: Rgba = c(0xEDFFFFFF); // SukiText (white at 0.93)
pub const TEXT_DIM: Rgba = c(0xCAFFFFFF); // SukiLowText — secondary body text
pub const TEXT_MUTED: Rgba = c(0xB4B4B4FF); // SukiMuteText — labels, metadata
pub const TEXT_FAINT: Rgba = c(0x727272FF); // hints, quietest meta

/// Text selection in fields: the accent at 25% alpha.
pub fn selection() -> Rgba {
    Rgba { r: ACCENT.r, g: ACCENT.g, b: ACCENT.b, a: 0.25 }
}

// ---- accent ----------------------------------------------------------------

/// The Suki accent override from App.xaml.cs.
pub const ACCENT: Rgba = c(0x7090CFFF);
pub const ON_ACCENT: Rgba = c(0xFFFFFFFF);

// ---- fixed semantic colors ---------------------------------------------------

pub const DANGER: Rgba = c(0xF5222DFF); // ChipPalette red
pub const WARN: Rgba = c(0xFA8C16FF); // amber
pub const SUCCESS_BRIGHT: Rgba = c(0x4EC94EFF); // ChipPalette bright green
pub const AMBER_BRIGHT: Rgba = c(0xFFAB40FF);
pub const PURPLE_BRIGHT: Rgba = c(0xA78BFAFF);
pub const BLUE_BRIGHT: Rgba = c(0x6E9BF7FF);
pub const GOLD_BRIGHT: Rgba = c(0xF5B400FF); // favorite star, active
pub const STAR_MUTED: Rgba = c(0x8B90A0FF); // favorite star, inactive

/// Tinted chip background for a bright hue (ChipBrushes TintAlpha).
pub fn tint(color: Rgba) -> Hsla {
    let h: Hsla = color.into();
    Hsla { a: 0.18, ..h }
}

// ---- diffs (renderer arrives with the Changes tab) --------------------------

pub const DIFF_GUTTER: Rgba = c(0xB4B4B49E);
pub const DIFF_ADD_BG: Hsla = hsla(0.30, 0.55, 0.5, 0.15);
pub const DIFF_DEL_BG: Hsla = hsla(0.01, 0.85, 0.6, 0.15);
pub const DIFF_HUNK_BG: Hsla = hsla(0.0, 0.0, 0.6, 0.10);

// ---- font scale ------------------------------------------------------------

/// The settings FontScale values ("compact" | "normal" | "large").
pub const FONT_COMPACT: &str = "compact";
pub const FONT_NORMAL: &str = "normal";
pub const FONT_LARGE: &str = "large";

pub fn normalize_font_scale(scale: Option<&str>) -> &'static str {
    match scale {
        Some(s) if s == FONT_COMPACT => FONT_COMPACT,
        Some(s) if s == FONT_LARGE => FONT_LARGE,
        _ => FONT_NORMAL,
    }
}

fn factor_bits(scale: Option<&str>) -> u32 {
    f32::to_bits(match normalize_font_scale(scale) {
        FONT_COMPACT => 0.85,
        FONT_LARGE => 1.15,
        _ => 1.0,
    })
}

static USER_FACTOR: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0); // 0 = factor 1.0, set from settings at startup
static SYSTEM_FACTOR: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0); // 0 = factor 1.0, gsettings text-scaling

fn font_factor() -> f32 {
    let bits = |v: u32| if v == 0 { 1.0 } else { f32::from_bits(v) };
    bits(USER_FACTOR.load(std::sync::atomic::Ordering::Relaxed))
        * bits(SYSTEM_FACTOR.load(std::sync::atomic::Ordering::Relaxed))
}

/// Applies the settings text-size variant; call before [`apply_kit_theme`] so
/// the first frame is already sized, and again on save.
pub fn set_font_scale(scale: Option<&str>) {
    USER_FACTOR.store(factor_bits(scale), std::sync::atomic::Ordering::Relaxed);
}

/// Token getters; the bases are the sizes the const era pinned. The kit rem
/// base (apply_kit_theme) rides the same factor.
fn scaled(v: f32) -> f32 {
    (v * font_factor() * 100.0).round() / 100.0
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

pub const CODE_FONT: &str = "DejaVu Sans Mono";
pub const FONT_UI: &str = "Inter";

static SYSTEM_FACTOR_TX: std::sync::OnceLock<async_channel::Sender<f32>> = std::sync::OnceLock::new();

/// Follows the desktop text scaling GTK apps honor — gsettings
/// org.gnome.desktop.interface text-scaling-factor. The initial value applies
/// synchronously; the monitor feeds changes that re-run the kit theme (which
/// refreshes every window). Without gsettings the factor stays 1.0.
pub fn init_system_font_scale(cx: &mut App) {
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

fn set_system_factor(factor: f32, cx: &mut App) {
    let bits = f32::to_bits(factor);
    if SYSTEM_FACTOR.swap(bits, std::sync::atomic::Ordering::Relaxed) != bits {
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

