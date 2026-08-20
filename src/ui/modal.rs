//! Neutral styling shared by the application's full modal overlays.
//!
//! Feature-owned modal content, messages, click policy, and focus behavior
//! stay with each feature; this module only provides their common visual
//! primitives.

use iced::widget::{button, container};
use iced::{Background, Border, Color, Theme};

use crate::theme::Palette;

/// The translucent scrim behind a modal card.
pub(crate) fn backdrop(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
        ..Default::default()
    }
}

/// The neutral elevated surface containing feature-owned modal content.
pub(crate) fn card(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(Palette::lightened(
            palette.dark_background,
            0.08,
        ))),
        border: Border {
            color: palette.muted,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

/// A low-emphasis bordered action used in modal button rows.
pub(crate) fn quiet_button(
    palette: &Palette,
    _theme: &Theme,
    status: button::Status,
) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(
                Palette::lightened(palette.darker_background, 0.25),
            )),
            _ => None,
        },
        border: Border {
            color: palette.muted,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{backdrop, card, quiet_button};
    use crate::theme::Palette;
    use iced::widget::button;
    use iced::{Background, Color, Theme};

    #[test]
    fn shared_modal_primitives_follow_the_neutral_palette() {
        let palette = Palette::default();

        assert_eq!(
            backdrop(&Theme::Dark).background,
            Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.6)))
        );

        let card = card(&palette);
        assert_eq!(card.border.color, palette.muted);
        assert_eq!(card.border.width, 1.0);

        let idle = quiet_button(&palette, &Theme::Dark, button::Status::Active);
        let hovered = quiet_button(&palette, &Theme::Dark, button::Status::Hovered);
        assert_eq!(idle.background, None);
        assert!(hovered.background.is_some());
        assert_eq!(idle.border.color, palette.muted);
    }
}
