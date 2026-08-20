//! Markdown preview rendering and its scrollable surface.
//!
//! The viewer owns the correspondence between parsed Markdown items and the
//! preview model's numbered elements. The app supplies only read-only comment,
//! find, and palette collaborators and maps the resulting preview-local
//! messages at the composition boundary.

use std::ops::Range;

use super::decorations::{self, Config, Pipeline};
use super::scroll::{caret_id, scrollable_id};
use super::{Message, PreviewElement, State};
use crate::{comments, interactive_text, theme::Palette};

use iced::widget::markdown::Catalog as _;
use iced::widget::{container, markdown, scrollable};
use iced::{Element, Font, Length, Theme};

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
                decoration_config: Config::from_palette(&palette),
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
    /// Producer colors and geometry derived from the current theme policy.
    decoration_config: Config,
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
    /// Assembles the ordered producer pipeline shared by paragraphs and code
    /// blocks. Producers append generic primitives; this viewer only supplies
    /// their read-only collaborators and registration order.
    fn decorations(
        &self,
        element: usize,
        preview_element: &PreviewElement,
    ) -> interactive_text::TextDecorations {
        Pipeline::new()
            // Preserve paint order: comment element/span, ordinary/current
            // find matches, visual selection, then caret.
            .append(|output| {
                decorations::append_comments(
                    output,
                    self.comments,
                    element,
                    preview_element.len(),
                    &self.decoration_config,
                )
            })
            .append(|output| {
                decorations::append_find(
                    output,
                    preview_element,
                    element,
                    self.find_query,
                    self.current_match.as_ref(),
                    &self.decoration_config,
                )
            })
            .append(|output| {
                decorations::append_visual(
                    output,
                    element,
                    preview_element.len(),
                    self.visual,
                    &self.decoration_config,
                )
            })
            .append(|output| {
                decorations::append_caret(
                    output,
                    element,
                    self.focused_element,
                    self.caret_column,
                    caret_id(),
                    &self.decoration_config,
                )
            })
            .finish()
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
