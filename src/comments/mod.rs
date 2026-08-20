//! Comments feature state, reducer, semantic events, and domain model.

mod model;

pub use model::{CommentCard, Mark};
use model::{Comments, Span};

use iced::widget::text_editor;

use crate::preview::{CaretPosition, PreviewElement};

/// Comment-local widget and workflow messages.
#[derive(Debug, Clone)]
pub(crate) enum Message {
    OpenComposer,
    CloseComposer,
    EditComposer(text_editor::Action),
    SaveComposer,
    EditActive,
    ComposerCardPressed,
    ActivateCard(usize, usize),
    DeleteComment(usize, usize),
    ResolveThread(usize),
    EditDraft(text_editor::Action),
    AddDraftAsGlobal,
    PublishDraft,
    Cycle,
    ToggleSidebar,
}

/// Read-only app context needed to anchor comments and preserve the existing
/// save-without-an-open-popup no-op behavior.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Context {
    pub(crate) caret: CaretPosition,
    pub(crate) selection: Option<(CaretPosition, CaretPosition)>,
    pub(crate) composer_open: bool,
}

/// Semantic consequences interpreted by the composing application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    NavigateTo(CaretPosition),
    FocusComposer,
    PublishRequested,
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

/// Comments owned as one feature: the domain store, note composer draft,
/// optional comment being edited, and sidebar visibility override.
pub(crate) struct State {
    model: Comments,
    composer: text_editor::Content,
    editing_target: Option<(usize, usize)>,
    sidebar_override: Option<bool>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            model: Comments::new(),
            composer: text_editor::Content::new(),
            editing_target: None,
            sidebar_override: None,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.model.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.model.len()
    }

    pub(crate) fn mark_for(&self, element: usize, len: usize) -> Mark {
        self.model.mark_for(element, len)
    }

    pub(crate) fn anchor_selection_for(
        &self,
        element: usize,
        len: usize,
    ) -> Option<std::ops::Range<usize>> {
        self.model.anchor_selection_for(element, len)
    }

    pub(crate) fn cards(&self, source: &str, elements: &[PreviewElement]) -> Vec<CommentCard> {
        self.model.cards(source, elements)
    }

    pub(crate) fn draft(&self) -> &text_editor::Content {
        self.model.draft()
    }

    pub(crate) fn composer(&self) -> &text_editor::Content {
        &self.composer
    }

    pub(crate) fn editing_target(&self) -> Option<(usize, usize)> {
        self.editing_target
    }

    pub(crate) fn active_history(&self) -> &[String] {
        self.model.active_history()
    }

    /// Whether the sidebar is visible: an explicit toggle wins, otherwise
    /// comments make it appear automatically.
    pub(crate) fn sidebar_shown(&self) -> bool {
        self.sidebar_override.unwrap_or(!self.model.is_empty())
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

/// Applies one comments message. Domain mutations remain entirely behind this
/// feature boundary; the app receives only semantic coordination events.
pub(crate) fn update(state: &mut State, message: Message, context: Context) -> Update {
    match message {
        Message::OpenComposer => Update::event(Event::FocusComposer),
        Message::CloseComposer => {
            state.composer = text_editor::Content::new();
            state.editing_target = None;
            Update::none()
        }
        Message::EditComposer(action) => {
            state.composer.perform(action);
            Update::none()
        }
        Message::SaveComposer => {
            if !context.composer_open {
                return Update::none();
            }

            let text = state.composer.text();

            if let Some((thread, entry)) = state.editing_target.take() {
                state.model.activate(thread, entry);
                state.model.edit_active(&text);
            } else if let Some((anchor, caret)) = context.selection {
                state.model.save_selection(&text, Span::new(anchor, caret));
            } else {
                state.model.save(&text, context.caret);
            }

            state.composer = text_editor::Content::new();
            Update::none()
        }
        Message::EditActive => {
            let Some(text) = state.model.active_text().map(str::to_owned) else {
                return Update::none();
            };

            state.composer = text_editor::Content::with_text(&text);
            state.editing_target = state.model.active_entry();
            Update::event(Event::FocusComposer)
        }
        // The card swallows clicks so they do not reach the editing surface.
        Message::ComposerCardPressed => Update::none(),
        Message::ActivateCard(thread, entry) => state
            .model
            .activate(thread, entry)
            .map_or_else(Update::none, |position| {
                Update::event(Event::NavigateTo(position))
            }),
        Message::DeleteComment(thread, entry) => {
            state.model.delete(thread, entry);

            if state.editing_target == Some((thread, entry)) {
                state.editing_target = None;
                state.composer = text_editor::Content::new();
            }

            Update::none()
        }
        Message::ResolveThread(thread) => {
            state.model.resolve(thread);
            Update::none()
        }
        Message::EditDraft(action) => {
            state.model.edit_draft(action);
            Update::none()
        }
        Message::AddDraftAsGlobal => {
            state.model.add_draft_as_global();
            Update::none()
        }
        Message::PublishDraft => Update::event(Event::PublishRequested),
        Message::Cycle => state.model.cycle().map_or_else(Update::none, |position| {
            Update::event(Event::NavigateTo(position))
        }),
        Message::ToggleSidebar => {
            state.sidebar_override = Some(!state.sidebar_shown());
            Update::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{update, Context, Event, Message, State};
    use crate::preview::{CaretPosition, ElementMap};
    use iced::widget::text_editor::{Action, Edit};
    use std::sync::Arc;

    fn at(element: usize, column: usize) -> CaretPosition {
        CaretPosition { element, column }
    }

    fn context(caret: CaretPosition) -> Context {
        Context {
            caret,
            selection: None,
            composer_open: true,
        }
    }

    fn paste(text: &str) -> Action {
        Action::Edit(Edit::Paste(Arc::new(text.to_owned())))
    }

    fn edit_composer(state: &mut State, text: &str) {
        update(state, Message::EditComposer(paste(text)), context(at(0, 0)));
    }

    fn save_at(state: &mut State, text: &str, position: CaretPosition) {
        edit_composer(state, text);
        update(state, Message::SaveComposer, context(position));
    }

    #[test]
    fn saving_and_dismissing_the_composer_preserve_note_behavior() {
        let mut state = State::new();
        let position = at(1, 2);

        edit_composer(&mut state, "  fix this  \n");
        update(&mut state, Message::SaveComposer, context(position));

        assert_eq!(state.composer().text(), "");
        assert_eq!(state.len(), 1);
        assert_eq!(
            state.cards(
                "# Title\n\nbody",
                ElementMap::parse("# Title\n\nbody").elements()
            )[0]
            .text,
            "fix this"
        );
        assert_eq!(
            update(&mut state, Message::Cycle, context(position)).event,
            Some(Event::NavigateTo(position))
        );

        edit_composer(&mut state, "half-written");
        update(&mut state, Message::CloseComposer, context(position));
        assert_eq!(state.composer().text(), "");
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn saving_without_an_open_composer_is_a_no_op() {
        let mut state = State::new();
        edit_composer(&mut state, "should not save");

        update(
            &mut state,
            Message::SaveComposer,
            Context {
                composer_open: false,
                ..context(at(0, 0))
            },
        );

        assert!(state.is_empty());
        assert_eq!(state.composer().text(), "should not save");
    }

    #[test]
    fn composer_saves_comments_on_code_block_elements() {
        let markdown = "text\n\n```\ncode line\n```\n\nafter";
        let elements = ElementMap::parse(markdown);
        let mut state = State::new();

        save_at(&mut state, "about the code", at(1, 4));

        assert_eq!(state.mark_for(1, 32), super::Mark::Active);
        assert!(state.cards(markdown, elements.elements())[0]
            .quote
            .contains("```"));
    }

    #[test]
    fn selection_context_anchors_the_saved_comment() {
        let markdown = "alpha\n\nbeta\n\ngamma";
        let elements = ElementMap::parse(markdown);
        let mut state = State::new();
        edit_composer(&mut state, "about the span");

        update(
            &mut state,
            Message::SaveComposer,
            Context {
                caret: at(2, 3),
                selection: Some((at(1, 1), at(2, 3))),
                composer_open: true,
            },
        );

        let cards = state.cards(markdown, elements.elements());
        assert_eq!(cards[0].quote, "eta gam");
        assert_eq!(state.anchor_selection_for(1, 4), Some(1..4));
        assert_eq!(state.anchor_selection_for(2, 5), Some(0..3));
    }

    #[test]
    fn editing_keeps_history_and_dismissal_discards_changes() {
        let mut state = State::new();
        save_at(&mut state, "first", at(1, 0));

        let result = update(&mut state, Message::EditActive, context(at(1, 0)));
        assert_eq!(result.event, Some(Event::FocusComposer));
        assert_eq!(state.editing_target(), Some((0, 0)));
        assert_eq!(state.composer().text(), "first");

        update(
            &mut state,
            Message::EditComposer(Action::SelectAll),
            context(at(1, 0)),
        );
        update(
            &mut state,
            Message::EditComposer(paste("  second take  ")),
            context(at(1, 0)),
        );
        update(&mut state, Message::SaveComposer, context(at(1, 0)));
        assert_eq!(state.cards("", &[])[0].text, "second take");
        assert_eq!(state.active_history(), ["first"]);

        update(&mut state, Message::EditActive, context(at(1, 0)));
        update(
            &mut state,
            Message::EditComposer(Action::SelectAll),
            context(at(1, 0)),
        );
        update(
            &mut state,
            Message::EditComposer(paste("nope")),
            context(at(1, 0)),
        );
        update(&mut state, Message::CloseComposer, context(at(1, 0)));
        assert_eq!(state.cards("", &[])[0].text, "second take");
        assert_eq!(state.composer().text(), "");
        assert!(state.editing_target().is_none());
    }

    #[test]
    fn threads_grow_resolve_and_delete_through_the_reducer() {
        let mut state = State::new();
        save_at(&mut state, "root", at(0, 0));
        save_at(&mut state, "reply", at(0, 0));

        let cards = state.cards("one\n\ntwo", ElementMap::parse("one\n\ntwo").elements());
        assert_eq!((cards[1].thread, cards[1].entry), (0, 1));
        assert_eq!(cards[1].depth, 1);

        update(&mut state, Message::ResolveThread(0), context(at(0, 0)));
        assert_eq!(state.mark_for(0, 32), super::Mark::None);
        update(&mut state, Message::ResolveThread(0), context(at(0, 0)));
        assert_eq!(state.mark_for(0, 32), super::Mark::Active);

        update(&mut state, Message::DeleteComment(0, 1), context(at(0, 0)));
        assert_eq!(state.len(), 1);
        update(&mut state, Message::DeleteComment(0, 0), context(at(0, 0)));
        assert!(state.is_empty());
    }

    #[test]
    fn draft_add_and_publish_are_local_workflow_actions() {
        let mut state = State::new();
        update(
            &mut state,
            Message::EditDraft(paste("  overall note  ")),
            context(at(0, 0)),
        );
        update(&mut state, Message::AddDraftAsGlobal, context(at(0, 0)));

        assert_eq!(state.len(), 1);
        let cards = state.cards("", &[]);
        assert_eq!(cards[0].label, Some("Global"));
        assert_eq!(cards[0].text, "overall note");
        assert_eq!(state.draft().text(), "");
        assert_eq!(
            update(&mut state, Message::PublishDraft, context(at(0, 0))).event,
            Some(Event::PublishRequested)
        );
    }

    #[test]
    fn composer_focus_delete_and_sidebar_visibility_stay_feature_local() {
        let mut state = State::new();
        assert!(!state.sidebar_shown());
        assert_eq!(
            update(&mut state, Message::OpenComposer, context(at(0, 0))).event,
            Some(Event::FocusComposer)
        );

        save_at(&mut state, "note", at(0, 0));
        assert!(state.sidebar_shown());
        update(&mut state, Message::ToggleSidebar, context(at(0, 0)));
        assert!(!state.sidebar_shown());

        update(&mut state, Message::EditActive, context(at(0, 0)));
        assert_eq!(state.editing_target(), Some((0, 0)));
        update(&mut state, Message::DeleteComment(0, 0), context(at(0, 0)));
        assert!(state.editing_target().is_none());
        assert_eq!(state.composer().text(), "");
        assert!(state.is_empty());
    }

    #[test]
    fn card_activation_and_cycle_emit_navigation_requests() {
        let mut state = State::new();
        let position = at(2, 4);
        save_at(&mut state, "note", position);

        assert_eq!(
            update(&mut state, Message::ActivateCard(0, 0), context(at(0, 0))).event,
            Some(Event::NavigateTo(position))
        );
        assert_eq!(
            update(&mut state, Message::Cycle, context(at(0, 0))).event,
            Some(Event::NavigateTo(position))
        );
    }
}
