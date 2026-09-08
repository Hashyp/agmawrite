//! Neutral styling shared by the application's full modal overlays.
//!
//! Feature-owned modal content, messages, click policy, and focus behavior
//! stay with each feature; this module only provides their common visual
//! primitives.

use iced::widget::{button, container};
use iced::{Background, Border, Theme};

use crate::theme::Palette;

/// The translucent scrim behind a modal card: the theme's deepest
/// background tinted over the page, dark on dark themes and grey on light
/// ones.
pub(crate) fn backdrop(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(
            palette.tint(palette.darker_background, 0.6),
        )),
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

/// The keyboard-focused sibling of [`quiet_button`]: the same low-
/// emphasis action with the theme's accent border and a faint accent
/// wash, marking where `Enter` will land.
pub(crate) fn focused_button(
    palette: &Palette,
    _theme: &Theme,
    status: button::Status,
) -> button::Style {
    let quiet = quiet_button(palette, _theme, status);

    let background = match status {
        button::Status::Hovered | button::Status::Pressed => quiet.background,
        _ => Some(Background::Color(palette.tint(palette.accent, 0.12))),
    };

    button::Style {
        background,
        border: Border {
            color: palette.accent,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..quiet
    }
}

#[cfg(test)]
mod tests {
    use super::{backdrop, card, focused_button, quiet_button};
    use crate::theme::Palette;
    use iced::widget::button;
    use iced::{Background, Theme};

    #[test]
    fn shared_modal_primitives_follow_the_neutral_palette() {
        let palette = Palette::default();

        assert_eq!(
            backdrop(&palette).background,
            Some(Background::Color(
                palette.tint(palette.darker_background, 0.6)
            ))
        );

        let card = card(&palette);
        assert_eq!(card.border.color, palette.muted);
        assert_eq!(card.border.width, 1.0);

        let idle = quiet_button(&palette, &Theme::Dark, button::Status::Active);
        let hovered = quiet_button(&palette, &Theme::Dark, button::Status::Hovered);
        assert_eq!(idle.background, None);
        assert!(hovered.background.is_some());
        assert_eq!(idle.border.color, palette.muted);

        // The keyboard-focused variant keeps the quiet shape but swaps in
        // the accent border and a faint accent wash.
        let focused = focused_button(&palette, &Theme::Dark, button::Status::Active);
        assert_eq!(focused.border.color, palette.accent);
        assert_eq!(focused.border.radius, idle.border.radius);
        assert!(focused.background.is_some());

        let focused_hover =
            focused_button(&palette, &Theme::Dark, button::Status::Hovered);
        assert_eq!(focused_hover.background, hovered.background);
    }
}
