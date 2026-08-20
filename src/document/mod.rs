pub(crate) mod io;
pub(crate) mod watch;

use iced::widget::text_editor;
use iced::{Subscription, Task};
use std::path::{Path, PathBuf};

/// What the unsaved-changes dialog guards: opening another file, or
/// closing the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsavedAction {
    OpenFile,
    CloseWindow(iced::window::Id),
}

/// Why the app needs to refresh projections derived from the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceReplacement {
    Loaded,
    External,
}

/// Document-local widget, lifecycle, asynchronous I/O, and watcher messages.
#[derive(Debug, Clone)]
pub(crate) enum Message {
    Edit(text_editor::Action),
    OpenRequested,
    OpenLoaded(Option<(PathBuf, String)>),
    SaveRequested,
    SavePathChosen(Option<PathBuf>),
    /// The save result carries the exact snapshot written to disk. It may be
    /// older than the live text by the time the operation completes.
    Saved(Result<PathBuf, String>, String),
    ExternalChange,
    CloseRequested(iced::window::Id),
    UnsavedCardPressed,
    UnsavedCancel,
    UnsavedSave,
    UnsavedDiscard,
}

/// Semantic consequences that remain owned by the composing application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    SourceReplaced { reason: SourceReplacement },
    CloseWindow(iced::window::Id),
    UnsavedVisibilityChanged(bool),
}

pub(crate) struct Update {
    pub(crate) task: Task<Message>,
    pub(crate) event: Option<Event>,
}

impl Update {
    fn none() -> Self {
        Self {
            task: Task::none(),
            event: None,
        }
    }

    fn task(task: Task<Message>) -> Self {
        Self { task, event: None }
    }

    fn event(event: Event) -> Self {
        Self {
            task: Task::none(),
            event: Some(event),
        }
    }

    fn task_and_event(task: Task<Message>, event: Event) -> Self {
        Self {
            task,
            event: Some(event),
        }
    }
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

    pub(crate) fn move_to(&mut self, cursor: text_editor::Cursor) {
        self.content.move_to(cursor);
    }

    /// Replaces the document with a newly loaded file and makes that text
    /// the saved baseline.
    fn load(&mut self, path: PathBuf, contents: &str) {
        self.path = Some(path);
        self.content = text_editor::Content::with_text(contents);
        self.saved_contents = contents.to_owned();
    }

    /// Records the exact snapshot completed by a save. The live source may
    /// already contain newer edits, which must remain modified.
    fn mark_saved(&mut self, snapshot: String) {
        self.saved_contents = snapshot;
    }

    /// Replaces source text from disk while preserving its cursor and
    /// selection at the nearest valid positions. Returns whether the live
    /// source changed.
    fn replace_external(&mut self, contents: &str) -> bool {
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
}

/// Applies one document message and returns only document-local work plus a
/// semantic event for consequences owned by the app.
pub(crate) fn update(state: &mut State, message: Message) -> Update {
    match message {
        Message::Edit(action) => {
            edit(state, action);
            Update::none()
        }
        Message::OpenRequested => {
            if state.is_modified() {
                show_unsaved(state, UnsavedAction::OpenFile)
            } else {
                Update::task(Task::perform(io::open(), Message::OpenLoaded))
            }
        }
        Message::OpenLoaded(Some((path, contents))) => {
            state.load(path, &contents);
            Update::event(Event::SourceReplaced {
                reason: SourceReplacement::Loaded,
            })
        }
        Message::OpenLoaded(None) => Update::none(),
        Message::SaveRequested => Update::task(start_save(state)),
        Message::SavePathChosen(Some(path)) => {
            state.path = Some(path.clone());
            let snapshot = state.text();
            Update::task(Task::perform(
                io::save(path, snapshot.clone()),
                move |result| Message::Saved(result, snapshot),
            ))
        }
        Message::SavePathChosen(None) => {
            state.pending_unsaved = None;
            Update::event(Event::UnsavedVisibilityChanged(false))
        }
        Message::Saved(Ok(_path), snapshot) => {
            // The completed operation baselines what it actually wrote, not
            // whatever happens to be in the editor now.
            state.mark_saved(snapshot);

            if let Some(action) = state.pending_unsaved.take() {
                if state.is_modified() {
                    return show_unsaved(state, action);
                }

                return run_unsaved_action(action);
            }

            Update::none()
        }
        Message::Saved(Err(error), _snapshot) => {
            eprintln!("agmawrite: {error}");
            state.pending_unsaved = None;
            Update::event(Event::UnsavedVisibilityChanged(false))
        }
        // The card swallows clicks so they do not reach the editing surface.
        Message::UnsavedCardPressed => Update::none(),
        Message::UnsavedCancel => {
            state.pending_unsaved = None;
            Update::event(Event::UnsavedVisibilityChanged(false))
        }
        Message::UnsavedSave => {
            Update::task_and_event(start_save(state), Event::UnsavedVisibilityChanged(false))
        }
        Message::UnsavedDiscard => match state.pending_unsaved.take() {
            Some(action) => run_unsaved_action(action),
            None => Update::none(),
        },
        Message::CloseRequested(id) => {
            if state.is_modified() {
                show_unsaved(state, UnsavedAction::CloseWindow(id))
            } else {
                Update::event(Event::CloseWindow(id))
            }
        }
        Message::ExternalChange => {
            let Some(path) = state.path.as_deref() else {
                return Update::none();
            };
            let Ok(contents) = io::read(path) else {
                return Update::none();
            };

            if state.replace_external(&contents) {
                Update::event(Event::SourceReplaced {
                    reason: SourceReplacement::External,
                })
            } else {
                Update::none()
            }
        }
    }
}

/// Maps the document watcher's local event into the document reducer's local
/// message. The app maps this subscription once at the feature boundary.
pub(crate) fn subscription(path: &Path) -> Subscription<Message> {
    watch::subscription(path).map(|_event| Message::ExternalChange)
}

fn show_unsaved(state: &mut State, action: UnsavedAction) -> Update {
    state.pending_unsaved = Some(action);
    Update::event(Event::UnsavedVisibilityChanged(true))
}

fn start_save(state: &State) -> Task<Message> {
    match state.path() {
        Some(path) => {
            let snapshot = state.text();
            Task::perform(
                io::save(path.to_path_buf(), snapshot.clone()),
                move |result| Message::Saved(result, snapshot),
            )
        }
        None => Task::perform(io::pick_save_path(), Message::SavePathChosen),
    }
}

fn run_unsaved_action(action: UnsavedAction) -> Update {
    match action {
        UnsavedAction::OpenFile => Update::task(Task::perform(io::open(), Message::OpenLoaded)),
        UnsavedAction::CloseWindow(id) => Update::event(Event::CloseWindow(id)),
    }
}

/// Applies a source edit. Enter continues or terminates Markdown lists using
/// the shared pure continuation policy.
fn edit(state: &mut State, action: text_editor::Action) {
    use text_editor::{Action, Edit};

    if !matches!(action, Action::Edit(Edit::Enter)) {
        state.content.perform(action);
        return;
    }

    let cursor = state.content.cursor().position;
    let before = state.content.line(cursor.line).map(|line| {
        line.text
            .char_indices()
            .nth(cursor.column)
            .map_or(line.text.as_ref(), |(index, _)| &line.text[..index])
            .to_owned()
    });

    match before.as_deref().map(crate::editing::continuation) {
        Some(crate::editing::Continuation::Continue(prefix)) => {
            state.content.perform(action);
            state
                .content
                .perform(Action::Edit(Edit::Paste(std::sync::Arc::new(prefix))));
        }
        Some(crate::editing::Continuation::Outdent(count)) => {
            for _ in 0..count {
                state.content.perform(Action::Edit(Edit::Backspace));
            }
            state.content.perform(action);
        }
        _ => state.content.perform(action),
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
    use super::{update, Event, Message, SourceReplacement, State, UnsavedAction};
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

        update(&mut document, Message::Edit(insert('x')));
        assert!(document.is_modified());

        let snapshot = document.text();
        update(&mut document, Message::Saved(Ok("saved".into()), snapshot));
        assert!(!document.is_modified());

        let result = update(
            &mut document,
            Message::OpenLoaded(Some(("/tmp/a.md".into(), "other".into()))),
        );
        assert_eq!(
            result.event,
            Some(Event::SourceReplaced {
                reason: SourceReplacement::Loaded
            })
        );
        assert!(!document.is_modified());
    }

    #[test]
    fn save_completion_baselines_the_exact_written_snapshot() {
        let mut document = State::new("draft", None);
        document.move_to(Cursor {
            position: Position { line: 0, column: 5 },
            selection: None,
        });
        update(&mut document, Message::Edit(insert('!')));
        let snapshot = document.text();
        update(&mut document, Message::Edit(insert('?')));

        update(&mut document, Message::Saved(Ok("saved".into()), snapshot));

        assert_eq!(document.text(), "draft!?");
        assert!(document.is_modified());
    }

    #[test]
    fn enter_continues_and_terminates_lists() {
        let mut document = State::new("- item", None);
        document.move_to(Cursor {
            position: Position { line: 0, column: 6 },
            selection: None,
        });

        update(&mut document, Message::Edit(Action::Edit(Edit::Enter)));
        assert_eq!(document.text(), "- item\n- ");
        assert_eq!(
            document.content().cursor().position,
            Position { line: 1, column: 2 }
        );

        update(&mut document, Message::Edit(Action::Edit(Edit::Enter)));
        assert_eq!(document.text(), "- item\n\n");
        assert_eq!(
            document.content().cursor().position,
            Position { line: 2, column: 0 }
        );
    }

    #[test]
    fn guarded_open_cancels_discards_and_waits_for_a_clean_save() {
        let mut document = State::new("draft", Some("/tmp/document-open.md".into()));
        update(&mut document, Message::Edit(insert('!')));

        let result = update(&mut document, Message::OpenRequested);
        assert_eq!(result.event, Some(Event::UnsavedVisibilityChanged(true)));
        assert_eq!(document.pending_action(), Some(UnsavedAction::OpenFile));

        let result = update(&mut document, Message::UnsavedCancel);
        assert_eq!(result.event, Some(Event::UnsavedVisibilityChanged(false)));
        assert!(document.pending_action().is_none());
        assert!(document.is_modified());

        update(&mut document, Message::OpenRequested);
        update(&mut document, Message::UnsavedDiscard);
        assert!(document.pending_action().is_none());
        assert!(document.is_modified());

        update(&mut document, Message::OpenRequested);
        update(&mut document, Message::UnsavedSave);
        let snapshot = document.text();
        let result = update(&mut document, Message::Saved(Ok("saved".into()), snapshot));
        assert!(result.event.is_none());
        assert!(document.pending_action().is_none());
        assert!(!document.is_modified());
    }

    #[test]
    fn guarded_open_reopens_when_edits_arrive_during_save() {
        let mut document = State::new("draft", Some("/tmp/document-stale.md".into()));
        document.move_to(Cursor {
            position: Position { line: 0, column: 5 },
            selection: None,
        });
        update(&mut document, Message::Edit(insert('!')));
        update(&mut document, Message::OpenRequested);
        update(&mut document, Message::UnsavedSave);
        let snapshot = document.text();

        update(&mut document, Message::Edit(insert('?')));
        let result = update(&mut document, Message::Saved(Ok("saved".into()), snapshot));

        assert_eq!(result.event, Some(Event::UnsavedVisibilityChanged(true)));
        assert_eq!(document.pending_action(), Some(UnsavedAction::OpenFile));
        assert_eq!(document.text(), "draft!?");
        assert!(document.is_modified());
    }

    #[test]
    fn cancelling_save_path_drops_the_guarded_action() {
        let mut document = State::new("draft", None);
        update(&mut document, Message::Edit(insert('!')));
        update(&mut document, Message::OpenRequested);
        update(&mut document, Message::UnsavedSave);

        let result = update(&mut document, Message::SavePathChosen(None));

        assert_eq!(result.event, Some(Event::UnsavedVisibilityChanged(false)));
        assert!(document.pending_action().is_none());
        assert!(document.is_modified());
    }

    #[test]
    fn guarded_close_reopens_after_a_stale_save_and_closes_when_clean() {
        let id = iced::window::Id::unique();
        let mut document = State::new("draft", Some("/tmp/document-close.md".into()));
        document.move_to(Cursor {
            position: Position { line: 0, column: 5 },
            selection: None,
        });
        update(&mut document, Message::Edit(insert('!')));

        let result = update(&mut document, Message::CloseRequested(id));
        assert_eq!(result.event, Some(Event::UnsavedVisibilityChanged(true)));
        assert_eq!(
            document.pending_action(),
            Some(UnsavedAction::CloseWindow(id))
        );

        update(&mut document, Message::UnsavedSave);
        let snapshot = document.text();
        update(&mut document, Message::Edit(insert('?')));
        let stale = update(&mut document, Message::Saved(Ok("saved".into()), snapshot));
        assert_eq!(stale.event, Some(Event::UnsavedVisibilityChanged(true)));
        assert_eq!(
            document.pending_action(),
            Some(UnsavedAction::CloseWindow(id))
        );

        update(&mut document, Message::UnsavedSave);
        let snapshot = document.text();
        let clean = update(&mut document, Message::Saved(Ok("saved".into()), snapshot));
        assert_eq!(clean.event, Some(Event::CloseWindow(id)));
        assert!(document.pending_action().is_none());
        assert!(!document.is_modified());
    }
}
