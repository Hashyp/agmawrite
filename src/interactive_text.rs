use iced::advanced::text::{Paragraph as _, Renderer as _};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{layout, renderer, Layout, Renderer as _, Widget};
use iced::widget::{markdown, text, Id};
use iced::{
    Background, Border, Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Size,
    Theme, Vector,
};
use unicode_segmentation::UnicodeSegmentation;

const PARAGRAPH_PADDING: f32 = 4.0;

/// A caret painted at a grapheme column. The optional widget ID lets scroll
/// operations locate the element containing the caret.
#[derive(Debug, Clone)]
pub struct CaretDecoration {
    pub column: usize,
    pub color: Color,
    pub id: Option<Id>,
}

/// A background painted behind a grapheme range.
#[derive(Debug, Clone, PartialEq)]
pub struct RangedBackground {
    pub range: std::ops::Range<usize>,
    pub color: Color,
}

/// A background tint covering the whole interactive element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WholeElementBackground {
    pub color: Color,
}

/// A vertical bar painted at the left edge of the interactive element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GutterDecoration {
    pub width: f32,
    pub color: Color,
}

/// Generic decorations painted around Markdown text.
///
/// Ranged backgrounds are painted in vector order. The complete layer order
/// is Markdown span backgrounds, the whole-element background and gutters,
/// ranged backgrounds, the caret, and finally the text.
#[derive(Debug, Clone, Default)]
pub struct TextDecorations {
    pub whole_element_background: Option<WholeElementBackground>,
    pub gutters: Vec<GutterDecoration>,
    pub ranged_backgrounds: Vec<RangedBackground>,
    pub caret: Option<CaretDecoration>,
}

pub fn paragraph<'a, M: 'a>(
    settings: markdown::Settings,
    text: &markdown::Text,
    decorations: TextDecorations,
) -> Element<'a, M> {
    let spans: Vec<_> = text.spans(settings.style).iter().cloned().collect();

    Element::new(InteractiveText {
        spans,
        decorations,
        size: settings.text_size,
        line_height: iced::advanced::text::LineHeight::default(),
        font: settings.style.font,
        _message: std::marker::PhantomData,
    })
}

/// A fenced code block in the preview: the same generic decorations as a
/// text element over monospace code.
pub fn code<'a, M: 'a>(
    settings: markdown::Settings,
    code: &str,
    decorations: TextDecorations,
) -> Element<'a, M> {
    let span: text::Span<'static, markdown::Uri, Font> =
        text::Span::new(code.to_owned()).font(settings.style.code_block_font);

    Element::new(InteractiveText {
        spans: vec![span],
        decorations,
        size: settings.code_size,
        line_height: iced::advanced::text::LineHeight::default(),
        font: settings.style.code_block_font,
        _message: std::marker::PhantomData,
    })
}

struct InteractiveText<M> {
    spans: Vec<text::Span<'static, markdown::Uri, Font>>,
    decorations: TextDecorations,
    size: Pixels,
    line_height: iced::advanced::text::LineHeight,
    font: Font,
    /// The widget never emits a message; the type parameter only keeps the
    /// dependency pointing one way, from the app root to this leaf module.
    _message: std::marker::PhantomData<M>,
}

type RendererParagraph = <Renderer as iced::advanced::text::Renderer>::Paragraph;

struct State {
    spans: Vec<text::Span<'static, markdown::Uri, Font>>,
    paragraph: RendererParagraph,
}

impl<M> Widget<M, Theme, Renderer> for InteractiveText<M> {
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
        _cursor: iced::advanced::mouse::Cursor,
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

        // Whole-element decorations paint above Markdown span backgrounds and
        // below every ranged annotation.
        let bounds = layout.bounds();

        if let Some(background) = self.decorations.whole_element_background {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..Default::default()
                },
                background.color,
            );
        }

        for gutter in &self.decorations.gutters {
            draw_gutter(renderer, bounds, gutter.width, gutter.color);
        }

        let text: String = self.spans.iter().map(|span| span.text.as_ref()).collect();
        let line_height = self.line_height.to_absolute(self.size).0;
        let width = state.paragraph.min_bounds().width;

        // Ranged annotations paint in model order. The preview supplies the
        // comment span, ordinary find matches, current find match, and visual
        // selection in that order.
        for background in &self.decorations.ranged_backgrounds {
            for bounds in selection_rects(
                &state.paragraph,
                &text,
                background.range.clone(),
                width,
                line_height,
            ) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: bounds + translation,
                        ..Default::default()
                    },
                    background.color,
                );
            }
        }

        if let Some(caret_decoration) = &self.decorations.caret {
            let origin = layout.position() + text_offset;
            let point = caret_point(
                &state.paragraph,
                &text,
                caret_decoration.column,
                width,
                line_height,
            );

            let caret = Rectangle::new(
                Point::new(origin.x + point.x, origin.y + point.y + 1.0),
                Size::new(2.0, (line_height - 2.0).max(2.0)),
            );
            renderer.fill_quad(
                renderer::Quad {
                    bounds: caret,
                    ..Default::default()
                },
                caret_decoration.color,
            );
        }

        renderer.fill_paragraph(
            &state.paragraph,
            layout.position() + text_offset,
            defaults.text_color,
            *viewport,
        );
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(id) = self
            .decorations
            .caret
            .as_ref()
            .and_then(|caret| caret.id.as_ref())
        {
            operation.container(Some(id), layout.bounds());
        }
    }
}

/// Draws a left-edge gutter on an interactive element.
fn draw_gutter(renderer: &mut Renderer, bounds: Rectangle, width: f32, color: Color) {
    let bar = Rectangle::new(
        Point::new(bounds.x, bounds.y + 1.0),
        Size::new(width, (bounds.height - 2.0).max(2.0)),
    );

    renderer.fill_quad(
        renderer::Quad {
            bounds: bar,
            ..Default::default()
        },
        color,
    );
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

#[cfg(test)]
mod tests {
    use super::{RangedBackground, TextDecorations};
    use iced::Color;

    #[test]
    fn text_decorations_preserve_ranged_background_insertion_order() {
        let first = RangedBackground {
            range: 1..3,
            color: Color::BLACK,
        };
        let second = RangedBackground {
            range: 2..5,
            color: Color::WHITE,
        };
        let decorations = TextDecorations {
            ranged_backgrounds: vec![first.clone(), second.clone()],
            ..TextDecorations::default()
        };

        assert_eq!(decorations.ranged_backgrounds, vec![first, second]);
        assert!(decorations.whole_element_background.is_none());
        assert!(decorations.gutters.is_empty());
        assert!(decorations.caret.is_none());
    }
}
