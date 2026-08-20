//! Markdown preview rendering and its scrollable surface.
//!
//! The viewer owns the correspondence between parsed Markdown items and the
//! preview model's numbered elements. The app supplies only read-only comment,
//! find, and palette collaborators and maps the resulting preview-local
//! messages at the composition boundary.

use std::ops::Range;

use super::scroll::{caret_id, scrollable_id};
use super::{element_selection, Message, PreviewElement, State};
use crate::{comments, editing, interactive_text, theme::Palette};

use iced::widget::markdown::Catalog as _;
use iced::widget::{container, markdown, scrollable};
use iced::{Color, Element, Font, Length, Theme};

const PREVIEW_FONT: Font = Font::with_name("iA Writer Mono S");
const COMMENTED_BAR_WIDTH: f32 = 3.0;
const ACTIVE_COMMENT_BAR_WIDTH: f32 = 4.0;

/// The background of a visual-mode selection. Translucent blue: the white
/// preview text stays clearly readable on top of it.
const VISUAL_SELECTION_COLOR: Color = Color::from_rgba(0.25, 0.5, 1.0, 0.4);

/// The background of a find match: translucent amber, distinct from
/// the selection blue.
const FIND_MATCH_COLOR: Color = Color::from_rgba(0.95, 0.75, 0.25, 0.4);

/// The background of the current find match: the same amber, brighter
/// and more opaque, so it stands out from the rest of the matches.
const CURRENT_FIND_MATCH_COLOR: Color = Color::from_rgba(0.98, 0.62, 0.15, 0.75);

/// Read-only collaborators needed to decorate and style the preview surface.
///
/// The comments state is borrowed only for its decoration queries; find data
/// is passed as the already-resolved query/current match rather than exposing
/// the app's find controller to the preview feature.
pub(crate) struct ViewContext<'a> {
    comments: &'a comments::State,
    find_query: &'a str,
    current_find_match: Option<(usize, Range<usize>)>,
    palette: Palette,
}

impl<'a> ViewContext<'a> {
    pub(crate) fn new(
        comments: &'a comments::State,
        find_query: &'a str,
        current_find_match: Option<(usize, Range<usize>)>,
        palette: Palette,
    ) -> Self {
        Self {
            comments,
            find_query,
            current_find_match,
            palette,
        }
    }
}

/// Builds the complete preview surface, including the hidden-scrollbar
/// scrollable targeted by preview-local scroll operations.
pub(crate) fn view<'a>(state: &'a State, context: ViewContext<'a>) -> Element<'a, Message> {
    let position = state.caret();
    let visual = state.visual_selection();
    let palette = context.palette;

    scrollable(
        container(markdown::view_with(
            state.markdown().items(),
            markdown::Settings::with_text_size(20.0, markdown_style(&palette)),
            &PreviewViewer {
                claims: state.claims(),
                focused_element: position.element,
                caret_column: position.column,
                visual,
                comments: context.comments,
                find_query: context.find_query,
                current_match: context.current_find_match,
                text_color: palette.foreground,
                comment_colors: comment_colors(&palette),
            },
        ))
        .width(Length::Fill)
        .padding([0, 8]),
    )
    .id(scrollable_id())
    .direction(scrollable::Direction::Vertical(
        scrollable::Scrollbar::hidden(),
    ))
    .height(Length::Fill)
    .into()
}

fn markdown_style(palette: &Palette) -> markdown::Style {
    let theme = if palette.light {
        &Theme::Light
    } else {
        &Theme::Dark
    };

    markdown::Style {
        font: PREVIEW_FONT,
        inline_code_font: PREVIEW_FONT,
        code_block_font: PREVIEW_FONT,
        ..markdown::Style::from(theme)
    }
}

#[derive(Debug, Clone, Copy)]
struct CommentColors {
    commented_bar: Color,
    active_bar: Color,
    active_tint: Color,
    commented_span_tint: Color,
}

fn comment_colors(palette: &Palette) -> CommentColors {
    CommentColors {
        // Open comments keep the palette's warning/annotation color, while
        // the active comment uses the same accent as its sidebar card.
        commented_bar: palette.tint(palette.yellow, 0.75),
        active_bar: palette.accent,
        active_tint: palette.tint(palette.accent, 0.09),
        commented_span_tint: palette.tint(palette.yellow, 0.22),
    }
}

struct PreviewViewer<'a> {
    /// Claims the next numbered element per rendered item from the map —
    /// the viewer never counts itself, so the numbering parity between the
    /// parse walk and the viewer lives in the preview module alone.
    claims: super::Claims<'a>,
    focused_element: usize,
    caret_column: usize,
    /// The `(anchor, caret)` endpoints of the visual-mode selection.
    visual: Option<(super::CaretPosition, super::CaretPosition)>,
    /// Saved comments, to know which elements carry one.
    comments: &'a comments::State,
    /// The find popup's query; its matches paint as highlights.
    find_query: &'a str,
    /// The current find match, as the element and range it lives in.
    current_match: Option<(usize, Range<usize>)>,
    /// The color the caret and decorations paint with — the palette's
    /// foreground.
    text_color: Color,
    /// The current theme's comment bar and tint colors.
    comment_colors: CommentColors,
}

impl<'a> markdown::Viewer<'a, Message> for PreviewViewer<'a> {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::LinkClicked(url)
    }

    fn heading(
        &self,
        mut settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text: &'a markdown::Text,
        _index: usize,
    ) -> Element<'a, Message> {
        settings.text_size = match level {
            markdown::HeadingLevel::H1 => settings.h1_size,
            markdown::HeadingLevel::H2 => settings.h2_size,
            markdown::HeadingLevel::H3 => settings.h3_size,
            markdown::HeadingLevel::H4 => settings.h4_size,
            markdown::HeadingLevel::H5 => settings.h5_size,
            markdown::HeadingLevel::H6 => settings.h6_size,
        };
        self.text_element(settings, text)
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        self.text_element(settings, text)
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        // The map numbers code blocks like any element; if they ever
        // disagree, fall back to the plain, non-interactive look.
        let Some((element, preview_element)) = self.claims.claim() else {
            return markdown::code_block(settings, lines, Message::LinkClicked);
        };

        let decorations = self.decorations(element, preview_element);

        // The code block keeps the default look — dark surface, inset —
        // with the interactive code inside instead of the plain lines.
        container(interactive_text::code(
            settings,
            preview_element.text(),
            decorations,
        ))
        .width(Length::Fill)
        .padding(settings.code_size / 4.0)
        .class(Theme::code_block())
        .into()
    }
}

impl<'a> PreviewViewer<'a> {
    /// The decorations the claimed `element` carries: its slice of the
    /// visual selection, the caret when focused, its comment mark and
    /// anchored span, and its find matches.
    fn decorations(
        &self,
        element: usize,
        preview_element: &PreviewElement,
    ) -> interactive_text::TextDecorations {
        let mark = self.comments.mark_for(element, preview_element.len());
        let whole_element_background =
            (mark == comments::Mark::Active).then_some(interactive_text::WholeElementBackground {
                color: self.comment_colors.active_tint,
            });
        let gutters = match mark {
            comments::Mark::Active => vec![interactive_text::GutterDecoration {
                width: ACTIVE_COMMENT_BAR_WIDTH,
                color: self.comment_colors.active_bar,
            }],
            comments::Mark::Commented => vec![interactive_text::GutterDecoration {
                width: COMMENTED_BAR_WIDTH,
                color: self.comment_colors.commented_bar,
            }],
            comments::Mark::None => Vec::new(),
        };

        // Ranged backgrounds are appended in paint order: selected comment
        // span, ordinary find matches, current find match, visual selection.
        let mut ranged_backgrounds = Vec::new();

        if let Some(range) = self
            .comments
            .anchor_selection_for(element, preview_element.len())
        {
            ranged_backgrounds.push(interactive_text::RangedBackground {
                range,
                color: self.comment_colors.commented_span_tint,
            });
        }

        if !self.find_query.is_empty() {
            ranged_backgrounds.extend(
                editing::matches_in(preview_element.text(), self.find_query)
                    .into_iter()
                    .map(|range| interactive_text::RangedBackground {
                        range,
                        color: FIND_MATCH_COLOR,
                    }),
            );
        }

        if let Some((match_element, range)) = &self.current_match {
            if *match_element == element {
                ranged_backgrounds.push(interactive_text::RangedBackground {
                    range: range.clone(),
                    color: CURRENT_FIND_MATCH_COLOR,
                });
            }
        }

        if let Some(range) = self.visual.and_then(|(anchor, caret)| {
            element_selection(anchor, caret, element, preview_element.len())
        }) {
            ranged_backgrounds.push(interactive_text::RangedBackground {
                range,
                color: VISUAL_SELECTION_COLOR,
            });
        }

        let focused = self.focused_element == element;

        interactive_text::TextDecorations {
            whole_element_background,
            gutters,
            ranged_backgrounds,
            caret: focused.then(|| interactive_text::CaretDecoration {
                column: self.caret_column,
                color: self.text_color,
                id: Some(caret_id()),
            }),
        }
    }

    fn text_element(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        // The map numbers elements exactly like the viewer numbers items;
        // if they ever disagree the item renders plainly, without caret,
        // selection, or comment mark.
        let Some((element, preview_element)) = self.claims.claim() else {
            return interactive_text::paragraph(
                settings,
                text,
                interactive_text::TextDecorations::default(),
            );
        };

        let decorations = self.decorations(element, preview_element);

        interactive_text::paragraph(settings, text, decorations)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        comment_colors, PreviewViewer, ACTIVE_COMMENT_BAR_WIDTH, CURRENT_FIND_MATCH_COLOR,
        FIND_MATCH_COLOR, VISUAL_SELECTION_COLOR,
    };
    use crate::preview::{CaretPosition, State};
    use crate::theme::Palette;
    use crate::{comments, interactive_text};
    use iced::widget::text_editor::{Action, Edit};
    use std::sync::Arc;

    fn at(element: usize, column: usize) -> CaretPosition {
        CaretPosition { element, column }
    }

    #[test]
    fn viewer_constructs_the_generic_decoration_model_in_layer_order() {
        let preview = State::new("alpha beta");
        let mut comments = comments::State::new();
        let comment_context = comments::Context {
            caret: at(0, 6),
            selection: Some((at(0, 1), at(0, 6))),
            composer_open: true,
        };
        comments::update(
            &mut comments,
            comments::Message::EditComposer(Action::Edit(Edit::Paste(Arc::new("note".to_owned())))),
            comment_context,
        );
        comments::update(
            &mut comments,
            comments::Message::SaveComposer,
            comment_context,
        );

        let palette = Palette::default();
        let viewer = PreviewViewer {
            claims: preview.claims(),
            focused_element: 0,
            caret_column: 7,
            visual: Some((at(0, 2), at(0, 8))),
            comments: &comments,
            find_query: "alpha",
            current_match: Some((0, 0..5)),
            text_color: palette.foreground,
            comment_colors: comment_colors(&palette),
        };

        let decorations = viewer.decorations(0, &preview.elements()[0]);
        let colors = comment_colors(&palette);

        assert_eq!(
            decorations.whole_element_background,
            Some(interactive_text::WholeElementBackground {
                color: colors.active_tint,
            })
        );
        assert_eq!(
            decorations.gutters,
            vec![interactive_text::GutterDecoration {
                width: ACTIVE_COMMENT_BAR_WIDTH,
                color: colors.active_bar,
            }]
        );
        assert_eq!(
            decorations.ranged_backgrounds,
            vec![
                interactive_text::RangedBackground {
                    range: 1..6,
                    color: colors.commented_span_tint,
                },
                interactive_text::RangedBackground {
                    range: 0..5,
                    color: FIND_MATCH_COLOR,
                },
                interactive_text::RangedBackground {
                    range: 0..5,
                    color: CURRENT_FIND_MATCH_COLOR,
                },
                interactive_text::RangedBackground {
                    range: 2..8,
                    color: VISUAL_SELECTION_COLOR,
                },
            ]
        );

        let caret = decorations.caret.expect("focused element has a caret");
        assert_eq!(caret.column, 7);
        assert_eq!(caret.color, palette.foreground);
        assert!(caret.id.is_some());
    }
}
