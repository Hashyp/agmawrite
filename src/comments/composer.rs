//! Note composer popup, focus operation, and comment-specific styling.

use iced::widget::{
    button, column, container, mouse_area, operation::focus, row, text, text_editor,
};
use iced::{Background, Border, Element, Font, Length, Task, Theme};

use super::{Message, State};
use crate::theme::Palette;

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
const EDITOR_ID: &str = "note-editor";

/// Focuses the comments-owned composer editor without exposing its widget ID
/// to the composing application.
pub(crate) fn focus_composer() -> Task<Message> {
    focus(iced::widget::Id::new(EDITOR_ID))
}

/// The note composer: a translucent backdrop with a centered card holding
/// the draft, history, and controls. The card swallows clicks while the
/// backdrop closes the composer, preserving the editing surface below it.
pub(crate) fn view(state: &State, palette: Palette) -> Element<'_, Message> {
    let editing = state.editing_target().is_some();
    let mut card_body = column![].spacing(12);

    card_body = card_body.push(if editing {
        text("Editing comment — Ctrl+S saves, Esc discards")
            .font(EDITOR_FONT)
            .size(12)
            .color(palette.yellow)
    } else {
        text("Note")
            .font(EDITOR_FONT)
            .size(12)
            .color(palette.light_foreground)
    });

    // The texts this comment replaced, newest first, stay dimmed above the
    // editor while an existing comment is being edited.
    if editing {
        for previous in state.active_history().iter().rev() {
            card_body = card_body.push(
                text(format!("— {previous}"))
                    .font(EDITOR_FONT)
                    .size(12)
                    .color(palette.dark_foreground),
            );
        }
    }

    card_body = card_body.push(
        text_editor(state.composer())
            .id(iced::widget::Id::new(EDITOR_ID))
            .on_action(Message::EditComposer)
            .font(EDITOR_FONT)
            .size(20)
            .height(Length::Fixed(160.0))
            .padding(8)
            .style(move |theme, status| editor_style(&palette, theme, status)),
    );

    let mut buttons = row![button(
        text("Close")
            .font(EDITOR_FONT)
            .size(14)
            .color(palette.light_foreground),
    )
    .on_press(Message::CloseComposer)
    .padding([6, 12])
    .style(move |theme, status| button_style(&palette, theme, status))]
    .width(Length::Fill);

    if let Some((thread, entry)) = state.editing_target() {
        buttons = buttons.push(
            button(text("Delete").font(EDITOR_FONT).size(14).color(palette.red))
                .on_press(Message::DeleteComment(thread, entry))
                .padding([6, 12])
                .style(move |theme, status| button_style(&palette, theme, status)),
        );
    }

    buttons = buttons
        .push(iced::widget::Space::new().width(Length::Fill))
        .push(
            button(
                text("Save")
                    .font(EDITOR_FONT)
                    .size(14)
                    .color(palette.foreground),
            )
            .on_press(Message::SaveComposer)
            .padding([6, 12])
            .style(move |theme, status| button_style(&palette, theme, status)),
        );

    card_body = card_body.push(buttons);

    let card = mouse_area(
        container(card_body)
            .width(Length::Fixed(440.0))
            .padding(16)
            .style(move |_theme| card_style(&palette)),
    )
    .on_press(Message::ComposerCardPressed);

    mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(backdrop_style),
    )
    .on_press(Message::CloseComposer)
    .into()
}

fn editor_style(
    palette: &Palette,
    _theme: &Theme,
    _status: text_editor::Status,
) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(palette.background),
        border: Border::default(),
        placeholder: palette.foreground,
        value: palette.foreground,
        selection: palette.selection,
    }
}

fn backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(iced::Color::from_rgba(
            0.0, 0.0, 0.0, 0.6,
        ))),
        ..Default::default()
    }
}

fn card_style(palette: &Palette) -> container::Style {
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

fn button_style(palette: &Palette, _theme: &Theme, status: button::Status) -> button::Style {
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
    use super::{card_style, editor_style, view};
    use crate::comments::State;
    use crate::theme::Palette;
    use iced::widget::text_editor;
    use iced::{Background, Theme};

    #[test]
    fn composer_view_builds_from_comments_state() {
        let state = State::new();
        let _ = view(&state, Palette::default());
    }

    #[test]
    fn composer_styles_follow_the_comments_palette() {
        let palette = Palette::default();
        let editor = editor_style(&palette, &Theme::Dark, text_editor::Status::Active);
        let card = card_style(&palette);

        assert_eq!(editor.background, Background::Color(palette.background));
        assert_eq!(editor.value, palette.foreground);
        assert_eq!(editor.selection, palette.selection);
        assert_eq!(card.border.color, palette.muted);
        assert_eq!(card.border.width, 1.0);
    }
}
