//! Find popup rendering and input focus ownership.

use iced::widget::{button, container, operation::focus, row, text, text_input, Id};
use iced::{alignment, Background, Border, Element, Font, Length, Task, Theme};

use crate::theme::Palette;

use super::{current, match_count, Message, State, Surface};

const FIND_INPUT_ID: &str = "find-input";
const FIND_FONT: Font = Font::with_name("iA Writer Mono S");

/// Builds the top-right find popup with its live counter and navigation.
pub(crate) fn view<'a>(
    state: &'a State,
    surface: Surface<'_>,
    palette: Palette,
) -> Element<'a, Message> {
    let total = match_count(state, surface);
    let counter = counter(state, total, palette);

    let mut card_row = row![text_input("Search…", state.query())
        .id(input_id())
        .on_input(Message::QueryChanged)
        .font(FIND_FONT)
        .size(14)
        .padding(6)
        .width(Length::Fixed(220.0))]
    .spacing(8)
    .align_y(alignment::Vertical::Center);

    if let Some((counter, color)) = counter {
        card_row = card_row.push(text(counter).font(FIND_FONT).size(12).color(color));
    }

    // ▲ steps back, ▼ steps forward — the mouse path of Enter and
    // Shift+Enter.
    card_row = card_row
        .push(
            button(
                text("▲")
                    .font(FIND_FONT)
                    .size(12)
                    .color(palette.light_foreground),
            )
            .on_press(Message::Previous)
            .padding([4, 8])
            .style(move |theme, status| button_style(&palette, theme, status)),
        )
        .push(
            button(
                text("▼")
                    .font(FIND_FONT)
                    .size(12)
                    .color(palette.light_foreground),
            )
            .on_press(Message::Next)
            .padding([4, 8])
            .style(move |theme, status| button_style(&palette, theme, status)),
        );

    container(
        container(card_row)
            .padding(8)
            .style(move |_theme| card_style(&palette)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(alignment::Horizontal::Right)
    .align_y(alignment::Vertical::Top)
    .padding(16)
    .into()
}

pub(super) fn focus_input() -> Task<Message> {
    focus(input_id())
}

fn input_id() -> Id {
    Id::new(FIND_INPUT_ID)
}

fn counter(state: &State, total: usize, palette: Palette) -> Option<(String, iced::Color)> {
    (!state.query().is_empty()).then(|| {
        if total == 0 {
            ("no match".to_owned(), palette.red)
        } else {
            let current = current(state, total).map_or(1, |index| index + 1);
            (format!("{current}/{total}"), palette.dark_foreground)
        }
    })
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
    use super::counter;
    use crate::find::{update, Message, State, Surface};
    use crate::theme::Palette;

    #[test]
    fn counter_is_hidden_empty_and_reports_missing_and_current_matches() {
        let palette = Palette::default();
        let source = "one two one";
        let mut state = State::new();

        assert!(counter(&state, 0, palette).is_none());

        update(
            &mut state,
            Message::QueryChanged("missing".to_owned()),
            Surface::Source(source),
        );
        assert_eq!(
            counter(&state, 0, palette).map(|value| value.0),
            Some("no match".to_owned())
        );

        update(
            &mut state,
            Message::QueryChanged("one".to_owned()),
            Surface::Source(source),
        );
        assert_eq!(
            counter(&state, 2, palette).map(|value| value.0),
            Some("1/2".to_owned())
        );

        update(&mut state, Message::Next, Surface::Source(source));
        assert_eq!(
            counter(&state, 2, palette).map(|value| value.0),
            Some("2/2".to_owned())
        );
    }
}
