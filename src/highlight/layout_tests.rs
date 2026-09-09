//! Regression tests at the renderer editor boundary used by the source widget.
use super::{format, MarkdownMarkers};
use crate::theme::Palette;
use iced::advanced::graphics::text::Editor;
use iced::advanced::text::editor::{Action, Cursor, Editor as _, Position, Selection};
use iced::advanced::text::{Highlighter, LineHeight, Wrapping};
use iced::{Font, Pixels, Point, Size};

fn layout(
    editor: &mut Editor,
    highlighter: &mut impl Highlighter<Highlight = super::Highlight>,
    bounds: Size,
    size: f32,
) {
    editor.update(
        bounds,
        Font::MONOSPACE,
        Pixels(size),
        LineHeight::Relative(1.8),
        Wrapping::Word,
        highlighter,
    );
    let theme = Palette::default().runtime_theme();
    editor.highlight(Font::MONOSPACE, highlighter, |highlight| {
        format(highlight, &theme)
    });
}

/// An existing highlighter that does not opt in to line-height overrides.
struct Uniform(usize);

impl Highlighter for Uniform {
    type Settings = ();
    type Highlight = super::Highlight;
    type Iterator<'a> = std::iter::Empty<(std::ops::Range<usize>, super::Highlight)>;

    fn new(_: &()) -> Self {
        Self(0)
    }
    fn update(&mut self, _: &()) {
        self.0 = 0;
    }
    fn change_line(&mut self, line: usize) {
        self.0 = self.0.min(line);
    }
    fn current_line(&self) -> usize {
        self.0
    }
    fn highlight_line(&mut self, _: &str) -> Self::Iterator<'_> {
        self.0 += 1;
        std::iter::empty()
    }
}

#[test]
fn other_highlighters_keep_uniform_line_heights_by_default() {
    let mut editor = Editor::with_text("before\n\nafter");
    let mut highlighter = Uniform::new(&());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 20.0);

    assert_eq!(highlighter.current_line(), 3);
    assert_eq!(editor.min_bounds().height, 108.0);
}

#[test]
fn moving_to_document_end_keeps_the_caret_visible_after_first_reflow() {
    use iced::advanced::text::editor::Motion;
    let mut editor = Editor::with_text(&format!("{}last", "paragraph\n\n".repeat(60)));
    let mut highlighter = MarkdownMarkers::new(&String::new());
    let bounds = Size::new(600.0, 180.0);
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    editor.perform(Action::Move(Motion::DocumentEnd));
    layout(&mut editor, &mut highlighter, bounds, 20.0);

    let Selection::Caret(point) = editor.selection() else {
        panic!("expected caret")
    };
    assert_eq!(point.y, 144.0);
    assert_eq!(highlighter.current_line(), editor.line_count());
}

#[test]
fn a_fence_run_with_trailing_text_does_not_expand_code_blanks() {
    let mut editor = Editor::with_text("```rust\n```not a closing fence\n\ncode\n```\n\nafter");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 20.0);

    assert_eq!(
        editor
            .buffer()
            .layout_runs()
            .map(|run| run.line_height)
            .collect::<Vec<_>>(),
        vec![36.0, 36.0, 36.0, 36.0, 36.0, 54.0, 36.0]
    );
}

#[test]
fn editing_an_unvisited_line_replays_fence_context_before_spacing_it() {
    use iced::advanced::text::editor::Edit;
    let mut editor =
        Editor::with_text("intro\n\nintro\n\n```rust\ncode\ncode\ncode\ncode\ncode\n\ncode\n\n```");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    let bounds = Size::new(600.0, 126.0);
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    editor.move_to(Cursor {
        position: Position {
            line: 10,
            column: 0,
        },
        selection: None,
    });
    editor.perform(Action::Edit(Edit::Insert(' ')));
    layout(&mut editor, &mut highlighter, bounds, 20.0);

    let run = editor
        .buffer()
        .layout_runs()
        .find(|run| run.line_i == 10)
        .unwrap();
    assert_eq!(run.line_height, 36.0);
}

#[test]
fn clicking_after_scrolling_uses_the_shifted_paragraph_position() {
    let mut editor = Editor::with_text("one\n\ntwo\n\nthree\n\nfour\n\nfive");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    let bounds = Size::new(600.0, 126.0);
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    editor.perform(Action::Scroll { lines: 1 });
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    editor.perform(Action::Click(Point::new(1.0, 60.0)));

    assert_eq!(editor.cursor().position, Position { line: 2, column: 0 });
    let Selection::Caret(point) = editor.selection() else {
        panic!("expected caret")
    };
    assert_eq!(point.y, 54.0);
}

#[test]
fn rewrapping_keeps_the_caret_on_the_paragraph_after_the_gap() {
    let mut editor = Editor::with_text("alpha beta gamma delta\n\nlast");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 600.0), 20.0);
    editor.move_to(Cursor {
        position: Position { line: 2, column: 0 },
        selection: None,
    });
    layout(&mut editor, &mut highlighter, Size::new(100.0, 600.0), 20.0);

    let runs: Vec<_> = editor.buffer().layout_runs().collect();
    assert!(runs.iter().filter(|run| run.line_i == 0).count() > 1);
    let rendered_top = runs.iter().find(|run| run.line_i == 2).unwrap().line_top;
    let Selection::Caret(point) = editor.selection() else {
        panic!("expected caret")
    };
    assert_eq!(point.y, rendered_top);
}

#[test]
fn adding_and_removing_text_on_a_separator_updates_its_height() {
    use iced::advanced::text::editor::Edit;
    let mut editor = Editor::with_text("before\n\nafter");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    let bounds = Size::new(600.0, 400.0);
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    editor.move_to(Cursor {
        position: Position { line: 1, column: 0 },
        selection: None,
    });
    editor.perform(Action::Edit(Edit::Insert('x')));
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    assert_eq!(editor.min_bounds().height, 108.0);

    editor.perform(Action::Edit(Edit::Backspace));
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    assert_eq!(editor.min_bounds().height, 126.0);
    assert_eq!(editor.line(1).unwrap().text, "");
}

#[test]
fn highlighting_invalidates_a_caret_cached_before_reflow() {
    let mut editor = Editor::with_text("title\n\nparagraph one\n\nparagraph two");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    editor.update(
        Size::new(600.0, 400.0),
        Font::MONOSPACE,
        Pixels(20.0),
        LineHeight::Relative(1.8),
        Wrapping::Word,
        &mut highlighter,
    );
    editor.move_to(Cursor {
        position: Position { line: 4, column: 0 },
        selection: None,
    });
    let _ = editor.selection();
    let theme = Palette::default().runtime_theme();
    editor.highlight(Font::MONOSPACE, &mut highlighter, |highlight| {
        format(highlight, &theme)
    });

    let Selection::Caret(point) = editor.selection() else {
        panic!("expected caret")
    };
    assert_eq!(point.y, 180.0);
}

#[test]
fn changing_font_while_scrolled_away_does_not_draw_an_offscreen_caret() {
    let mut editor = Editor::with_text(&"paragraph\n\n".repeat(40));
    let mut highlighter = MarkdownMarkers::new(&String::new());
    let bounds = Size::new(600.0, 180.0);
    layout(&mut editor, &mut highlighter, bounds, 20.0);
    editor.perform(Action::Scroll { lines: 30 });
    editor.update(
        bounds,
        Font::default(),
        Pixels(20.0),
        LineHeight::Relative(1.8),
        Wrapping::Word,
        &mut highlighter,
    );

    let Selection::Caret(point) = editor.selection() else {
        panic!("expected caret")
    };
    assert!(
        point.y <= -36.0,
        "offscreen caret should be clipped: {point:?}"
    );
}

#[test]
fn highlighting_stops_at_the_actual_viewport_even_with_cached_lines_below_it() {
    let mut editor = Editor::with_text("title\n\nparagraph one\n\nparagraph two");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 20.0);
    highlighter.change_line(0);
    layout(&mut editor, &mut highlighter, Size::new(600.0, 126.0), 20.0);

    assert_eq!(highlighter.current_line(), 3);
}

#[test]
fn font_size_changes_recompute_paragraph_gaps() {
    let mut editor = Editor::with_text("before\n\nafter");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 20.0);
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 30.0);

    assert_eq!(
        editor
            .buffer()
            .layout_runs()
            .map(|run| run.line_height)
            .collect::<Vec<_>>(),
        vec![54.0, 81.0, 54.0]
    );
}

#[test]
fn spaces_and_tabs_are_blank_paragraph_separators_too() {
    let mut editor = Editor::with_text("before\n \t\nafter");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 20.0);

    assert_eq!(editor.min_bounds().height, 126.0);
}

#[test]
fn blank_lines_in_fenced_code_keep_normal_height() {
    let mut editor = Editor::with_text("before\n\n```rust\n\nlet x = 1;\n\n```\n\nafter");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 600.0), 20.0);

    assert_eq!(
        editor
            .buffer()
            .layout_runs()
            .map(|run| run.line_height)
            .collect::<Vec<_>>(),
        vec![36.0, 54.0, 36.0, 36.0, 36.0, 36.0, 36.0, 54.0, 36.0]
    );
}

#[test]
fn clicking_after_two_paragraph_gaps_places_the_caret_on_the_text() {
    let mut editor = Editor::with_text("title\n\nparagraph one\n\nparagraph two");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 20.0);
    editor.perform(Action::Click(Point::new(1.0, 190.0)));

    assert_eq!(editor.cursor().position, Position { line: 4, column: 0 });
    let Selection::Caret(point) = editor.selection() else {
        panic!("expected caret")
    };
    assert_eq!(point.y, 180.0);
}

#[test]
fn selecting_after_paragraph_gaps_highlights_the_rendered_text() {
    let mut editor = Editor::with_text("title\n\nparagraph one\n\nparagraph two");
    let mut highlighter = MarkdownMarkers::new(&String::new());
    layout(&mut editor, &mut highlighter, Size::new(600.0, 400.0), 20.0);
    editor.move_to(Cursor {
        position: Position { line: 4, column: 4 },
        selection: Some(Position { line: 2, column: 0 }),
    });

    let Selection::Range(regions) = editor.selection() else {
        panic!("expected selection")
    };
    assert_eq!(
        regions.iter().map(|r| (r.y, r.height)).collect::<Vec<_>>(),
        vec![(90.0, 36.0), (180.0, 36.0)]
    );
    assert_eq!(editor.copy().as_deref(), Some("paragraph one\n\npara"));
}
