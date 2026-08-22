//! Bottom-bar document controls and the current input-mode badge.

use iced::widget::{button, canvas, container, row, text, tooltip, Space};
use iced::{alignment, Border, Color, Element, Font, Length, Theme};

use super::icons::{OpenFileIcon, PreviewIcon, SaveIcon, WriteIcon, ICON_BUTTON_SIZE, ICON_SIZE};
use crate::input::{ModeBadge, PreviewToggle, ToolbarPresentation};
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
    presentation: ToolbarPresentation,
    palette: Palette,
}

impl Model {
    pub(crate) fn new(presentation: ToolbarPresentation, palette: Palette) -> Self {
        Self {
            presentation,
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
        tooltip_label("Ctrl + o, Open", palette),
        tooltip::Position::Top,
    );

    let save_button = tooltip(
        button(icon(SaveIcon))
            .on_press(Message::Save)
            .width(Length::Fixed(ICON_BUTTON_SIZE))
            .height(Length::Fixed(ICON_BUTTON_SIZE))
            .padding(0)
            .style(move |theme, status| icon_button_style(&palette, theme, status)),
        tooltip_label("Ctrl + s, Save", palette),
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
            tooltip_label(label, model.palette),
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
    match model.presentation.preview_toggle() {
        Some(PreviewToggle::Preview) => Some(("Ctrl + p, Preview", icon(PreviewIcon))),
        Some(PreviewToggle::Write) => Some(("Ctrl + p, Write", icon(WriteIcon))),
        None => None,
    }
}

fn tooltip_label(label: &'static str, palette: Palette) -> container::Container<'static, Message> {
    container(text(label).font(TOOLBAR_FONT).size(12))
        .padding([4, 8])
        .style(move |theme| crate::ui::tooltip::style(&palette, theme))
}

fn mode_badge(model: Model) -> Element<'static, Message> {
    let color = mode_color(model.presentation.badge(), model.palette);
    let label = mode_label(model.presentation);

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

fn mode_label(presentation: ToolbarPresentation) -> String {
    let mode = match presentation.badge() {
        ModeBadge::Visual => "VISUAL",
        ModeBadge::Note => "NOTE",
        ModeBadge::Find => "FIND",
        ModeBadge::View => "VIEW",
        ModeBadge::Write => "WRITE",
    };

    // A pending count shows beside the mode, like vim's cmdline — the `3`
    // of a `3j` waiting for its motion.
    presentation
        .pending_count()
        .map_or_else(|| mode.to_owned(), |count| format!("{count} {mode}"))
}

fn mode_color(mode: ModeBadge, palette: Palette) -> Color {
    match mode {
        ModeBadge::Visual => palette.blue,
        ModeBadge::Note => palette.yellow,
        ModeBadge::Find => palette.orange,
        ModeBadge::View | ModeBadge::Write => palette.light_foreground,
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
    use crate::input::{InteractionState, ModeBadge};
    use crate::theme::Palette;

    #[test]
    fn mode_badge_formats_every_legal_presentation() {
        let write = InteractionState::editable();
        assert_eq!(mode_label(write.view().toolbar()), "WRITE");

        let mut preview = write;
        assert!(preview.toggle_preview());
        assert_eq!(mode_label(preview.view().toolbar()), "VIEW");

        let mut visual = preview;
        visual.toggle_visual().unwrap();
        visual.push_count_digit(1);
        visual.push_count_digit(2);
        assert_eq!(mode_label(visual.view().toolbar()), "12 VISUAL");

        let mut note = preview;
        note.open_note().unwrap();
        assert_eq!(mode_label(note.view().toolbar()), "NOTE");

        let mut find = preview;
        find.open_find();
        assert_eq!(mode_label(find.view().toolbar()), "FIND");

        let palette = Palette::default();
        assert_eq!(mode_color(ModeBadge::Visual, palette), palette.blue);
        assert_eq!(mode_color(ModeBadge::Note, palette), palette.yellow);
        assert_eq!(mode_color(ModeBadge::Find, palette), palette.orange);
        assert_eq!(
            mode_color(ModeBadge::View, palette),
            palette.light_foreground
        );
        assert_eq!(
            mode_color(ModeBadge::Write, palette),
            palette.light_foreground
        );
    }

    #[test]
    fn preview_toggle_comes_from_the_workspace_presentation() {
        let palette = Palette::default();
        let write_state = InteractionState::editable();
        let mut preview_state = write_state;
        assert!(preview_state.toggle_preview());
        let preview_only_state = InteractionState::preview_only();

        let write = Model::new(write_state.view().toolbar(), palette);
        let preview = Model::new(preview_state.view().toolbar(), palette);
        let preview_only = Model::new(preview_only_state.view().toolbar(), palette);

        assert_eq!(toggle_presentation(write).unwrap().0, "Ctrl + p, Preview");
        assert_eq!(toggle_presentation(preview).unwrap().0, "Ctrl + p, Write");
        assert!(toggle_presentation(preview_only).is_none());
    }
}
