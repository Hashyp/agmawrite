//! Projects document and preview state into the presentation-only status line.

use unicode_segmentation::UnicodeSegmentation;

use super::App;
use crate::input::Surface;
use crate::preview::{CaretPosition, PreviewElement};
use crate::ui::status_bar::{self, Model};
use iced::widget::text_editor::Position;

pub(super) fn model(app: &App, source: &str) -> Model {
    let interaction = app.interaction.view();
    let (line, column) = match interaction.surface() {
        Surface::Preview => location(source, app.preview.elements(), app.preview.caret()),
        Surface::Write => write_location(app.document.content().cursor().position),
    };
    Model {
        interaction,
        filename: status_bar::filename(app.document.path(), app.document.is_modified()),
        full_path: app.document.path().map_or_else(
            || "Unsaved document".to_owned(),
            |path| path.display().to_string(),
        ),
        location: format!("{line}:{column}"),
        progress: progress(line, source.lines().count().max(1)),
        metadata: app.status_metadata.clone(),
        palette: app.palette,
        comment_count: app.comments.len(),
        report: app.report.clone(),
    }
}

/// Neovim's own yank report wording for a charwise yank: the grapheme count
/// of what was yanked — counting like the editor does, one character per
/// grapheme cluster — with Vim's singular form for one.
pub(super) fn yank_report(text: &str) -> String {
    let characters = text.graphemes(true).count();
    if characters == 1 {
        "1 character yanked".to_owned()
    } else {
        format!("{characters} characters yanked")
    }
}

/// Source line plus approximate rendered grapheme column: inline Markdown
/// delimiters are stripped in preview, so this is not an exact source offset.
fn location(source: &str, elements: &[PreviewElement], caret: CaretPosition) -> (usize, usize) {
    let Some(element) = elements.get(caret.element) else {
        return (1, 1);
    };
    let start = crate::editing::position_at(source, element.source().start);
    let prefix: String = element.text().graphemes(true).take(caret.column).collect();
    let extra_lines = prefix.matches('\n').count();
    let line = start.line + extra_lines;
    let rendered_column = prefix
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .graphemes(true)
        .count();
    let source_width = source
        .lines()
        .nth(line)
        .unwrap_or_default()
        .graphemes(true)
        .count();
    (
        (line + 1).min(source.lines().count().max(1)),
        rendered_column.min(source_width) + 1,
    )
}

/// The write-mode ruler: the source editor's own caret, one-based like vim
/// and exact — the editor owns both the line and the column.
fn write_location(position: Position) -> (usize, usize) {
    (position.line + 1, position.column + 1)
}

// Exactly Lualine's progress component: caret line / total source lines,
// not viewport scroll percentage (mouse scrolling leaves the ruler alone).
fn progress(line: usize, total: usize) -> String {
    if line == 1 {
        "Top".into()
    } else if line >= total {
        "Bot".into()
    } else {
        format!("{}%", line * 100 / total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::State;
    use crate::theme::Palette;

    #[test]
    fn location_maps_block_start_with_approximate_grapheme_column() {
        let source = "# Title\n\né👩‍💻\nlast\n";
        let preview = State::new(source);
        let element = &preview.elements()[1];
        let column = element
            .text()
            .graphemes(true)
            .position(|g| g == "l")
            .unwrap();
        assert_eq!(
            location(
                source,
                preview.elements(),
                CaretPosition { element: 1, column }
            ),
            (3, 3)
        );
    }

    #[test]
    fn empty_document_has_a_valid_ruler() {
        assert_eq!(
            location(
                "",
                &[],
                CaretPosition {
                    element: 0,
                    column: 0
                }
            ),
            (1, 1)
        );
    }

    #[test]
    fn write_ruler_reports_the_source_editor_caret() {
        use iced::widget::text_editor::Cursor;

        let contents = "one\ntwo\nthree\n";
        let mut app = App {
            document: crate::document::State::new(contents, None),
            preview: State::new(contents),
            interaction: crate::input::InteractionState::editable(),
            comments: crate::comments::State::new(),
            find: crate::find::State::new(),
            help: crate::help::Help::new(),
            palette: Palette::default(),
            status_metadata: Default::default(),
            report: None,
        };
        app.document.move_to(Cursor {
            position: Position { line: 1, column: 2 },
            selection: None,
        });

        let ruler = model(&app, contents);

        assert_eq!(ruler.location, "2:3");
        assert_eq!(ruler.progress, "66%");
    }

    #[test]
    fn ruler_matches_lualine_progress_conventions() {
        assert_eq!(progress(1, 1), "Top");
        assert_eq!(progress(1, 10), "Top");
        assert_eq!(progress(5, 10), "50%");
        assert_eq!(progress(10, 10), "Bot");
    }

    #[test]
    fn yank_report_counts_graphemes_with_vims_singular() {
        // `v` + `l` + `l` + `y` over two astral characters.
        assert_eq!(yank_report("👩‍💻"), "1 character yanked");
        assert_eq!(yank_report("ab👩‍💻"), "3 characters yanked");
        assert_eq!(yank_report(""), "0 characters yanked");
    }

    #[test]
    fn status_model_carries_the_yank_report() {
        let mut app = App {
            document: crate::document::State::new("body", None),
            preview: State::new("body"),
            interaction: crate::input::InteractionState::preview_only(),
            comments: crate::comments::State::new(),
            find: crate::find::State::new(),
            help: crate::help::Help::new(),
            palette: Palette::default(),
            status_metadata: Default::default(),
            report: Some("3 characters yanked".to_owned()),
        };

        assert_eq!(
            model(&app, "body").report.as_deref(),
            Some("3 characters yanked")
        );

        app.report = None;
        assert_eq!(model(&app, "body").report, None);
    }
}
