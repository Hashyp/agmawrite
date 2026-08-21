//! Find feature state, reducer, navigation events, and popup view.

mod model;
mod view;

use std::ops::Range;

use iced::Task;

use crate::editing::{self, SourceMatch};
use crate::preview::PreviewElement;

use model::{preview_matches, Find, Way};

pub(crate) use view::view;

/// The read-only editing surface searched by a find transition.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Surface<'a> {
    Source(&'a str),
    Preview(&'a [PreviewElement]),
}

/// Find-local widget and navigation messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Message {
    Open,
    Close,
    QueryChanged(String),
    Next,
    Previous,
}

/// Exact navigation selected by the reducer for the composing app to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    Opened,
    Closed,
    SelectSource(SourceMatch),
    SelectPreview { element: usize, range: Range<usize> },
}

pub(crate) struct Update {
    pub(crate) task: Task<Message>,
    pub(crate) event: Option<Event>,
}

impl Update {
    fn task_and_event(task: Task<Message>, event: Event) -> Self {
        Self {
            task,
            event: Some(event),
        }
    }

    fn event(event: Option<Event>) -> Self {
        Self {
            task: Task::none(),
            event,
        }
    }
}

/// Query and current-match state owned by the find feature.
#[derive(Debug, Clone, Default)]
pub(crate) struct State {
    model: Find,
}

impl State {
    pub(crate) fn new() -> Self {
        Self { model: Find::new() }
    }

    /// The live query used by source and preview highlighters.
    pub(crate) fn query(&self) -> &str {
        self.model.query()
    }

    /// The current preview match used to paint its distinct highlight.
    pub(crate) fn current_preview_match(
        &self,
        elements: &[PreviewElement],
    ) -> Option<(usize, Range<usize>)> {
        let matches = preview_matches(elements, self.query());
        self.model
            .current(matches.len())
            .and_then(|index| matches.get(index).cloned())
    }
}

/// Applies a find-local transition against a read-only source or preview.
pub(crate) fn update(state: &mut State, message: Message, surface: Surface<'_>) -> Update {
    match message {
        Message::Open => Update::task_and_event(view::focus_input(), Event::Opened),
        Message::Close => Update::event(Some(Event::Closed)),
        Message::QueryChanged(query) => {
            state.model.set_query(&query);
            Update::event(select(state, Way::First, surface))
        }
        Message::Next => Update::event(select(state, Way::Next, surface)),
        Message::Previous => Update::event(select(state, Way::Previous, surface)),
    }
}

/// Focuses the find query when reopening it after Help closes.
pub(crate) fn focus_input() -> Task<Message> {
    view::focus_input()
}

fn select(state: &mut State, way: Way, surface: Surface<'_>) -> Option<Event> {
    if !state.model.is_active() {
        return None;
    }

    match surface {
        Surface::Source(source) => {
            let matches = editing::source_matches(source, state.query());
            let index = state.model.select(way, matches.len())?;
            Some(Event::SelectSource(matches[index].clone()))
        }
        Surface::Preview(elements) => {
            let matches = preview_matches(elements, state.query());
            let index = state.model.select(way, matches.len())?;
            let (element, range) = matches[index].clone();
            Some(Event::SelectPreview { element, range })
        }
    }
}

fn match_count(state: &State, surface: Surface<'_>) -> usize {
    if !state.model.is_active() {
        return 0;
    }

    match surface {
        Surface::Source(source) => editing::source_matches(source, state.query()).len(),
        Surface::Preview(elements) => preview_matches(elements, state.query()).len(),
    }
}

fn current(state: &State, total: usize) -> Option<usize> {
    state.model.current(total)
}

#[cfg(test)]
mod tests {
    use super::{update, Event, Message, State, Surface};
    use crate::editing::SourceMatch;
    use crate::preview::ElementMap;

    #[test]
    fn reducer_selects_and_steps_source_matches() {
        let source = "# Title\n\nbody text\n\nmore text";
        let mut state = State::new();

        let first = update(
            &mut state,
            Message::QueryChanged("text".to_owned()),
            Surface::Source(source),
        );
        assert_eq!(
            first.event,
            Some(Event::SelectSource(SourceMatch {
                line: 2,
                columns: 5..9,
            }))
        );

        let next = update(&mut state, Message::Next, Surface::Source(source));
        assert_eq!(
            next.event,
            Some(Event::SelectSource(SourceMatch {
                line: 4,
                columns: 5..9,
            }))
        );

        let wrapped = update(&mut state, Message::Next, Surface::Source(source));
        assert_eq!(
            wrapped.event,
            Some(Event::SelectSource(SourceMatch {
                line: 2,
                columns: 5..9,
            }))
        );

        let previous = update(&mut state, Message::Previous, Surface::Source(source));
        assert_eq!(
            previous.event,
            Some(Event::SelectSource(SourceMatch {
                line: 4,
                columns: 5..9,
            }))
        );
    }

    #[test]
    fn reducer_selects_preview_matches_and_restarts_on_query_change() {
        let map = ElementMap::parse("# Title\n\nbody text\n\nmore text");
        let surface = Surface::Preview(map.elements());
        let mut state = State::new();

        let first = update(
            &mut state,
            Message::QueryChanged("text".to_owned()),
            surface,
        );
        assert_eq!(
            first.event,
            Some(Event::SelectPreview {
                element: 1,
                range: 5..9,
            })
        );

        let next = update(&mut state, Message::Next, surface);
        assert_eq!(
            next.event,
            Some(Event::SelectPreview {
                element: 2,
                range: 5..9,
            })
        );

        let restarted = update(
            &mut state,
            Message::QueryChanged("more".to_owned()),
            surface,
        );
        assert_eq!(
            restarted.event,
            Some(Event::SelectPreview {
                element: 2,
                range: 0..4,
            })
        );
        assert_eq!(state.current_preview_match(map.elements()), Some((2, 0..4)));

        let empty = update(&mut state, Message::QueryChanged(String::new()), surface);
        assert!(empty.event.is_none());
        assert_eq!(state.current_preview_match(map.elements()), None);
    }
}
