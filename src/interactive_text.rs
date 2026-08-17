use std::borrow::Cow;
use std::collections::HashSet;

use iced::advanced::text::{Paragraph as _, Renderer as _};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{layout, mouse, renderer, Clipboard, Layout, Renderer as _, Shell, Widget};
use iced::widget::{markdown, text, Id};
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Renderer,
    Size, Theme, Vector,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{Message, ParagraphSelection, WordSelection};

const PARAGRAPH_PADDING: f32 = 4.0;

/// The background of a visual-mode selection. Translucent blue: the white
/// preview text stays clearly readable on top of it.
const VISUAL_SELECTION_COLOR: Color = Color::from_rgba(0.25, 0.5, 1.0, 0.4);

#[allow(clippy::too_many_arguments)]
pub fn paragraph<'a>(
    settings: markdown::Settings,
    text: &markdown::Text,
    paragraph: ParagraphSelection,
    selected_words: &'a HashSet<WordSelection>,
    selected_paragraphs: &'a HashSet<ParagraphSelection>,
    selection: Option<std::ops::Range<usize>>,
    caret: Option<usize>,
    id: Option<Id>,
) -> Element<'a, Message> {
    let mut spans = Vec::new();
    let mut words = Vec::new();
    let mut word_index = 0;

    for source in text.spans(settings.style).iter() {
        for (_, part) in source.text.as_ref().split_word_bound_indices() {
            if part.is_empty() {
                continue;
            }

            let is_word = part.chars().any(char::is_alphanumeric);
            let mut span = source.clone();
            span.text = Cow::Owned(part.to_owned());
            spans.push(span);

            if is_word {
                words.push(Some(WordSelection {
                    paragraph: paragraph.0,
                    word: word_index,
                }));
                word_index += 1;
            } else {
                words.push(None);
            }
        }
    }

    Element::new(InteractiveText {
        spans,
        words,
        paragraph,
        selected_words,
        selected_paragraphs,
        selection,
        caret,
        id,
        size: settings.text_size,
        line_height: iced::advanced::text::LineHeight::default(),
        font: settings.style.font,
    })
}

struct InteractiveText<'a> {
    spans: Vec<text::Span<'static, markdown::Uri, Font>>,
    words: Vec<Option<WordSelection>>,
    paragraph: ParagraphSelection,
    selected_words: &'a HashSet<WordSelection>,
    selected_paragraphs: &'a HashSet<ParagraphSelection>,
    /// The grapheme columns of a visual-mode selection within this element.
    selection: Option<std::ops::Range<usize>>,
    caret: Option<usize>,
    id: Option<Id>,
    size: Pixels,
    line_height: iced::advanced::text::LineHeight,
    font: Font,
}

type RendererParagraph = <Renderer as iced::advanced::text::Renderer>::Paragraph;

struct State {
    spans: Vec<text::Span<'static, markdown::Uri, Font>>,
    paragraph: RendererParagraph,
}

impl Widget<Message, Theme, Renderer> for InteractiveText<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            spans: Vec::new(),
            paragraph: Default::default(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();

        layout::sized(limits, Length::Shrink, Length::Shrink, |limits| {
            let available = limits.max();
            let bounds = Size::new(
                (available.width - PARAGRAPH_PADDING * 2.0).max(0.0),
                (available.height - PARAGRAPH_PADDING * 2.0).max(0.0),
            );
            let content = || iced::advanced::Text {
                content: self.spans.as_slice(),
                bounds,
                size: self.size,
                line_height: self.line_height,
                font: self.font,
                align_x: iced::advanced::text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: iced::advanced::text::Shaping::Advanced,
                wrapping: iced::advanced::text::Wrapping::Word,
            };

            if state.spans != self.spans {
                state.paragraph = RendererParagraph::with_spans(content());
                state.spans = self.spans.clone();
            } else {
                match state.paragraph.compare(iced::advanced::Text {
                    content: (),
                    bounds,
                    size: self.size,
                    line_height: self.line_height,
                    font: self.font,
                    align_x: iced::advanced::text::Alignment::Default,
                    align_y: iced::alignment::Vertical::Top,
                    shaping: iced::advanced::text::Shaping::Advanced,
                    wrapping: iced::advanced::text::Wrapping::Word,
                }) {
                    iced::advanced::text::Difference::None => {}
                    iced::advanced::text::Difference::Bounds => {
                        state.paragraph.resize(bounds);
                    }
                    iced::advanced::text::Difference::Shape => {
                        state.paragraph = RendererParagraph::with_spans(content());
                    }
                }
            }

            state.paragraph.min_bounds()
                + Size::new(PARAGRAPH_PADDING * 2.0, PARAGRAPH_PADDING * 2.0)
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<State>();
        let text_offset = Vector::new(PARAGRAPH_PADDING, PARAGRAPH_PADDING);
        let translation = layout.position() - Point::ORIGIN + text_offset;

        // Preserve Markdown's own decorations, such as inline-code backgrounds.
        for (index, span) in self.spans.iter().enumerate() {
            if let Some(highlight) = span.highlight {
                draw_regions(
                    renderer,
                    state.paragraph.span_bounds(index),
                    translation,
                    span.padding,
                    highlight.border,
                    highlight.background,
                );
            }
        }

        let hovered = cursor
            .position_in(layout.bounds())
            .and_then(|position| state.paragraph.hit_span(position - text_offset))
            .and_then(|index| self.words.get(index).copied().flatten());
        let inside = cursor.is_over(layout.bounds());

        if self.selected_paragraphs.contains(&self.paragraph) {
            draw_outline(renderer, layout.bounds(), Color::from_rgb(0.9, 0.1, 0.1));
        } else if inside && hovered.is_none() {
            draw_outline(renderer, layout.bounds(), Color::from_rgb(0.0, 0.55, 0.2));
        }

        let text: String = self.spans.iter().map(|span| span.text.as_ref()).collect();
        let line_height = self.line_height.to_absolute(self.size).0;
        let width = state.paragraph.min_bounds().width;

        if let Some(selection) = self.selection.clone() {
            for bounds in selection_rects(&state.paragraph, &text, selection, width, line_height) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: bounds + translation,
                        ..Default::default()
                    },
                    VISUAL_SELECTION_COLOR,
                );
            }
        }

        if let Some(column) = self.caret {
            let origin = layout.position() + text_offset;
            let point = caret_point(&state.paragraph, &text, column, width, line_height);

            let caret = Rectangle::new(
                Point::new(origin.x + point.x, origin.y + point.y + 1.0),
                Size::new(2.0, (line_height - 2.0).max(2.0)),
            );
            renderer.fill_quad(
                renderer::Quad {
                    bounds: caret,
                    ..Default::default()
                },
                Color::WHITE,
            );
        }

        for (index, word) in self.words.iter().enumerate() {
            let Some(word) = word else { continue };
            let color = if self.selected_words.contains(word) {
                Some(Color::from_rgb(0.9, 0.1, 0.1))
            } else if hovered == Some(*word) {
                Some(Color::from_rgb(0.05, 0.15, 0.45))
            } else {
                None
            };

            if let Some(color) = color {
                draw_regions(
                    renderer,
                    state.paragraph.span_bounds(index),
                    translation,
                    iced::Padding::from(1.0),
                    Border {
                        color,
                        width: 1.5,
                        radius: 0.0.into(),
                    },
                    Background::Color(Color::TRANSPARENT),
                );
            }
        }

        renderer.fill_paragraph(
            &state.paragraph,
            layout.position() + text_offset,
            defaults.text_color,
            *viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if !matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            return;
        }

        let Some(position) = cursor.position_in(layout.bounds()) else {
            return;
        };

        let state = tree.state.downcast_ref::<State>();
        let word = state
            .paragraph
            .hit_span(position - Vector::new(PARAGRAPH_PADDING, PARAGRAPH_PADDING))
            .and_then(|index| self.words.get(index).copied().flatten());

        if let Some(word) = word {
            shell.publish(Message::SelectWord(word));
        } else {
            shell.publish(Message::SelectParagraph(self.paragraph));
        }
        shell.capture_event();
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(id) = self.id.as_ref() {
            operation.container(Some(id), layout.bounds());
        }
    }
}

/// The wrapped lines of a paragraph, as `(line index, byte start, byte
/// end, top y)` tuples. Lines are found by probing the paragraph hit test
/// at the middle of each line; their byte ranges partition the text.
fn wrapped_lines(
    paragraph: &RendererParagraph,
    width: f32,
    line_height: f32,
) -> Vec<(usize, usize, usize, f32)> {
    let mut lines = Vec::new();
    let mut line_y = 0.0;
    let mut line = 0;

    while line_y < paragraph.min_bounds().height {
        let mid_y = line_y + line_height / 2.0;

        let Some(start) = paragraph
            .hit_test(Point::new(0.0, mid_y))
            .map(|hit| hit.cursor())
        else {
            break;
        };

        let end = paragraph
            .hit_test(Point::new(width, mid_y))
            .map_or(start, |hit| hit.cursor());

        lines.push((line, start, end, line_y));

        line += 1;
        line_y += line_height;
    }

    lines
}

/// Locates the top-left point of the caret for the grapheme `column` of the
/// rendered `text`.
///
/// The horizontal position comes from the paragraph's exact per-grapheme
/// advances (`grapheme_position`) — inverting the hit test instead would
/// snap to glyph *centers* (the nearest boundary), which places the caret
/// half a glyph away from the boundary.
fn caret_point(
    paragraph: &RendererParagraph,
    text: &str,
    column: usize,
    width: f32,
    line_height: f32,
) -> Point {
    let total = text.graphemes(true).count();
    let column = column.min(total);
    let target = text
        .grapheme_indices(true)
        .nth(column)
        .map_or(text.len(), |(index, _)| index);

    for (line, start, end, line_y) in wrapped_lines(paragraph, width, line_height) {
        if start <= target && target <= end {
            let index_in_line = text[start..target].graphemes(true).count();

            let x = paragraph
                .grapheme_position(line, index_in_line)
                .map_or(0.0, |point| point.x);

            return Point::new(x, line_y);
        }
    }

    Point::ORIGIN
}

/// Computes the highlight rectangles for a visual-mode selection (given as
/// grapheme columns) across the wrapped lines of the rendered text. The
/// rectangles are paragraph-local; text is drawn on top of them later, so
/// the selection color can stay translucent.
fn selection_rects(
    paragraph: &RendererParagraph,
    text: &str,
    selection: std::ops::Range<usize>,
    width: f32,
    line_height: f32,
) -> Vec<Rectangle> {
    let total = text.graphemes(true).count();
    let start_column = selection.start.min(total);
    let end_column = selection.end.min(total).max(start_column);

    let byte_of = |column: usize| {
        text.grapheme_indices(true)
            .nth(column)
            .map_or(text.len(), |(index, _)| index)
    };

    let selection_start = byte_of(start_column);
    let selection_end = byte_of(end_column);

    let mut rects = Vec::new();

    for (line, line_start, line_end, line_y) in wrapped_lines(paragraph, width, line_height) {
        let lo = line_start.max(selection_start);
        let hi = line_end.min(selection_end);

        if hi <= lo {
            continue;
        }

        let index_start = text[line_start..lo].graphemes(true).count();
        let index_end = text[line_start..hi].graphemes(true).count();

        let x0 = paragraph
            .grapheme_position(line, index_start)
            .map_or(0.0, |point| point.x);
        let x1 = paragraph
            .grapheme_position(line, index_end)
            .map_or(x0, |point| point.x);

        if x1 > x0 {
            rects.push(Rectangle::new(
                Point::new(x0, line_y),
                Size::new(x1 - x0, line_height),
            ));
        }
    }

    rects
}

fn draw_outline(renderer: &mut Renderer, bounds: Rectangle, color: Color) {
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: Border {
                color,
                width: 1.5,
                radius: 0.0.into(),
            },
            ..Default::default()
        },
        Color::TRANSPARENT,
    );
}

fn draw_regions(
    renderer: &mut Renderer,
    regions: Vec<Rectangle>,
    translation: Vector,
    padding: iced::Padding,
    border: Border,
    background: Background,
) {
    for bounds in regions {
        let bounds = Rectangle::new(
            bounds.position() - Vector::new(padding.left, padding.top),
            bounds.size() + Size::new(padding.x(), padding.y()),
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: bounds + translation,
                border,
                ..Default::default()
            },
            background,
        );
    }
}
