//! The corner controls: the open and save document icons floating at the
//! window's top-left corner of an editable session. Save is disabled —
//! dimmed, actionless — while the document has no unsaved changes; a
//! preview-only session (`--preview`) shows no corner controls at all.
//! The `Ctrl + O`/`Ctrl + S` shortcuts keep working everywhere.

use iced::widget::{button, canvas, container, row, text, tooltip};
use iced::{Element, Font, Length};

use super::icons::{OpenFileIcon, SaveIcon, ICON_BUTTON_SIZE, ICON_SIZE};
use crate::theme::Palette;

const CONTROLS_FONT: Font = Font::with_name("iA Writer Mono S");

/// Actions produced by the corner controls and the status bar's surface
/// switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Message {
    Open,
    Save,
    TogglePreview,
}

/// The read-only state needed to render the corner controls.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Model {
    /// Whether the document has unsaved changes.
    modified: bool,
    palette: Palette,
}

impl Model {
    pub(crate) fn new(modified: bool, palette: Palette) -> Self {
        Self { modified, palette }
    }

    /// Save acts only while there is something to write; a disabled icon
    /// publishes no message.
    fn save_enabled(self) -> bool {
        self.modified
    }
}

/// Renders the open and save icons.
pub(crate) fn view(model: Model) -> Element<'static, Message> {
    let palette = model.palette;

    row![
        icon_button(
            OpenFileIcon { dimmed: false },
            "Ctrl + o, Open",
            Some(Message::Open),
            palette,
        ),
        icon_button(
            SaveIcon {
                dimmed: !model.save_enabled()
            },
            "Ctrl + s, Save",
            model.save_enabled().then_some(Message::Save),
            palette,
        ),
    ]
    .spacing(4)
    .into()
}

fn icon_button<Icon>(
    icon: Icon,
    hint: &'static str,
    message: Option<Message>,
    palette: Palette,
) -> Element<'static, Message>
where
    Icon: canvas::Program<Message> + 'static,
{
    tooltip(
        button(
            canvas(icon)
                .width(Length::Fixed(ICON_SIZE))
                .height(Length::Fixed(ICON_SIZE)),
        )
        .on_press_maybe(message)
        .width(Length::Fixed(ICON_BUTTON_SIZE))
        .height(Length::Fixed(ICON_BUTTON_SIZE))
        .padding(0)
        .style(move |theme, status| icon_button_style(&palette, theme, status)),
        tooltip_label(hint, palette),
        tooltip::Position::Bottom,
    )
    .into()
}

fn tooltip_label(label: &'static str, palette: Palette) -> container::Container<'static, Message> {
    container(text(label).font(CONTROLS_FONT).size(12))
        .padding([4, 8])
        .style(move |theme| crate::ui::tooltip::style(&palette, theme))
}

fn icon_button_style(
    palette: &Palette,
    _theme: &iced::Theme,
    status: button::Status,
) -> button::Style {
    button::Style {
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => palette.light_foreground,
            _ => palette.foreground,
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::Model;
    use crate::theme::Palette;

    #[test]
    fn save_waits_for_unsaved_changes() {
        let clean = Model::new(false, Palette::default());
        assert!(!clean.save_enabled());

        let edited = Model::new(true, Palette::default());
        assert!(edited.save_enabled());
    }
}
