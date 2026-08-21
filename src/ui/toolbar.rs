//! Bottom-bar document controls and the current input-mode badge.

use iced::widget::{button, canvas, container, row, text, tooltip, Space};
use iced::{alignment, Background, Border, Color, Element, Font, Length, Theme};

use super::icons::{OpenFileIcon, PreviewIcon, SaveIcon, WriteIcon, ICON_BUTTON_SIZE, ICON_SIZE};
use crate::input::Mode;
use crate::theme::Palette;

const TOOLBAR_FONT: Font = Font::with_name("iA Writer Mono S");

/// Actions produced by the toolbar's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Message {
    Open,
    Save,
    TogglePreview,
}

/// The read-only state needed to render the bottom toolbar.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Model {
    preview: bool,
    can_toggle_preview: bool,
    mode: Mode,
    pending_count: u32,
    palette: Palette,
}

impl Model {
    pub(crate) fn new(
        preview: bool,
        can_toggle_preview: bool,
        mode: Mode,
        pending_count: u32,
        palette: Palette,
    ) -> Self {
        Self {
            preview,
            can_toggle_preview,
            mode,
            pending_count,
            palette,
        }
    }
}

/// Renders the open/save controls, optional preview toggle, and mode badge.
pub(crate) fn view(model: Model) -> Element<'static, Message> {
    let palette = model.palette;
    let open_button = tooltip(
        button(icon(OpenFileIcon))
            .on_press(Message::Open)
            .width(Length::Fixed(ICON_BUTTON_SIZE))
            .height(Length::Fixed(ICON_BUTTON_SIZE))
            .padding(0)
            .style(move |theme, status| icon_button_style(&palette, theme, status)),
        tooltip_label("Ctrl + o, Open"),
        tooltip::Position::Top,
    );

    let save_button = tooltip(
        button(icon(SaveIcon))
            .on_press(Message::Save)
            .width(Length::Fixed(ICON_BUTTON_SIZE))
            .height(Length::Fixed(ICON_BUTTON_SIZE))
            .padding(0)
            .style(move |theme, status| icon_button_style(&palette, theme, status)),
        tooltip_label("Ctrl + s, Save"),
        tooltip::Position::Top,
    );

    let mut controls: Vec<Element<'static, Message>> = vec![open_button.into(), save_button.into()];

    if let Some((label, icon)) = toggle_presentation(model) {
        let toggle_button = tooltip(
            button(icon)
                .on_press(Message::TogglePreview)
                .width(Length::Fixed(ICON_BUTTON_SIZE))
                .height(Length::Fixed(ICON_BUTTON_SIZE))
                .padding(0)
                .style(move |theme, status| icon_button_style(&palette, theme, status)),
            tooltip_label(label),
            tooltip::Position::Top,
        );
        controls.push(toggle_button.into());
    }

    controls.push(mode_badge(model));
    controls.push(Space::new().width(Length::Fill).height(Length::Fill).into());

    row(controls)
        .width(Length::Fill)
        .height(Length::FillPortion(1))
        .spacing(4)
        .align_y(alignment::Vertical::Bottom)
        .into()
}

fn icon<Icon>(icon: Icon) -> Element<'static, Message>
where
    Icon: canvas::Program<Message> + 'static,
{
    canvas(icon)
        .width(Length::Fixed(ICON_SIZE))
        .height(Length::Fixed(ICON_SIZE))
        .into()
}

/// The eye invites switching to preview while writing; the pencil switches
/// back to writing while previewing. Preview-only mode has no toggle.
fn toggle_presentation(model: Model) -> Option<(&'static str, Element<'static, Message>)> {
    if !model.can_toggle_preview {
        None
    } else if model.preview {
        Some(("Ctrl + p, Write", icon(WriteIcon)))
    } else {
        Some(("Ctrl + p, Preview", icon(PreviewIcon)))
    }
}

fn tooltip_label(label: &'static str) -> container::Container<'static, Message> {
    container(text(label).font(TOOLBAR_FONT).size(12))
        .padding([4, 8])
        .style(tooltip_style)
}

fn mode_badge(model: Model) -> Element<'static, Message> {
    let color = mode_color(model.mode, model.palette);
    let label = mode_label(model.mode, model.pending_count);

    container(text(label).font(TOOLBAR_FONT).size(12).color(color))
        // The extra bottom padding pushes the label a few pixels up, level
        // with the icon glyphs beside it instead of below them.
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 4.0,
            left: 8.0,
        })
        .height(Length::Fixed(ICON_BUTTON_SIZE))
        .align_y(alignment::Vertical::Center)
        .style(move |_theme| mode_badge_style(color))
        .into()
}

fn mode_label(mode: Mode, pending_count: u32) -> String {
    let mode = match mode {
        Mode::Visual => "VISUAL",
        Mode::Note => "NOTE",
        Mode::Find => "FIND",
        Mode::View => "VIEW",
        Mode::Write => "WRITE",
    };

    // A pending count shows beside the mode, like vim's cmdline — the `3`
    // of a `3j` waiting for its motion.
    if pending_count > 0 {
        format!("{pending_count} {mode}")
    } else {
        mode.to_owned()
    }
}

fn mode_color(mode: Mode, palette: Palette) -> Color {
    match mode {
        Mode::Visual => palette.blue,
        Mode::Note => palette.yellow,
        Mode::Find => palette.orange,
        Mode::View | Mode::Write => palette.light_foreground,
    }
}

fn icon_button_style(palette: &Palette, _theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => palette.light_foreground,
            _ => palette.foreground,
        },
        ..Default::default()
    }
}

fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.15, 0.15, 0.15))),
        text_color: Some(Color::WHITE),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
        ..Default::default()
    }
}

fn mode_badge_style(color: Color) -> container::Style {
    container::Style {
        text_color: Some(color),
        border: Border {
            color: Color { a: 0.4, ..color },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{mode_color, mode_label, toggle_presentation, Model};
    use crate::input::Mode;
    use crate::theme::Palette;

    #[test]
    fn mode_badge_formats_every_mode_and_pending_count() {
        assert_eq!(mode_label(Mode::Write, 0), "WRITE");
        assert_eq!(mode_label(Mode::View, 0), "VIEW");
        assert_eq!(mode_label(Mode::Visual, 3), "3 VISUAL");
        assert_eq!(mode_label(Mode::Note, 12), "12 NOTE");
        assert_eq!(mode_label(Mode::Find, 0), "FIND");

        let palette = Palette::default();
        assert_eq!(mode_color(Mode::Visual, palette), palette.blue);
        assert_eq!(mode_color(Mode::Note, palette), palette.yellow);
        assert_eq!(mode_color(Mode::Find, palette), palette.orange);
        assert_eq!(mode_color(Mode::View, palette), palette.light_foreground);
        assert_eq!(mode_color(Mode::Write, palette), palette.light_foreground);
    }

    #[test]
    fn preview_toggle_preserves_labels_and_is_hidden_in_preview_only_mode() {
        let palette = Palette::default();
        let write = Model::new(false, true, Mode::Write, 0, palette);
        let preview = Model::new(true, true, Mode::View, 0, palette);
        let preview_only = Model::new(true, false, Mode::View, 0, palette);

        assert_eq!(toggle_presentation(write).unwrap().0, "Ctrl + p, Preview");
        assert_eq!(toggle_presentation(preview).unwrap().0, "Ctrl + p, Write");
        assert!(toggle_presentation(preview_only).is_none());
    }
}
