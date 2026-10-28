//! The kit theme must carry the DevTools palette after apply_kit_theme — a
//! Light→Dark mode flip inside the color-edit closure makes gpui-component
//! reload its stock dark theme over the overrides (near-black table canvas).

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
        assert_eq!(t.table, gpui::Hsla::from(theme::BACKGROUND));
        assert_eq!(t.popover, gpui::Hsla::from(theme::POPUP));
        assert_eq!(t.primary, gpui::Hsla::from(theme::ACCENT));
    });
}
