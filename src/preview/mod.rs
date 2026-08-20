//! Preview feature state, reducer, and parsed caret model.

mod model;

use model::Caret;
pub(crate) use model::{
    element_selection, CaretPosition, Claims, ElementMap, Jump, Motion, Page, Placement,
    PreviewElement, WordMotion,
};

use iced::widget::{markdown, text_editor};

/// Preview-local navigation and scrolling messages.
#[derive(Debug, Clone)]
pub(crate) enum Message {
    Toggle,
    LinkClicked(markdown::Uri),
    Move(Motion, usize),
    MoveWord(WordMotion, usize),
    Jump(Jump, usize),
    AcknowledgeG,
    AcknowledgeZ,
    AcknowledgeCount(u32),
    Cancel,
    ToggleVisual,
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

/// Semantic work that remains at the application boundary until preview
/// scrolling and link handling are extracted in later tasks.
#[derive(Debug, Clone)]
pub(crate) enum Event {
    ToggleRequested,
    OpenLink(markdown::Uri),
    RevealCaret,
    ScrollBy(f32),
    ScrollPage(Page, usize),
    ScrollCaret(Placement),
}

pub(crate) struct Update {
    pub(crate) event: Option<Event>,
}

impl Update {
    fn none() -> Self {
        Self { event: None }
    }

    fn event(event: Event) -> Self {
        Self { event: Some(event) }
    }
}

/// The complete preview projection and navigation state.
pub(crate) struct State {
    markdown: markdown::Content,
    elements: ElementMap,
    caret: Caret,
    visual_anchor: Option<CaretPosition>,
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
    }

    /// Refreshes source edits when entering preview and places the preview
    /// caret at the source editor cursor.
    pub(crate) fn refresh_from_source(&mut self, source: &str, content: &text_editor::Content) {
        self.markdown = markdown::Content::parse(source);
        self.elements = ElementMap::parse(source);
        self.caret
            .move_to_source_cursor(content, self.elements.elements());
        self.clear_visual_selection();
    }

    /// Places the caret for app-coordinated comment or find navigation.
    pub(crate) fn place_caret(&mut self, position: CaretPosition) {
        self.caret.place(position);
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
}

impl Default for State {
    fn default() -> Self {
        Self::new("")
    }
}

/// Applies one preview transition. Widget-tree scrolling remains an app-owned
/// event until Task 15; navigation and visual state mutate only here.
pub(crate) fn update(state: &mut State, message: Message, context: Context) -> Update {
    match message {
        Message::Toggle => Update::event(Event::ToggleRequested),
        Message::LinkClicked(uri) => Update::event(Event::OpenLink(uri)),
        Message::Move(motion, count) => {
            if state
                .caret
                .move_by(state.elements.elements(), motion, count)
            {
                Update::event(Event::RevealCaret)
            } else {
                Update::none()
            }
        }
        Message::MoveWord(motion, count) => {
            if state
                .caret
                .move_word(state.elements.elements(), motion, count)
            {
                Update::event(Event::RevealCaret)
            } else {
                Update::none()
            }
        }
        Message::Jump(jump, count) => {
            if state.caret.jump(state.elements.elements(), jump, count) {
                Update::event(Event::RevealCaret)
            } else {
                Update::none()
            }
        }
        Message::AcknowledgeG | Message::AcknowledgeZ | Message::AcknowledgeCount(_) => {
            Update::none()
        }
        Message::Cancel => {
            state.clear_visual_selection();
            Update::none()
        }
        Message::ToggleVisual => {
            state.visual_anchor = context.visual_active.then(|| state.caret());
            Update::none()
        }
        Message::ScrollBy(y) => Update::event(Event::ScrollBy(y)),
        Message::ScrollPage(page, count) => Update::event(Event::ScrollPage(page, count)),
        Message::ScrollCaret(placement) => Update::event(Event::ScrollCaret(placement)),
    }
}

#[cfg(test)]
mod tests {
    use super::{update, CaretPosition, Context, Event, Message, Motion, State, WordMotion};
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
    fn reducer_owns_navigation_counts_and_reveal_requests() {
        let mut state = State::new("one two\n\nthree\n\nfour");
        let context = Context {
            visual_active: false,
        };

        assert!(matches!(
            update(&mut state, Message::Move(Motion::Down, 2), context).event,
            Some(Event::RevealCaret)
        ));
        assert_eq!(state.caret(), at(2, 0));

        state.place_caret(at(0, 0));
        assert!(matches!(
            update(
                &mut state,
                Message::MoveWord(WordMotion::NextStart, 2),
                context
            )
            .event,
            Some(Event::RevealCaret)
        ));
        assert_eq!(state.caret(), at(1, 0));

        assert!(matches!(
            update(&mut state, Message::Jump(super::Jump::Last, 0), context).event,
            Some(Event::RevealCaret)
        ));
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
}
