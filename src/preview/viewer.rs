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
use iced::widget::{container, markdown, scrollable, Id};
use iced::{Color, Element, Font, Length, Theme};

const PREVIEW_FONT: Font = Font::with_name("iA Writer Mono S");

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

fn comment_colors(palette: &Palette) -> interactive_text::CommentColors {
    interactive_text::CommentColors {
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
    comment_colors: interactive_text::CommentColors,
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
            decorations.selection,
            decorations.caret,
            decorations.id,
            decorations.commented,
            decorations.active_comment,
            decorations.comment_span,
            self.comment_colors,
            self.text_color,
            decorations.find,
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
    fn decorations(&self, element: usize, preview_element: &PreviewElement) -> Decorations {
        let focused = self.focused_element == element;

        Decorations {
            selection: self.visual.and_then(|(anchor, caret)| {
                element_selection(anchor, caret, element, preview_element.len())
            }),
            caret: focused.then_some(self.caret_column),
            id: focused.then(caret_id),
            commented: matches!(
                self.comments.mark_for(element, preview_element.len()),
                comments::Mark::Commented | comments::Mark::Active
            ),
            active_comment: self.comments.mark_for(element, preview_element.len())
                == comments::Mark::Active,
            comment_span: self
                .comments
                .anchor_selection_for(element, preview_element.len()),
            find: interactive_text::FindHighlights {
                matches: if self.find_query.is_empty() {
                    Vec::new()
                } else {
                    editing::matches_in(preview_element.text(), self.find_query)
                },
                current: match self.current_match {
                    Some((match_element, ref range)) if match_element == element => {
                        Some(range.clone())
                    }
                    _ => None,
                },
            },
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
                None,
                None,
                None,
                false,
                false,
                None,
                self.comment_colors,
                self.text_color,
                interactive_text::FindHighlights::none(),
            );
        };

        let decorations = self.decorations(element, preview_element);

        interactive_text::paragraph(
            settings,
            text,
            decorations.selection,
            decorations.caret,
            decorations.id,
            decorations.commented,
            decorations.active_comment,
            decorations.comment_span,
            self.comment_colors,
            self.text_color,
            decorations.find,
        )
    }
}

/// The interactive decorations of one preview element: its slice of the
/// visual-mode selection, the caret when it is the focused element, its
/// comment mark and anchored span, and its find highlights.
struct Decorations {
    selection: Option<Range<usize>>,
    caret: Option<usize>,
    id: Option<Id>,
    commented: bool,
    active_comment: bool,
    /// The selected-text span a comment was written for, when its anchor
    /// is a span.
    comment_span: Option<Range<usize>>,
    find: interactive_text::FindHighlights,
}

#[cfg(test)]
mod tests {
    use super::{comment_colors, Decorations, PreviewViewer};
    use crate::comments;
    use crate::preview::{CaretPosition, State};
    use crate::theme::Palette;
    use iced::widget::text_editor::{Action, Edit};
    use std::sync::Arc;

    fn at(element: usize, column: usize) -> CaretPosition {
        CaretPosition { element, column }
    }

    #[test]
    fn viewer_preserves_caret_visual_comment_and_find_decorations() {
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

        let Decorations {
            selection,
            caret,
            id,
            commented,
            active_comment,
            comment_span,
            find,
        } = viewer.decorations(0, &preview.elements()[0]);

        assert_eq!(selection, Some(2..8));
        assert_eq!(caret, Some(7));
        assert!(id.is_some());
        assert!(commented);
        assert!(active_comment);
        assert_eq!(comment_span, Some(1..6));
        assert_eq!(find.matches, vec![0..5]);
        assert_eq!(find.current, Some(0..5));
    }
}
