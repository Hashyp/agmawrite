//! The document-owned unsaved-changes modal.
//!
//! The application decides where this modal sits in the root stack. The
//! document feature owns its warning, controls, and document-local messages.

use iced::widget::{button, column, container, mouse_area, row, text, Space};
use iced::{Element, Font, Length};

use crate::theme::Palette;
use crate::ui::modal;

use super::{Message, UnsavedAction, UnsavedButton};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");

/// Renders the modal for the pending guarded document action. `focus` is
/// the button `j`/`k` navigation highlights — the accent-outlined one —
/// while `Enter` always saves, so the prompt opens focused on Save.
pub(crate) fn view(
    action: UnsavedAction,
    focus: UnsavedButton,
    palette: Palette,
) -> Element<'static, Message> {
    let button = |label: &'static str,
                  message: Message,
                  focused: bool,
                  color: iced::Color| {
        button(
            text(label)
                .font(EDITOR_FONT)
                .size(14)
                .color(if focused {
                    palette.foreground
                } else {
                    color
                }),
        )
        .on_press(message)
        .padding([6, 12])
        .style(move |theme, status| {
            if focused {
                modal::focused_button(&palette, theme, status)
            } else {
                modal::quiet_button(&palette, theme, status)
            }
        })
    };

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
                        "Cancel",
                        Message::UnsavedCancel,
                        focus == UnsavedButton::Cancel,
                        palette.light_foreground,
                    ),
                    Space::new().width(Length::Fill),
                    button(
                        "Save",
                        Message::UnsavedSave,
                        focus == UnsavedButton::Save,
                        palette.foreground,
                    ),
                    button(
                        "Discard",
                        Message::UnsavedDiscard,
                        focus == UnsavedButton::Discard,
                        palette.red,
                    ),
                ]
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .width(Length::Fixed(440.0))
        .padding(16)
        .style(move |_theme| modal::card(&palette)),
    )
    .on_press(Message::UnsavedCardPressed);

    mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_theme| modal::backdrop(&palette)),
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
