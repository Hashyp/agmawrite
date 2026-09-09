//! Composable producers for the generic decorations painted by preview text.
//!
//! Each producer knows about one annotation source and only appends generic
//! [`TextDecorations`] primitives. The Markdown viewer registers producers in
//! paint order; neither paragraphs nor code blocks need annotation-specific
//! arguments.

use std::ops::Range;

use iced::widget::Id;
use iced::Color;

use super::{element_selection, CaretPosition, PreviewElement};
use crate::interactive_text::{
    CaretDecoration, CurrentLine, RangedBackground, RangedOutline, TextDecorations,
    WholeElementBackground,
};
use crate::{comments, editing, theme::Palette};

const COMMENTED_BORDER_WIDTH: f32 = 1.0;
const ACTIVE_BORDER_WIDTH: f32 = 1.5;

/// Producer configuration for one preview theme.
///
/// Every color — caret, current line, visual-selection, comment, and find —
/// follows a palette role: the visual selection paints with the theme's own
/// selection color, the same role the write-mode editor selects with, the
/// current line tints the theme's lighter background — a barely-there step
/// off the page, fainter than any raised surface — and find highlights tint
/// the theme's yellow and orange.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Config {
    caret: Color,
    current_line: Color,
    visual_selection: Color,
    yank_flash: Color,
    commented_border: Color,
    active_border: Color,
    active_comment_tint: Color,
    commented_span_tint: Color,
    find_match: Color,
    current_find_match: Color,
    commented_border_width: f32,
    active_border_width: f32,
}

impl Config {
    pub(crate) fn from_palette(palette: &Palette) -> Self {
        Self::from_roles(palette)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_roles(&Palette::default())
    }
}

impl Config {
    fn from_roles(palette: &Palette) -> Self {
        Self {
            caret: palette.foreground,
            current_line: palette.tint(palette.lighter_background, 0.4),
            visual_selection: palette.selection,
            yank_flash: palette.tint(palette.yellow, 0.8),
            commented_border: palette.yellow,
            active_border: palette.accent,
            active_comment_tint: palette.tint(palette.accent, 0.09),
            commented_span_tint: palette.tint(palette.yellow, 0.22),
            find_match: palette.tint(palette.yellow, 0.4),
            current_find_match: palette.tint(palette.orange, 0.75),
            commented_border_width: COMMENTED_BORDER_WIDTH,
            active_border_width: ACTIVE_BORDER_WIDTH,
        }
    }
}

/// An ordered decoration assembly. Producers are run immediately and append
/// to the same primitive lists, so registration order is paint order.
#[derive(Debug, Default)]
pub(crate) struct Pipeline {
    output: TextDecorations,
}

impl Pipeline {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn append(mut self, producer: impl FnOnce(&mut TextDecorations)) -> Self {
        producer(&mut self.output);
        self
    }

    pub(crate) fn finish(self) -> TextDecorations {
        self.output
    }
}

/// Appends the current-line band, if this is the caret's element.
pub(crate) fn append_current_line(
    output: &mut TextDecorations,
    element: usize,
    focused_element: usize,
    column: usize,
    config: &Config,
) {
    if element == focused_element {
        output.current_line = Some(CurrentLine {
            column,
            color: config.current_line,
        });
    }
}

/// Appends the focused caret, if this is its element.
pub(crate) fn append_caret(
    output: &mut TextDecorations,
    element: usize,
    focused_element: usize,
    column: usize,
    id: Id,
    config: &Config,
) {
    if element == focused_element {
        output.caret = Some(CaretDecoration {
            column,
            color: config.caret,
            id: Some(id),
        });
    }
}

/// Appends this element's slice of the visual selection.
pub(crate) fn append_visual(
    output: &mut TextDecorations,
    element: usize,
    element_len: usize,
    visual: Option<(CaretPosition, CaretPosition)>,
    config: &Config,
) {
    if let Some(range) =
        visual.and_then(|(anchor, caret)| element_selection(anchor, caret, element, element_len))
    {
        output.ranged_backgrounds.push(RangedBackground {
            range,
            color: config.visual_selection,
        });
    }
}

/// Appends this element's slice of the yank flash — the afterglow of the
/// selection `y` copied, painted like Neovim's `vim.hl.on_yank` highlight.
pub(crate) fn append_yank_flash(
    output: &mut TextDecorations,
    element: usize,
    element_len: usize,
    flash: Option<(CaretPosition, CaretPosition)>,
    config: &Config,
) {
    if let Some(range) =
        flash.and_then(|(anchor, caret)| element_selection(anchor, caret, element, element_len))
    {
        output.ranged_backgrounds.push(RangedBackground {
            range,
            color: config.yank_flash,
        });
    }
}

/// Appends whole-element, outline, and selected-span primitives for comments:
/// the commented text is framed by a bordered rectangle — amber when merely
/// commented, cyan when active — with no fill of its own, so the tints and
/// backgrounds underneath stay exactly as they were.
pub(crate) fn append_comments(
    output: &mut TextDecorations,
    comments: &comments::State,
    element: usize,
    element_len: usize,
    anchor_id: Id,
    config: &Config,
) {
    if let comments::Mark::Active = comments.mark_for(element, element_len) {
        output.whole_element_background = Some(WholeElementBackground {
            color: config.active_comment_tint,
        });
    }

    for outline in comments.outlines_for(element, element_len) {
        let (color, width) = match outline.mark {
            comments::Mark::Active => (config.active_border, config.active_border_width),
            comments::Mark::Commented | comments::Mark::None => {
                (config.commented_border, config.commented_border_width)
            }
        };

        output.ranged_outlines.push(RangedOutline {
            range: outline.range,
            color,
            width,
        });
    }

    if let Some(range) = comments.anchor_selection_for(element, element_len) {
        output.ranged_backgrounds.push(RangedBackground {
            range,
            color: config.commented_span_tint,
        });
    }

    if comments.anchors_element(element, element_len) {
        output.reveal_id = Some(anchor_id);
    }
}

/// Appends ordinary matches followed by the current match, preserving their
/// established relative paint order.
pub(crate) fn append_find(
    output: &mut TextDecorations,
    preview_element: &PreviewElement,
    element: usize,
    query: &str,
    current_match: Option<&(usize, Range<usize>)>,
    config: &Config,
) {
    if !query.is_empty() {
        output.ranged_backgrounds.extend(
            editing::matches_in(preview_element.text(), query)
                .into_iter()
                .map(|range| RangedBackground {
                    range,
                    color: config.find_match,
                }),
        );
    }

    if let Some((match_element, range)) = current_match {
        if *match_element == element {
            output.ranged_backgrounds.push(RangedBackground {
                range: range.clone(),
                color: config.current_find_match,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_caret, append_comments, append_current_line, append_find, append_visual,
        append_yank_flash, Config, Pipeline,
    };
    use crate::comments;
    use crate::interactive_text::{
        CurrentLine, RangedBackground, RangedOutline, WholeElementBackground,
    };
    use crate::preview::{CaretPosition, State};
    use crate::theme::Palette;
    use iced::widget::text_editor::{Action, Edit};
    use iced::widget::Id;
    use iced::Color;
    use std::sync::Arc;

    fn at(element: usize, column: usize) -> CaretPosition {
        CaretPosition { element, column }
    }

    fn comments_on(range: std::ops::Range<usize>) -> comments::State {
        let mut comments = comments::State::new();
        let context = comments::Context {
            caret: at(0, range.end),
            selection: Some((at(0, range.start), at(0, range.end))),
            composer_open: true,
        };
        comments::update(
            &mut comments,
            comments::Message::EditComposer(Action::Edit(Edit::Paste(Arc::new("note".to_owned())))),
            context,
        );
        comments::update(&mut comments, comments::Message::SaveComposer, context);
        comments
    }

    #[test]
    fn producer_configuration_uses_palette_roles_and_stable_annotation_defaults() {
        let defaults = Config::default();
        let mut palette = Palette::default();
        palette.foreground = palette.red;
        palette.accent = palette.blue;
        palette.yellow = palette.green;
        palette.selection = Color::from_rgb(0.11, 0.22, 0.33);

        let config = Config::from_palette(&palette);

        assert_eq!(config.caret, palette.foreground);
        // The current line tints the theme's lighter background — one subtle
        // step off the page, fainter than the solid raised surface.
        assert_eq!(
            config.current_line,
            palette.tint(palette.lighter_background, 0.4)
        );
        // The visual selection paints with the theme's own selection role —
        // the same color the write-mode editor selects with.
        assert_eq!(config.visual_selection, palette.selection);
        assert_eq!(config.active_border, palette.accent);
        assert_eq!(config.commented_border, palette.yellow);
        assert_eq!(
            config.active_comment_tint,
            palette.tint(palette.accent, 0.09)
        );
        assert_eq!(
            config.commented_span_tint,
            palette.tint(palette.yellow, 0.22)
        );
        // Find highlights tint the theme's yellow and orange, like every
        // other annotation.
        assert_eq!(config.find_match, palette.tint(palette.yellow, 0.4));
        assert_eq!(
            config.current_find_match,
            palette.tint(palette.orange, 0.75)
        );
        // The yank flash is an IncSearch-style highlight: the same yellow
        // family as find matches, but brighter than any of them.
        assert_eq!(config.yank_flash, palette.tint(palette.yellow, 0.8));
        // The geometry defaults stay fixed.
        assert_eq!(
            config.commented_border_width,
            defaults.commented_border_width
        );
        assert_eq!(config.active_border_width, defaults.active_border_width);
    }

    #[test]
    fn yank_flash_paints_exactly_the_yanked_span() {
        let preview = State::new("alpha beta");
        let preview_element = &preview.elements()[0];
        let config = Config::from_palette(&Palette::default());
        let mut output = crate::interactive_text::TextDecorations::default();

        append_yank_flash(
            &mut output,
            0,
            preview_element.len(),
            Some((at(0, 11), at(0, 5))),
            &config,
        );

        assert_eq!(
            output.ranged_backgrounds,
            vec![RangedBackground {
                range: 5..10,
                color: config.yank_flash,
            }]
        );
    }

    #[test]
    fn overlapping_comment_find_and_visual_primitives_follow_pipeline_order() {
        let preview = State::new("alpha alpha");
        let preview_element = &preview.elements()[0];
        let comments = comments_on(1..10);
        let palette = Palette::default();
        let config = Config::from_palette(&palette);
        let current_match = (0, 6..11);
        let anchor = Id::unique();

        let decorations = Pipeline::new()
            .append(|output| {
                append_current_line(output, 0, 0, 7, &config);
            })
            .append(|output| {
                append_comments(
                    output,
                    &comments,
                    0,
                    preview_element.len(),
                    anchor.clone(),
                    &config,
                )
            })
            .append(|output| {
                append_find(
                    output,
                    preview_element,
                    0,
                    "alpha",
                    Some(&current_match),
                    &config,
                )
            })
            .append(|output| {
                append_visual(
                    output,
                    0,
                    preview_element.len(),
                    Some((at(0, 3), at(0, 9))),
                    &config,
                )
            })
            .append(|output| append_caret(output, 0, 0, 7, Id::unique(), &config))
            .finish();

        assert_eq!(
            decorations.ranged_backgrounds,
            vec![
                RangedBackground {
                    range: 1..10,
                    color: config.commented_span_tint,
                },
                RangedBackground {
                    range: 0..5,
                    color: config.find_match,
                },
                RangedBackground {
                    range: 6..11,
                    color: config.find_match,
                },
                RangedBackground {
                    range: 6..11,
                    color: config.current_find_match,
                },
                RangedBackground {
                    range: 3..9,
                    color: config.visual_selection,
                },
            ]
        );
        // The active comment frames exactly its selected span — no gutter
        // bars, no extra cursor at the span's start.
        assert_eq!(
            decorations.ranged_outlines,
            vec![RangedOutline {
                range: 1..10,
                color: config.active_border,
                width: config.active_border_width,
            }]
        );
        assert_eq!(
            decorations.whole_element_background,
            Some(WholeElementBackground {
                color: config.active_comment_tint,
            })
        );
        assert_eq!(
            decorations.current_line,
            Some(CurrentLine {
                column: 7,
                color: config.current_line,
            })
        );
        assert_eq!(decorations.reveal_id, Some(anchor));
        assert_eq!(
            decorations.caret.as_ref().map(|caret| caret.column),
            Some(7)
        );
    }

    /// Commented threads frame their span with the commented border, the
    /// active one on top; spot anchors frame their whole element, and only
    /// the active comment's element carries the reveal id.
    #[test]
    fn comments_frame_spans_and_elements_without_extra_cursors() {
        let preview = State::new("alpha alpha\n\nbeta");
        let first = &preview.elements()[0];
        let second = &preview.elements()[1];
        let mut comments = comments::State::new();

        let save = |comments: &mut comments::State, text: &str, context: comments::Context| {
            comments::update(
                comments,
                comments::Message::EditComposer(Action::Edit(Edit::Paste(Arc::new(
                    text.to_owned(),
                )))),
                context,
            );
            comments::update(comments, comments::Message::SaveComposer, context);
        };

        // Two distinct spans over the same element start two threads; a
        // spot note on another element starts a third, active one.
        save(
            &mut comments,
            "first",
            comments::Context {
                caret: at(0, 5),
                selection: Some((at(0, 0), at(0, 5))),
                composer_open: true,
            },
        );
        save(
            &mut comments,
            "second",
            comments::Context {
                caret: at(0, 7),
                selection: Some((at(0, 2), at(0, 7))),
                composer_open: true,
            },
        );
        save(
            &mut comments,
            "third",
            comments::Context {
                caret: at(1, 2),
                selection: None,
                composer_open: true,
            },
        );

        let config = Config::from_palette(&Palette::default());
        let anchor = Id::unique();
        let mut decorations = crate::interactive_text::TextDecorations::default();

        // Reactivating the middle thread: its span frames active, on top of
        // the older commented one.
        comments::update(
            &mut comments,
            comments::Message::ActivateCard(1, 0),
            comments::Context {
                caret: at(0, 0),
                selection: None,
                composer_open: false,
            },
        );

        append_comments(
            &mut decorations,
            &comments,
            0,
            first.len(),
            anchor.clone(),
            &config,
        );

        assert_eq!(
            decorations.ranged_outlines,
            vec![
                RangedOutline {
                    range: 0..5,
                    color: config.commented_border,
                    width: config.commented_border_width,
                },
                RangedOutline {
                    range: 2..7,
                    color: config.active_border,
                    width: config.active_border_width,
                },
            ]
        );
        assert_eq!(decorations.reveal_id, Some(anchor.clone()));

        // The spot thread merely comments its element now — framed whole,
        // without the active tint or reveal.
        let mut spot = crate::interactive_text::TextDecorations::default();
        append_comments(&mut spot, &comments, 1, second.len(), anchor, &config);

        assert_eq!(
            spot.ranged_outlines,
            vec![RangedOutline {
                range: 0..second.len(),
                color: config.commented_border,
                width: config.commented_border_width,
            }]
        );
        assert!(spot.whole_element_background.is_none());
        assert!(spot.reveal_id.is_none());
    }

    #[test]
    fn current_line_band_marks_only_the_carets_element() {
        let config = Config::from_palette(&Palette::default());
        let mut decorations = crate::interactive_text::TextDecorations::default();

        // An element the caret is not on carries no band.
        append_current_line(&mut decorations, 1, 0, 3, &config);
        assert_eq!(decorations.current_line, None);

        // The caret's element carries the band at its column.
        append_current_line(&mut decorations, 0, 0, 3, &config);
        assert_eq!(
            decorations.current_line,
            Some(CurrentLine {
                column: 3,
                color: config.current_line,
            })
        );
    }

    #[test]
    fn independent_ranged_producers_only_append_their_primitives() {
        let preview = State::new("alpha beta");
        let preview_element = &preview.elements()[0];
        let comments = comments_on(1..6);
        let config = Config::from_palette(&Palette::default());
        let sentinel = RangedBackground {
            range: 9..10,
            color: Palette::default().red,
        };
        let mut decorations = crate::interactive_text::TextDecorations {
            ranged_backgrounds: vec![sentinel.clone()],
            ..crate::interactive_text::TextDecorations::default()
        };

        append_comments(
            &mut decorations,
            &comments,
            0,
            preview_element.len(),
            Id::unique(),
            &config,
        );
        append_find(&mut decorations, preview_element, 0, "alpha", None, &config);
        append_visual(
            &mut decorations,
            0,
            preview_element.len(),
            Some((at(0, 2), at(0, 8))),
            &config,
        );

        assert_eq!(
            decorations
                .ranged_backgrounds
                .iter()
                .map(|background| background.range.clone())
                .collect::<Vec<_>>(),
            vec![9..10, 1..6, 0..5, 2..8]
        );
        assert_eq!(decorations.ranged_backgrounds[0], sentinel);
    }
}
