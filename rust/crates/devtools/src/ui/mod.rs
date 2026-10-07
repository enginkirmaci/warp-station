//! ui module: shared helpers + the main window.

pub mod bottom_bar;
pub mod changes;
pub mod dialogs;
pub mod diff;
pub mod repos;
pub mod window;

use gpui::Styled;

/// A tinted svg icon.
pub fn icon(path: &str, size: gpui::Pixels, color: impl Into<gpui::Hsla>) -> gpui::Svg {
    // flex_shrink_0: an svg is a leaf with no intrinsic size, so the default
    // flex-shrink can squeeze it to zero bounds in a tight container — gpui
    // then fails the render with "can't render at a zero size" on every repaint
    gpui::svg()
        .path(path)
        .size(size)
        .flex_shrink_0()
        .text_color(color.into())
}
