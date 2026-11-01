//! The kit theme must carry the DevTools palette after apply_kit_theme — a
//! Light→Dark mode flip inside the color-edit closure makes gpui-component
//! reload its stock dark theme over the overrides (near-black table canvas).
//! The bundled IBM Plex faces must also be present and parse.

use devtools::theme;
use gpui::TestAppContext;

#[gpui_kit::gpui::test]
fn palette_survives_the_mode_flip(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(theme::apply_kit_theme);

    cx.update(|cx| {
        use gpui_kit::component::{Theme, ThemeMode};
        let t = Theme::global(cx);
        assert_eq!(t.mode, ThemeMode::Dark);
        assert_eq!(t.background, gpui::Hsla::from(theme::BACKGROUND));
        // the redesigned ramp: tables are cards on the near-black page, not
        // the page itself
        assert_eq!(t.table, gpui::Hsla::from(theme::CARD));
        assert_eq!(t.popover, gpui::Hsla::from(theme::POPUP));
        assert_eq!(t.primary, gpui::Hsla::from(theme::ACCENT));
        assert_eq!(t.link, gpui::Hsla::from(theme::BLUE_BRIGHT));
    });
}

#[gpui_kit::gpui::test]
fn diffs_ride_the_app_palette(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(theme::apply_kit_theme);

    cx.update(|cx| {
        use gpui_kit::component::Theme;
        use warpui_kit_diff::DiffPalette;

        // without an installed palette the renderer falls back to the family
        // ramp and the app's diff tokens silently go unused
        let installed = *cx.global::<DiffPalette>();
        assert_eq!(installed.code_fg, theme::TEXT);
        assert_eq!(installed.muted_fg, theme::TEXT_MUTED);
        assert_eq!(installed.gutter, theme::DIFF_GUTTER);
        assert_eq!(installed.add, theme::ADD);
        assert_eq!(installed.del, theme::DEL);
        assert_eq!(installed.add_bg, theme::DIFF_ADD_BG);
        assert_eq!(installed.del_bg, theme::DIFF_DEL_BG);
        assert_eq!(installed.hunk_bg, theme::DIFF_HUNK_BG);

        // the renderer reads its font and token colors off the kit theme
        let t = Theme::global(cx);
        assert_eq!(t.mono_font_family.as_ref(), theme::code_font());
        assert!(std::sync::Arc::ptr_eq(
            &t.highlight_theme,
            &theme::highlight_theme()
        ));
    });
}

/// The token theme is a JSON literal of hex values, so this pins them to the
/// palette; an unparsable literal fails too, the family fallback has other hues.
#[test]
fn syntax_tokens_use_the_app_hues() {
    let tokens = theme::highlight_theme();
    let color = |name: &str| tokens.style(name).and_then(|style| style.color);

    assert_eq!(color("variable"), Some(theme::TEXT.into()));
    assert_eq!(color("comment"), Some(theme::TEXT_FAINT.into()));
    assert_eq!(color("punctuation"), Some(theme::TEXT_MUTED.into()));
    assert_eq!(color("string"), Some(theme::SUCCESS_BRIGHT.into()));
    assert_eq!(color("keyword"), Some(theme::PURPLE_BRIGHT.into()));
    assert_eq!(color("function"), Some(theme::BLUE_BRIGHT.into()));
    assert_eq!(color("number"), Some(theme::AMBER_BRIGHT.into()));
    assert_eq!(color("type"), Some(theme::AMBER_BRIGHT.into()));
}

#[gpui_kit::gpui::test]
fn bundled_brand_fonts_parse(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    // The headless text system lists no installed families, so the resolved
    // family cannot be asserted here — the app logs it at startup. What this
    // can prove is that all five embedded faces are present and parse.
    assert_eq!(
        devtools::assets::brand_fonts().len(),
        5,
        "every bundled Plex face is embedded"
    );
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(devtools::assets::brand_fonts())
            .expect("the bundled TTFs parse");
    });
}
