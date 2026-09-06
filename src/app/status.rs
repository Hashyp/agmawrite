//! Projects document and preview state into the presentation-only status line.

use unicode_segmentation::UnicodeSegmentation;

use super::App;
use crate::preview::{CaretPosition, PreviewElement};
use crate::ui::status_bar::{self, Model};

pub(super) fn model(app: &App, source: &str) -> Model {
    let (line, column) = location(source, app.preview.elements(), app.preview.caret());
    Model {
        interaction: app.interaction.view(),
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
    fn ruler_matches_lualine_progress_conventions() {
        assert_eq!(progress(1, 1), "Top");
        assert_eq!(progress(1, 10), "Top");
        assert_eq!(progress(5, 10), "50%");
        assert_eq!(progress(10, 10), "Bot");
    }
}
