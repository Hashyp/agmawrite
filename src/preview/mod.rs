//! Preview feature state, reducer, and parsed caret model.

mod decorations;
mod model;
mod scroll;
mod viewer;

use model::Caret;
pub(crate) use model::{
    element_selection, selection_text, CaretPosition, Claims, ElementMap, Jump, Motion, Page,
    Placement, PreviewElement, WordMotion,
};
pub(crate) use scroll::{place_caret_in_view, reveal_anchor, reveal_caret, scroll_by, scroll_page};
pub(crate) use viewer::{view, ViewContext};

use std::time::Duration;

use iced::widget::{markdown, text_editor};
use iced::{Subscription, Task};

/// How long the yanked span keeps flashing, like the default timeout of
/// Neovim's `vim.hl.on_yank`.
pub(crate) const YANK_FLASH: Duration = Duration::from_millis(300);

/// The yank flash's clearing ticker: one [`Message::ClearFlash`],
/// [`YANK_FLASH`] after the yank that armed it. The runtime's thread-pool
/// backend offers no timer, so the tick rides a plain thread like the
/// document watcher's.
pub(crate) fn flash_subscription() -> Subscription<Message> {
    Subscription::run_with((), |()| {
        iced::stream::channel(
            1,
            move |mut sender: iced::futures::channel::mpsc::Sender<Message>| async move {
                std::thread::spawn(move || {
                    std::thread::sleep(YANK_FLASH);
                    let _ = sender.try_send(Message::ClearFlash);
                });
                // The tick arrives on the timer thread; this runner only keeps
                // the stream alive until it lands.
                std::future::pending::<()>().await;
            },
        )
    })
}

/// Preview-local navigation and scrolling messages.
#[derive(Debug, Clone)]
pub(crate) enum Message {
    Toggle,
    LinkClicked(markdown::Uri),
    Move(Motion, usize),
    MoveWord(WordMotion, usize),
    Jump(Jump, usize),
    Cancel,
    ToggleVisual,
    /// Copies the visual selection, like vim's visual-mode `y`.
    Yank,
    /// Ends the yanked-span flash once its moment has passed.
    ClearFlash,
    ScrollBy(f32),
    ScrollPage(Page, usize),
    ScrollCaret(Placement),
}

/// Read-only application context needed for a preview transition.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Context {
    /// The input mode has already observed the message before the preview
    /// reducer runs, so this is the resulting visual-mode state.
    pub(crate) visual_active: bool,
}

/// Semantic work that remains at the application boundary.
#[derive(Debug, Clone)]
pub(crate) enum Event {
    ToggleRequested,
    OpenLink(markdown::Uri),
    /// The visual selection was yanked: its rendered text is the payload,
    /// ready for the system clipboard and a yank report.
    Yanked {
        text: String,
    },
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
}

/// The complete preview projection and navigation state.
pub(crate) struct State {
    markdown: markdown::Content,
    elements: ElementMap,
    caret: Caret,
    visual_anchor: Option<CaretPosition>,
    /// The span of the last yank, flashing over the preview for a moment
    /// like Neovim's `vim.hl.on_yank` highlight.
    yank_flash: Option<(CaretPosition, CaretPosition)>,
}

impl State {
    /// Initializes every preview projection from `source` with the caret at
    /// the beginning and no visual selection.
    pub(crate) fn new(source: &str) -> Self {
        Self {
            markdown: markdown::Content::parse(source),
            elements: ElementMap::parse(source),
            caret: Caret::new(),
            visual_anchor: None,
            yank_flash: None,
        }
    }

    /// Replaces derived source projections and resets the caret. Existing
    /// visual selection is retained to preserve external-change behavior;
    /// callers handling a document load explicitly clear it.
    pub(crate) fn replace_source(&mut self, source: &str) {
        self.markdown = markdown::Content::parse(source);
        self.elements = ElementMap::parse(source);
        self.caret = Caret::new();
    }

    /// Replaces source for a newly loaded document, resetting all preview
    /// navigation state bound to the old document.
    pub(crate) fn load_source(&mut self, source: &str) {
        self.replace_source(source);
        self.clear_visual_selection();
        self.yank_flash = None;
    }

    /// Refreshes source edits when entering preview and places the preview
    /// caret at the source editor cursor.
    pub(crate) fn refresh_from_source(&mut self, source: &str, content: &text_editor::Content) {
        self.markdown = markdown::Content::parse(source);
        self.elements = ElementMap::parse(source);
        self.caret
            .move_to_source_cursor(content, self.elements.elements());
        self.clear_visual_selection();
        self.yank_flash = None;
    }

    /// Places the caret for tests: production code moves the caret only
    /// through the preview reducer and its own projections.
    #[cfg(test)]
    pub(crate) fn place_caret(&mut self, position: CaretPosition) {
        self.caret.place(position);
    }

    /// Applies an exact preview match selected by the find feature.
    pub(crate) fn apply_find_selection(&mut self, element: usize, range: std::ops::Range<usize>) {
        self.caret.place(CaretPosition {
            element,
            column: range.start,
        });
    }

    /// Clears the fixed end of visual selection without changing the caret.
    pub(crate) fn clear_visual_selection(&mut self) {
        self.visual_anchor = None;
    }

    pub(crate) fn markdown(&self) -> &markdown::Content {
        &self.markdown
    }

    pub(crate) fn elements(&self) -> &[PreviewElement] {
        self.elements.elements()
    }

    pub(crate) fn claims(&self) -> Claims<'_> {
        self.elements.claims()
    }

    pub(crate) fn caret(&self) -> CaretPosition {
        self.caret.position()
    }

    pub(crate) fn visual_selection(&self) -> Option<(CaretPosition, CaretPosition)> {
        self.visual_anchor.map(|anchor| (anchor, self.caret()))
    }

    /// The flashing span of the last yank, exactly the selection it copied.
    pub(crate) fn yank_flash(&self) -> Option<(CaretPosition, CaretPosition)> {
        self.yank_flash
    }

    /// Whether the yank flash is showing, so the application knows to run
    /// its clearing ticker.
    pub(crate) fn flash_active(&self) -> bool {
        self.yank_flash.is_some()
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new("")
    }
}

/// Applies one preview transition. Navigation and visual state mutate here;
/// preview-local widget-tree tasks are constructed by [`scroll`].
pub(crate) fn update(state: &mut State, message: Message, context: Context) -> Update {
    match message {
        Message::Toggle => Update::event(Event::ToggleRequested),
        Message::LinkClicked(uri) => Update::event(Event::OpenLink(uri)),
        Message::Move(motion, count) => {
            if state
                .caret
                .move_by(state.elements.elements(), motion, count)
            {
                Update::task(reveal_caret())
            } else {
                Update::none()
            }
        }
        Message::MoveWord(motion, count) => {
            if state
                .caret
                .move_word(state.elements.elements(), motion, count)
            {
                Update::task(reveal_caret())
            } else {
                Update::none()
            }
        }
        Message::Jump(jump, count) => {
            if state.caret.jump(state.elements.elements(), jump, count) {
                Update::task(reveal_caret())
            } else {
                Update::none()
            }
        }
        Message::Cancel => {
            state.clear_visual_selection();
            Update::none()
        }
        Message::ToggleVisual => {
            state.visual_anchor = context.visual_active.then(|| state.caret());
            Update::none()
        }
        Message::Yank => {
            // The input mode has already left visual mode when the reducer
            // runs, so the anchor is spent here, never re-armed — like vim's
            // visual-mode `y`.
            let selection = state.visual_selection();
            state.visual_anchor = None;

            let Some(text) = selection
                .map(|span| selection_text(state.elements.elements(), span))
                .filter(|text| !text.is_empty())
            else {
                return Update::none();
            };

            // The yanked span keeps flashing for a moment, like Neovim's
            // `TextYankPost` highlight; the clipboard write and the yank
            // report are the application's to do.
            state.yank_flash = selection;
            Update::event(Event::Yanked { text })
        }
        Message::ClearFlash => {
            state.yank_flash = None;
            Update::none()
        }
        Message::ScrollBy(y) => Update::task(scroll_by(y)),
        Message::ScrollPage(page, count) => Update::task(scroll_page(page, count)),
        Message::ScrollCaret(placement) => Update::task(place_caret_in_view(placement)),
    }
}

#[cfg(test)]
mod tests {
    use super::{update, CaretPosition, Context, Message, Motion, State, WordMotion};
    use iced::widget::{markdown, text_editor};

    fn at(element: usize, column: usize) -> CaretPosition {
        CaretPosition { element, column }
    }

    fn assert_projection(state: &State, source: &str) {
        let expected = State::new(source);
        assert_eq!(state.elements(), expected.elements());
        assert_eq!(
            format!("{:?}", state.markdown().items()),
            format!("{:?}", markdown::Content::parse(source).items())
        );
    }

    #[test]
    fn source_operations_preserve_their_navigation_policies() {
        let mut state = State::new("old\n\nbody");
        state.place_caret(at(1, 2));
        update(
            &mut state,
            Message::ToggleVisual,
            Context {
                visual_active: true,
            },
        );

        state.replace_source("external\n\nreplacement");
        assert_projection(&state, "external\n\nreplacement");
        assert_eq!(state.caret(), at(0, 0));
        assert_eq!(state.visual_selection(), Some((at(1, 2), at(0, 0))));

        state.load_source("loaded");
        assert_projection(&state, "loaded");
        assert_eq!(state.caret(), at(0, 0));
        assert!(state.visual_selection().is_none());
    }

    #[test]
    fn entering_preview_refreshes_projection_and_uses_source_cursor() {
        let source = "first\n\nsecond\n\nthird";
        let mut content = text_editor::Content::with_text(source);
        content.move_to(text_editor::Cursor {
            position: text_editor::Position { line: 4, column: 2 },
            selection: Some(text_editor::Position { line: 2, column: 1 }),
        });
        let source_cursor = content.cursor();
        let mut state = State::new("stale");

        state.refresh_from_source(source, &content);

        assert_projection(&state, source);
        assert_eq!(state.caret(), at(2, 0));
        assert_eq!(content.cursor(), source_cursor);
        assert!(state.visual_selection().is_none());
    }

    #[test]
    fn reducer_owns_navigation_counts_and_keeps_scroll_work_local() {
        let mut state = State::new("one two\n\nthree\n\nfour");
        let context = Context {
            visual_active: false,
        };

        let result = update(&mut state, Message::Move(Motion::Down, 2), context);
        assert!(result.event.is_none());
        assert_eq!(state.caret(), at(2, 0));

        state.place_caret(at(0, 0));
        let result = update(
            &mut state,
            Message::MoveWord(WordMotion::NextStart, 2),
            context,
        );
        assert!(result.event.is_none());
        assert_eq!(state.caret(), at(1, 0));

        let result = update(&mut state, Message::Jump(super::Jump::Last, 0), context);
        assert!(result.event.is_none());
        assert_eq!(state.caret(), at(2, 0));
    }

    #[test]
    fn reducer_owns_visual_toggle_cancel_and_explicit_placement() {
        let mut state = State::new("first\n\nsecond");
        state.place_caret(at(1, 3));

        update(
            &mut state,
            Message::ToggleVisual,
            Context {
                visual_active: true,
            },
        );
        assert_eq!(state.visual_selection(), Some((at(1, 3), at(1, 3))));

        update(
            &mut state,
            Message::Move(Motion::Left, 2),
            Context {
                visual_active: true,
            },
        );
        assert_eq!(state.visual_selection(), Some((at(1, 3), at(1, 1))));

        update(
            &mut state,
            Message::Cancel,
            Context {
                visual_active: false,
            },
        );
        assert!(state.visual_selection().is_none());

        state.place_caret(at(0, 2));
        assert_eq!(state.caret(), at(0, 2));
    }

    /// Like vim's visual-mode `y`: the selection is spent, its text leaves
    /// as an event, and the yanked span stays behind as a flash for the
    /// viewer until the application's ticker clears it.
    #[test]
    fn yank_spends_the_selection_and_keeps_its_span_as_a_flash() {
        let mut state = State::new("one two\n\nthree four");
        state.place_caret(at(0, 4));
        update(
            &mut state,
            Message::ToggleVisual,
            Context {
                visual_active: true,
            },
        );
        update(
            &mut state,
            Message::Move(Motion::Right, 3),
            Context {
                visual_active: true,
            },
        );

        let result = update(
            &mut state,
            Message::Yank,
            Context {
                visual_active: false,
            },
        );
        match result.event {
            Some(super::Event::Yanked { text }) => assert_eq!(text, "two"),
            other => panic!("expected the yanked text, got {other:?}"),
        }
        assert_eq!(state.visual_selection(), None);
        assert_eq!(state.yank_flash(), Some((at(0, 4), at(0, 7))));
        assert!(state.flash_active());

        update(
            &mut state,
            Message::ClearFlash,
            Context {
                visual_active: false,
            },
        );
        assert_eq!(state.yank_flash(), None);
        assert!(!state.flash_active());
    }

    /// An empty selection yanks nothing — the clipboard keeps its contents
    /// and no flash appears — but the anchor is still spent, because the
    /// input mode has already left visual mode when the reducer runs.
    #[test]
    fn yanking_an_empty_selection_copies_nothing_and_flashes_nothing() {
        let mut state = State::new("alpha\n\nbeta");
        update(
            &mut state,
            Message::ToggleVisual,
            Context {
                visual_active: true,
            },
        );

        let result = update(
            &mut state,
            Message::Yank,
            Context {
                visual_active: false,
            },
        );
        assert!(result.event.is_none());
        assert_eq!(state.yank_flash(), None);
        assert_eq!(state.visual_selection(), None);
    }

    #[test]
    fn loading_a_new_document_clears_the_yank_flash() {
        let mut state = State::new("old\n\nbody");
        state.place_caret(at(0, 0));
        update(
            &mut state,
            Message::ToggleVisual,
            Context {
                visual_active: true,
            },
        );
        update(
            &mut state,
            Message::Move(Motion::Right, 3),
            Context {
                visual_active: true,
            },
        );
        update(
            &mut state,
            Message::Yank,
            Context {
                visual_active: false,
            },
        );
        assert!(state.flash_active());

        state.load_source("new");
        assert!(!state.flash_active());
    }
}
