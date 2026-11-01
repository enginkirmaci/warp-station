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
