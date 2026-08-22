//! The palette-driven tooltip surface shared by the toolbar and the
//! comments rail — one helper instead of per-feature copies.

use iced::widget::container;
use iced::{Background, Border, Theme};

use crate::theme::Palette;

/// A small floating label: the palette's raised surface with foreground
/// text, like every other surface the application paints.
pub(crate) fn style(palette: &Palette, _theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.raised())),
        text_color: Some(palette.foreground),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::style;
    use crate::theme::Palette;
    use iced::{Background, Theme};

    /// The tooltip follows palette roles — the raised surface and the
    /// foreground — never a fixed grey with white text.
    #[test]
    fn tooltip_paints_the_raised_surface_and_foreground() {
        let palette = Palette::default();
        let tooltip = style(&palette, &Theme::Dark);

        assert_eq!(
            tooltip.background,
            Some(Background::Color(palette.raised()))
        );
        assert_eq!(tooltip.text_color, Some(palette.foreground));
        assert_eq!(tooltip.border.radius.top_left, 3.0);
    }
}
