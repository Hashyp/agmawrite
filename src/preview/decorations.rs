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
    CaretDecoration, GutterDecoration, RangedBackground, TextDecorations, WholeElementBackground,
};
use crate::{comments, editing, theme::Palette};

const COMMENTED_BAR_WIDTH: f32 = 3.0;
const ACTIVE_COMMENT_BAR_WIDTH: f32 = 4.0;

/// Producer configuration for one preview theme.
///
/// Caret and comment colors follow palette roles. The palette has no dedicated
/// visual-selection or find-highlight roles, so their established colors live
/// in the default annotation policy. Keeping those defaults here preserves the
/// existing rendering while removing color policy from the viewer and leaf
/// widget.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Config {
    caret: Color,
    visual_selection: Color,
    commented_bar: Color,
    active_comment_bar: Color,
    active_comment_tint: Color,
    commented_span_tint: Color,
    find_match: Color,
    current_find_match: Color,
    commented_bar_width: f32,
    active_comment_bar_width: f32,
}

impl Config {
    pub(crate) fn from_palette(palette: &Palette) -> Self {
        let defaults = Self::default();

        Self {
            caret: palette.foreground,
            visual_selection: defaults.visual_selection,
            commented_bar: palette.tint(palette.yellow, 0.75),
            active_comment_bar: palette.accent,
            active_comment_tint: palette.tint(palette.accent, 0.09),
            commented_span_tint: palette.tint(palette.yellow, 0.22),
            find_match: defaults.find_match,
            current_find_match: defaults.current_find_match,
            commented_bar_width: defaults.commented_bar_width,
            active_comment_bar_width: defaults.active_comment_bar_width,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let palette = Palette::default();

        Self {
            caret: palette.foreground,
            visual_selection: Color::from_rgba(0.25, 0.5, 1.0, 0.4),
            commented_bar: palette.tint(palette.yellow, 0.75),
            active_comment_bar: palette.accent,
            active_comment_tint: palette.tint(palette.accent, 0.09),
            commented_span_tint: palette.tint(palette.yellow, 0.22),
            find_match: Color::from_rgba(0.95, 0.75, 0.25, 0.4),
            current_find_match: Color::from_rgba(0.98, 0.62, 0.15, 0.75),
            commented_bar_width: COMMENTED_BAR_WIDTH,
            active_comment_bar_width: ACTIVE_COMMENT_BAR_WIDTH,
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

/// Appends whole-element, gutter, and selected-span primitives for comments.
pub(crate) fn append_comments(
    output: &mut TextDecorations,
    comments: &comments::State,
    element: usize,
    element_len: usize,
    config: &Config,
) {
    let mark = comments.mark_for(element, element_len);

    match mark {
        comments::Mark::Active => {
            output.whole_element_background = Some(WholeElementBackground {
                color: config.active_comment_tint,
            });
            output.gutters.push(GutterDecoration {
                width: config.active_comment_bar_width,
                color: config.active_comment_bar,
            });
        }
        comments::Mark::Commented => output.gutters.push(GutterDecoration {
            width: config.commented_bar_width,
            color: config.commented_bar,
        }),
        comments::Mark::None => {}
    }

    if let Some(range) = comments.anchor_selection_for(element, element_len) {
        output.ranged_backgrounds.push(RangedBackground {
            range,
            color: config.commented_span_tint,
        });
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
        append_caret, append_comments, append_find, append_visual, Config, Pipeline,
        ACTIVE_COMMENT_BAR_WIDTH,
    };
    use crate::comments;
    use crate::interactive_text::{GutterDecoration, RangedBackground, WholeElementBackground};
    use crate::preview::{CaretPosition, State};
    use crate::theme::Palette;
    use iced::widget::text_editor::{Action, Edit};
    use iced::widget::Id;
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

        let config = Config::from_palette(&palette);

        assert_eq!(config.caret, palette.foreground);
        assert_eq!(config.active_comment_bar, palette.accent);
        assert_eq!(
            config.active_comment_tint,
            palette.tint(palette.accent, 0.09)
        );
        assert_eq!(config.commented_bar, palette.tint(palette.yellow, 0.75));
        assert_eq!(
            config.commented_span_tint,
            palette.tint(palette.yellow, 0.22)
        );
        assert_eq!(config.visual_selection, defaults.visual_selection);
        assert_eq!(config.find_match, defaults.find_match);
        assert_eq!(config.current_find_match, defaults.current_find_match);
    }

    #[test]
    fn overlapping_comment_find_and_visual_primitives_follow_pipeline_order() {
        let preview = State::new("alpha alpha");
        let preview_element = &preview.elements()[0];
        let comments = comments_on(1..10);
        let palette = Palette::default();
        let config = Config::from_palette(&palette);
        let current_match = (0, 6..11);

        let decorations = Pipeline::new()
            .append(|output| append_comments(output, &comments, 0, preview_element.len(), &config))
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
        assert_eq!(
            decorations.whole_element_background,
            Some(WholeElementBackground {
                color: config.active_comment_tint,
            })
        );
        assert_eq!(
            decorations.gutters,
            vec![GutterDecoration {
                width: ACTIVE_COMMENT_BAR_WIDTH,
                color: config.active_comment_bar,
            }]
        );
        assert_eq!(
            decorations.caret.as_ref().map(|caret| caret.column),
            Some(7)
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
