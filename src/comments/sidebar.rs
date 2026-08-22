//! Comments sidebar, collapsed rail, card projection, and local styling.

use iced::widget::{
    button, canvas, column, container, mouse_area, row, scrollable, text, text_editor, tooltip,
    Space,
};
use iced::{alignment, Background, Border, Element, Font, Length, Theme};

use super::model::CommentCard;
use super::{Message, State};
use crate::preview::PreviewElement;
use crate::theme::Palette;
use crate::ui::icons::{CommentsIcon, ICON_SIZE};

/// The width of the expanded comments sidebar on the right.
const SIDEBAR_WIDTH: f32 = 340.0;
const RAIL_WIDTH: f32 = 44.0;

/// Read-only collaborators needed to project comments into sidebar cards.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ViewContext<'a> {
    pub(crate) source: &'a str,
    pub(crate) preview_elements: &'a [PreviewElement],
    pub(crate) palette: Palette,
    pub(crate) font: Font,
}

/// Builds either the full sidebar or its collapsed rail. The comments feature
/// owns the visibility decision and geometry; the app only places this element
/// beside the editing surface and maps its local messages.
pub(crate) fn view<'a>(state: &'a State, context: ViewContext<'_>) -> Element<'a, Message> {
    if state.sidebar_shown() {
        container(row![
            edge_separator(context.palette),
            container(expanded(state, context))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
        .width(Length::Fixed(SIDEBAR_WIDTH))
        .height(Length::Fill)
        .into()
    } else {
        row![
            edge_separator(context.palette),
            collapsed(state, context.palette, context.font)
        ]
        .into()
    }
}

/// The 1px divider at the sidebar's left edge, drawn as a real element
/// rather than a container border so full-width comment cards can never
/// paint over it.
fn edge_separator(palette: Palette) -> Element<'static, Message> {
    container(Space::new().width(Length::Fixed(1.0)).height(Length::Fill))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(move |_theme| edge_separator_style(palette))
        .into()
}

/// The collapsed sidebar rail: the visual indication that a sidebar exists
/// and where it went. Its button expands the sidebar (`Ctrl+B` too), and the
/// count under the icon shows how many comments wait inside.
fn collapsed(state: &State, palette: Palette, font: Font) -> Element<'_, Message> {
    let expand_button = button(
        column![
            canvas(CommentsIcon)
                .width(Length::Fixed(ICON_SIZE))
                .height(Length::Fixed(ICON_SIZE)),
            text(if state.is_empty() {
                String::new()
            } else {
                state.len().to_string()
            })
            .font(font)
            .size(11)
            .color(if state.is_empty() {
                palette.dark_foreground
            } else {
                palette.accent
            }),
        ]
        .spacing(2)
        .align_x(alignment::Horizontal::Center),
    )
    .on_press(Message::ToggleSidebar)
    .width(Length::Fill)
    .padding([10, 4])
    .style(move |theme, status| rail_button_style(palette, theme, status));

    tooltip(
        container(
            container(expand_button)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(alignment::Vertical::Center),
        )
        .width(Length::Fixed(RAIL_WIDTH))
        .height(Length::Fill)
        .padding(0)
        .style(move |_theme| rail_style(palette)),
        container(
            text(format!("Ctrl + b, Comments ({})", state.len()))
                .font(font)
                .size(12),
        )
        .padding([4, 8])
        .style(move |theme| crate::ui::tooltip::style(&palette, theme)),
        tooltip::Position::Left,
    )
    .into()
}

/// The comments sidebar: a full-height panel with a scrollable tree of
/// comment threads — the root quoting the Markdown source it was written for,
/// replies indented under it — a resolved-history section below, and a free
/// text field with Add and Publish buttons at the bottom.
fn expanded<'a>(state: &'a State, context: ViewContext<'_>) -> Element<'a, Message> {
    let palette = context.palette;
    let font = context.font;
    let cards = state.cards(context.source, context.preview_elements);

    let open: Vec<Element<'_, Message>> = cards
        .iter()
        .filter(|card| !card.resolved)
        .map(|card| comment_card(card.clone(), palette, font))
        .collect();
    let resolved: Vec<Element<'_, Message>> = cards
        .iter()
        .filter(|card| card.resolved)
        .map(|card| comment_card(card.clone(), palette, font))
        .collect();

    let mut list: Vec<Element<'_, Message>> = Vec::new();

    if open.is_empty() && resolved.is_empty() {
        list.push(
            text("No comments yet — press c in the preview or write one below.")
                .font(font)
                .size(13)
                .color(palette.dark_foreground)
                .into(),
        );
    }

    list.extend(open);

    // Resolved threads are the history section — kept, dimmed, reopenable.
    if !resolved.is_empty() {
        list.push(
            text(format!("RESOLVED ({})", resolved.len()))
                .font(font)
                .size(13)
                .color(palette.dark_foreground)
                .into(),
        );
        list.extend(resolved);
    }

    container(
        column![
            container(
                text(format!("COMMENTS ({})", state.len()))
                    .font(font)
                    .size(13)
                    .color(palette.light_foreground),
            )
            .padding(iced::Padding {
                top: 4.0,
                left: 8.0,
                right: 8.0,
                ..iced::Padding::new(0.0)
            }),
            scrollable(column(list).spacing(8).width(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::hidden(),
                )),
            container(
                column![
                    text("Write a comment…")
                        .font(font)
                        .size(13)
                        .color(palette.dark_foreground),
                    text_editor(state.draft())
                        .on_action(Message::EditDraft)
                        .font(font)
                        .size(16)
                        .height(Length::Fixed(72.0))
                        .padding(6)
                        .style(move |theme, status| {
                            publish_editor_style(&palette, theme, status)
                        }),
                ]
                .spacing(4)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(iced::Padding {
                top: 6.0,
                bottom: 6.0,
                left: 8.0,
                right: 8.0,
            })
            .style(move |_theme| publish_field_style(&palette)),
            row![
                button(text("Add").font(font).size(15).color(palette.foreground),)
                    .on_press(Message::AddDraftAsGlobal)
                    .width(Length::Fill)
                    .padding([6, 12])
                    .style(move |_theme, status| add_button_style(&palette, status)),
                button(
                    text("Publish")
                        .font(font)
                        .size(15)
                        .color(palette.foreground),
                )
                .on_press(Message::PublishDraft)
                .width(Length::Fill)
                .padding([6, 12])
                .style(move |_theme, status| publish_button_style(&palette, status)),
            ]
            .spacing(8)
            .width(Length::Fill)
            .padding(iced::Padding {
                left: 8.0,
                right: 8.0,
                ..iced::Padding::new(0.0)
            }),
        ]
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(iced::Padding {
        top: 8.0,
        bottom: 8.0,
        ..iced::Padding::new(0.0)
    })
    .style(move |_theme| sidebar_style(&palette))
    .into()
}

/// One comment card projected from the model: the root quotes its anchor (or
/// carries the Global label), replies indent under it, and each card carries
/// its resolve toggle and delete action.
fn comment_card(card: CommentCard, palette: Palette, font: Font) -> Element<'static, Message> {
    let thread = card.thread;
    let entry = card.entry;

    let caption = card
        .label
        .map(str::to_owned)
        .unwrap_or_else(|| card.quote.clone());

    let mut body = column![].spacing(4);

    if card.first && !caption.is_empty() {
        body = body.push(
            text(if card.resolved {
                format!("✓ {caption}")
            } else {
                caption
            })
            .font(font)
            .size(13)
            .color(if card.active {
                palette.accent
            } else {
                palette.dark_foreground
            }),
        );
    }

    let mut text_row = row![text(card.text.clone())
        .font(font)
        .size(15)
        .color(if card.resolved {
            palette.dark_foreground
        } else {
            palette.foreground
        }),]
    .align_y(alignment::Vertical::Top);

    if card.history > 0 {
        text_row = text_row.push(
            text(format!("(edited ×{})", card.history))
                .font(font)
                .size(11)
                .color(palette.dark_foreground),
        );
    }

    body = body.push(text_row);
    body = body.push(
        row![
            button(
                text(if card.resolved {
                    "↺ Reopen"
                } else {
                    "✓ Resolve"
                })
                .font(font)
                .size(11)
                .color(palette.light_foreground),
            )
            .on_press(Message::ResolveThread(thread))
            .padding([2, 6])
            .style(move |theme, status| card_button_style(palette, theme, status)),
            button(
                text("× Delete")
                    .font(font)
                    .size(11)
                    .color(palette.light_foreground),
            )
            .on_press(Message::DeleteComment(thread, entry))
            .padding([2, 6])
            .style(move |theme, status| card_button_style(palette, theme, status)),
        ]
        .spacing(4),
    );

    let card_element: Element<'_, Message> = mouse_area(
        container(body)
            .padding(8)
            .width(Length::Fill)
            .style(move |_theme| comment_card_style(&palette, card.active, card.resolved)),
    )
    .on_press(Message::ActivateCard(thread, entry))
    .into();

    if card.depth == 0 {
        card_element
    } else {
        container(card_element)
            .padding(iced::Padding {
                left: 14.0 * card.depth as f32,
                ..iced::Padding::new(0.0)
            })
            .width(Length::Fill)
            .into()
    }
}

fn sidebar_style(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.dark_background)),
        border: Border::default(),
        ..Default::default()
    }
}

fn rail_style(palette: Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.dark_background)),
        border: Border::default(),
        ..Default::default()
    }
}

fn edge_separator_style(palette: Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.muted)),
        ..Default::default()
    }
}

fn rail_button_style(palette: Palette, _theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(
                Palette::lightened(palette.darker_background, 0.15),
            )),
            _ => None,
        },
        ..Default::default()
    }
}

fn comment_card_style(palette: &Palette, active: bool, resolved: bool) -> container::Style {
    let background = if active {
        palette.tint(palette.accent, 0.1)
    } else if resolved {
        palette.dark_background
    } else {
        palette.darker_background
    };

    container::Style {
        background: Some(Background::Color(background)),
        border: Border::default(),
        ..Default::default()
    }
}

fn card_button_style(palette: Palette, _theme: &Theme, status: button::Status) -> button::Style {
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

fn publish_field_style(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.lighter_background)),
        border: Border {
            color: palette.light_foreground,
            width: 1.5,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn publish_editor_style(
    palette: &Palette,
    _theme: &Theme,
    _status: text_editor::Status,
) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Palette::lightened(palette.lighter_background, 0.18)),
        border: Border::default(),
        placeholder: palette.foreground,
        value: palette.foreground,
        selection: Palette::lightened(palette.selection, 0.15),
    }
}

fn add_button_style(palette: &Palette, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(
                Palette::lightened(palette.darker_background, 0.3),
            )),
            _ => Some(Background::Color(palette.darker_background)),
        },
        border: Border {
            color: match status {
                button::Status::Hovered | button::Status::Pressed => palette.light_foreground,
                _ => palette.muted,
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn publish_button_style(palette: &Palette, status: button::Status) -> button::Style {
    let base = Palette::darkened(palette.blue, 0.55);
    let hover = Palette::darkened(palette.blue, 0.4);

    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(hover)),
            _ => Some(Background::Color(base)),
        },
        border: Border {
            color: Palette::lightened(palette.blue, 0.2),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        comment_card_style, edge_separator_style, publish_editor_style, sidebar_style, view,
        ViewContext,
    };
    use crate::comments::{update, Context, Message, State};
    use crate::preview::{CaretPosition, ElementMap};
    use crate::theme::Palette;
    use iced::widget::text_editor::{self, Action, Edit};
    use iced::{Background, Font, Theme};
    use std::sync::Arc;

    const FONT: Font = Font::with_name("iA Writer Mono S");

    fn context() -> Context {
        Context {
            caret: CaretPosition {
                element: 0,
                column: 0,
            },
            selection: None,
            composer_open: false,
        }
    }

    #[test]
    fn sidebar_view_builds_collapsed_and_full_from_comments_state() {
        let source = "# Title\n\nbody";
        let elements = ElementMap::parse(source);
        let view_context = || ViewContext {
            source,
            preview_elements: elements.elements(),
            palette: Palette::default(),
            font: FONT,
        };
        let mut state = State::new();

        let _ = view(&state, view_context());

        update(
            &mut state,
            Message::EditDraft(Action::Edit(Edit::Paste(Arc::new("overall".to_owned())))),
            context(),
        );
        update(&mut state, Message::AddDraftAsGlobal, context());

        let _ = view(&state, view_context());
    }

    #[test]
    fn sidebar_card_and_publish_styles_follow_the_comments_palette() {
        let palette = Palette::default();
        let sidebar = sidebar_style(&palette);
        let active = comment_card_style(&palette, true, false);
        let publish = publish_editor_style(&palette, &Theme::Dark, text_editor::Status::Active);

        assert_eq!(
            sidebar.background,
            Some(Background::Color(palette.dark_background))
        );
        assert_eq!(sidebar.border.width, 0.0);
        assert_eq!(
            edge_separator_style(palette).background,
            Some(Background::Color(palette.muted))
        );
        assert_eq!(
            active.background,
            Some(Background::Color(palette.tint(palette.accent, 0.1)))
        );
        assert_eq!(publish.value, palette.foreground);
        assert_eq!(
            publish.selection,
            Palette::lightened(palette.selection, 0.15)
        );
    }
}
