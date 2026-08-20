//! The document-owned unsaved-changes modal.
//!
//! The application decides where this modal sits in the root stack. The
//! document feature owns its warning, controls, and document-local messages.

use iced::widget::{button, column, container, mouse_area, row, text, Space};
use iced::{Element, Font, Length};

use crate::theme::Palette;

use super::{Message, UnsavedAction};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");

/// Renders the modal for the pending guarded document action.
pub(crate) fn view(action: UnsavedAction, palette: Palette) -> Element<'static, Message> {
    let card = mouse_area(
        container(
            column![
                text("Unsaved changes")
                    .font(EDITOR_FONT)
                    .size(16)
                    .color(palette.foreground),
                text(warning(action))
                    .font(EDITOR_FONT)
                    .size(13)
                    .color(palette.light_foreground),
                row![
                    button(
                        text("Cancel")
                            .font(EDITOR_FONT)
                            .size(14)
                            .color(palette.light_foreground),
                    )
                    .on_press(Message::UnsavedCancel)
                    .padding([6, 12])
                    .style(move |theme, status| {
                        crate::popup_button_style(&palette, theme, status)
                    }),
                    Space::new().width(Length::Fill),
                    button(
                        text("Save")
                            .font(EDITOR_FONT)
                            .size(14)
                            .color(palette.foreground),
                    )
                    .on_press(Message::UnsavedSave)
                    .padding([6, 12])
                    .style(move |theme, status| {
                        crate::popup_button_style(&palette, theme, status)
                    }),
                    button(
                        text("Discard")
                            .font(EDITOR_FONT)
                            .size(14)
                            .color(palette.red),
                    )
                    .on_press(Message::UnsavedDiscard)
                    .padding([6, 12])
                    .style(move |theme, status| {
                        crate::popup_button_style(&palette, theme, status)
                    }),
                ]
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .width(Length::Fixed(440.0))
        .padding(16)
        .style(move |_theme| crate::note_card_style(&palette)),
    )
    .on_press(Message::UnsavedCardPressed);

    mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(crate::note_backdrop_style),
    )
    .on_press(Message::UnsavedCancel)
    .into()
}

fn warning(action: UnsavedAction) -> &'static str {
    match action {
        UnsavedAction::OpenFile => {
            "The document has unsaved changes. Opening a new file will discard them."
        }
        UnsavedAction::CloseWindow(_) => "The document has unsaved changes. Close without saving?",
    }
}

#[cfg(test)]
mod tests {
    use super::warning;
    use crate::document::UnsavedAction;

    #[test]
    fn warning_describes_the_pending_action() {
        assert_eq!(
            warning(UnsavedAction::OpenFile),
            "The document has unsaved changes. Opening a new file will discard them."
        );
        assert_eq!(
            warning(UnsavedAction::CloseWindow(iced::window::Id::unique())),
            "The document has unsaved changes. Close without saving?"
        );
    }
}
