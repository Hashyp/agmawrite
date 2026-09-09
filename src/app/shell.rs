//! Neutral root-shell geometry and stable layer construction.
//!
//! The composition root supplies already-built feature elements in their
//! intended order. This module only sizes and places them; it knows nothing
//! about application or feature state.

use iced::widget::{column, container, row, stack, Space};
use iced::{Background, Color, Element, Length};

const TOP_MARGIN_PORTION: u16 = 1;
const EDITOR_AREA_PORTION: u16 = 8;
const TOOLBAR_PORTION: u16 = 1;
const SIDE_MARGIN_PORTION: u16 = 5;
const MAIN_AREA_PORTION: u16 = 90;

/// Builds a stable stack by pushing overlays in bottom-to-top iterator order.
pub(crate) fn stack_layers<'a, Message: 'a>(
    base: Element<'a, Message>,
    overlays: impl IntoIterator<Item = Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut layers = stack![base];

    for overlay in overlays {
        layers = layers.push(overlay);
    }

    layers.into()
}

/// Places the editor, toolbar, and comments sidebar in the root shell.
///
/// The editor keeps symmetric five-percent side margins while the sidebar
/// remains outside those margins and flush with the window's right edge.
/// Vertically, the top margin, editor, and toolbar retain their 1/8/1 sizing.
pub(crate) fn layout<'a, Message: 'a>(
    editor: Element<'a, Message>,
    toolbar: Element<'a, Message>,
    sidebar: Element<'a, Message>,
    background: Color,
) -> Element<'a, Message> {
    let main = column![
        Space::new()
            .width(Length::Fill)
            .height(Length::FillPortion(TOP_MARGIN_PORTION)),
        container(editor)
            .width(Length::Fill)
            .height(Length::FillPortion(EDITOR_AREA_PORTION)),
        container(toolbar)
            .width(Length::Fill)
            .height(Length::FillPortion(TOOLBAR_PORTION)),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let main_with_margins = row![
        Space::new()
            .width(Length::FillPortion(SIDE_MARGIN_PORTION))
            .height(Length::Fill),
        container(main)
            .width(Length::FillPortion(MAIN_AREA_PORTION))
            .height(Length::Fill),
        Space::new()
            .width(Length::FillPortion(SIDE_MARGIN_PORTION))
            .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    container(
        row![container(main_with_margins)
            .width(Length::Fill)
            .height(Length::Fill)]
        .push(sidebar)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme| root_background(background))
    .into()
}

/// A global, fixed-height status line spans the entire window, including the
/// sidebar, just like LazyVim's `globalstatus`. The body keeps its own margins.
pub(crate) fn with_status_bar<'a, Message: 'a>(
    body: Element<'a, Message>,
    bar: Element<'a, Message>,
) -> Element<'a, Message> {
    column![
        container(body).width(Length::Fill).height(Length::Fill),
        bar
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Floats the corner controls over the body's top-left corner. The overlay
/// paints only: `Stack` passes events on unless a child captures them, so
/// clicks and scrolls reach the editor beneath everywhere outside the
/// buttons themselves.
pub(crate) fn with_corner_controls<'a, Message: 'a>(
    body: Element<'a, Message>,
    controls: Element<'a, Message>,
) -> Element<'a, Message> {
    use iced::alignment::{Horizontal, Vertical};

    stack![
        body,
        container(controls)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Left)
            .align_y(Vertical::Top)
            .padding(iced::Padding {
                top: 4.0,
                left: 4.0,
                ..iced::Padding::new(0.0)
            }),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn root_background(background: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        layout, root_background, stack_layers, with_corner_controls, with_status_bar,
        EDITOR_AREA_PORTION, MAIN_AREA_PORTION, SIDE_MARGIN_PORTION, TOOLBAR_PORTION,
        TOP_MARGIN_PORTION,
    };
    use iced::widget::Space;
    use iced::{Background, Color, Element};

    #[derive(Debug, Clone)]
    enum TestMessage {}

    #[test]
    fn geometry_preserves_editor_margins_and_area_sizing() {
        assert_eq!(SIDE_MARGIN_PORTION * 2 + MAIN_AREA_PORTION, 100);
        assert_eq!(
            TOP_MARGIN_PORTION + EDITOR_AREA_PORTION + TOOLBAR_PORTION,
            10
        );
    }

    #[test]
    fn shell_is_generic_over_messages_and_uses_the_requested_background() {
        let element = || Element::<TestMessage>::from(Space::new());
        let editing = stack_layers(element(), [element()]);
        let _shell = layout(editing, element(), element(), Color::BLACK);

        assert_eq!(
            root_background(Color::BLACK).background,
            Some(Background::Color(Color::BLACK))
        );

        // The corner overlay and the status bar wrap any body generically.
        let _dressed = with_status_bar(with_corner_controls(element(), element()), element());
    }
}
