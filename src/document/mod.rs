use iced::widget::text_editor;
use std::path::{Path, PathBuf};

/// What the unsaved-changes dialog guards: opening another file, or
/// closing the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsavedAction {
    OpenFile,
    CloseWindow(iced::window::Id),
}

/// The source document and the state needed to preserve its persistence
/// invariants.
pub(crate) struct State {
    content: text_editor::Content,
    path: Option<PathBuf>,
    saved_contents: String,
    pending_unsaved: Option<UnsavedAction>,
}

impl State {
    pub(crate) fn new(contents: &str, path: Option<PathBuf>) -> Self {
        Self {
            content: text_editor::Content::with_text(contents),
            path,
            saved_contents: contents.to_owned(),
            pending_unsaved: None,
        }
    }

    /// The editor content used to render the source surface.
    pub(crate) fn content(&self) -> &text_editor::Content {
        &self.content
    }

    /// The live source text.
    pub(crate) fn text(&self) -> String {
        self.content.text()
    }

    /// The file the document came from and saves back to, once known.
    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether the live source differs from the last loaded or saved
    /// snapshot.
    pub(crate) fn is_modified(&self) -> bool {
        self.content.text() != self.saved_contents
    }

    /// The action currently guarded by the unsaved-changes dialog.
    pub(crate) fn pending_action(&self) -> Option<UnsavedAction> {
        self.pending_unsaved
    }

    pub(crate) fn perform(&mut self, action: text_editor::Action) {
        self.content.perform(action);
    }

    pub(crate) fn move_to(&mut self, cursor: text_editor::Cursor) {
        self.content.move_to(cursor);
    }

    pub(crate) fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    /// Replaces the document with a newly loaded file and makes that text
    /// the saved baseline.
    pub(crate) fn load(&mut self, path: PathBuf, contents: &str) {
        self.path = Some(path);
        self.content = text_editor::Content::with_text(contents);
        self.saved_contents = contents.to_owned();
    }

    /// Records the exact snapshot completed by a save. The live source may
    /// already contain newer edits, which must remain modified.
    pub(crate) fn mark_saved(&mut self, snapshot: String) {
        self.saved_contents = snapshot;
    }

    /// Replaces source text from disk while preserving its cursor and
    /// selection at the nearest valid positions. Returns whether the live
    /// source changed.
    pub(crate) fn replace_external(&mut self, contents: &str) -> bool {
        if contents == self.content.text() {
            return false;
        }

        let cursor = self.content.cursor();
        let mut content = text_editor::Content::with_text(contents);
        content.move_to(text_editor::Cursor {
            position: clamp_position(&content, cursor.position),
            selection: cursor
                .selection
                .map(|position| clamp_position(&content, position)),
        });
        self.content = content;
        self.saved_contents = contents.to_owned();

        true
    }

    pub(crate) fn set_pending_action(&mut self, action: UnsavedAction) {
        self.pending_unsaved = Some(action);
    }

    pub(crate) fn take_pending_action(&mut self) -> Option<UnsavedAction> {
        self.pending_unsaved.take()
    }

    pub(crate) fn clear_pending_action(&mut self) {
        self.pending_unsaved = None;
    }
}

fn clamp_position(
    content: &text_editor::Content,
    position: text_editor::Position,
) -> text_editor::Position {
    let line = position.line.min(content.line_count().saturating_sub(1));
    let text = content.line(line).map(|line| line.text).unwrap_or_default();
    let mut column = position.column.min(text.len());

    // iced's editor columns are UTF-8 byte offsets. An external edit can
    // put the old offset in the middle of a new multi-byte character.
    while !text.is_char_boundary(column) {
        column -= 1;
    }

    text_editor::Position { line, column }
}

#[cfg(test)]
mod tests {
    use super::State;
    use iced::widget::text_editor::{Action, Cursor, Edit, Position};

    fn insert(character: char) -> Action {
        Action::Edit(Edit::Insert(character))
    }

    #[test]
    fn external_replacement_preserves_and_clamps_the_cursor() {
        let mut document = State::new("first line\nsecond line\nthird line", None);
        document.move_to(Cursor {
            position: Position { line: 1, column: 6 },
            selection: Some(Position { line: 2, column: 5 }),
        });

        assert!(document.replace_external("changed\nstill here\nlast"));
        let cursor = document.content().cursor();
        assert_eq!(cursor.position, Position { line: 1, column: 6 });
        assert_eq!(cursor.selection, Some(Position { line: 2, column: 4 }));

        assert!(document.replace_external("short"));
        let cursor = document.content().cursor();
        assert_eq!(cursor.position, Position { line: 0, column: 5 });
        assert_eq!(cursor.selection, Some(Position { line: 0, column: 4 }));
        assert!(!document.is_modified());
    }

    #[test]
    fn modified_tracking_follows_load_save_and_edits() {
        let mut document = State::new("body", None);
        assert!(!document.is_modified());

        document.perform(insert('x'));
        assert!(document.is_modified());

        document.mark_saved(document.text());
        assert!(!document.is_modified());

        document.load("/tmp/a.md".into(), "other");
        assert!(!document.is_modified());
    }

    #[test]
    fn plain_save_baselines_the_written_snapshot() {
        let mut document = State::new("draft", None);
        document.perform(insert('!'));

        document.mark_saved("draft".to_owned());
        assert!(document.is_modified());

        document.mark_saved(document.text());
        assert!(!document.is_modified());
    }

    #[test]
    fn edits_during_a_save_remain_modified() {
        let mut document = State::new("draft", None);
        document.move_to(Cursor {
            position: Position { line: 0, column: 5 },
            selection: None,
        });
        document.perform(insert('!'));
        let snapshot = document.text();

        document.perform(insert('?'));
        document.mark_saved(snapshot);

        assert_eq!(document.text(), "draft!?");
        assert!(document.is_modified());
    }
}
