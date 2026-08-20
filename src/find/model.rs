//! The find feature: the query, which match is current, and the match
//! lists the two editing surfaces navigate.
//!
//! Matching itself is ASCII-case-insensitive and lives in the editing
//! module ([`crate::editing`]); this module owns the selection state —
//! the query plus the index of the current match — and the enumeration
//! that orders matches across the preview's elements.

use std::ops::Range;

use crate::editing;
use crate::preview::PreviewElement;

/// The state of the find popup: the query typed so far and the index of
/// the current match among the live ones.
#[derive(Debug, Clone, Default)]
pub struct Find {
    query: String,
    current: Option<usize>,
}

/// Which match becomes the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// The first match — what every keystroke restarts at.
    First,
    /// The next match, wrapping around the end.
    Next,
    /// The previous match, wrapping around the start.
    Previous,
}

impl Find {
    pub fn new() -> Self {
        Self::default()
    }

    /// The query, matched case-insensitively.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether the query is live: matches highlight and navigate.
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// Replaces the query, unsetting the current match — the next
    /// selection restarts at the first match.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_owned();
        self.current = None;
    }

    /// The current match's index among `total` live matches, clamped
    /// when the document shifted underneath it.
    pub fn current(&self, total: usize) -> Option<usize> {
        self.current.map(|index| index.min(total.saturating_sub(1)))
    }

    /// Makes the match `way` points at current and returns its index;
    /// `None` when there is nothing to select.
    pub fn select(&mut self, way: Way, total: usize) -> Option<usize> {
        match way {
            Way::First => (total > 0).then(|| {
                self.current = Some(0);
                0
            }),
            Way::Next => self.step(total, 1),
            Way::Previous => self.step(total, total.saturating_sub(1)),
        }
    }

    /// Steps `forward` matches from the current one, wrapping around;
    /// without a current match, selects the first.
    fn step(&mut self, total: usize, forward: usize) -> Option<usize> {
        if total == 0 {
            return None;
        }

        let index = match self.current(total) {
            Some(index) => (index + forward) % total,
            None => 0,
        };

        self.current = Some(index);
        Some(index)
    }
}

/// All matches over the preview elements, in document order: the element
/// each match lives in and its grapheme range within it — the units the
/// preview caret and highlights use.
pub fn preview_matches(elements: &[PreviewElement], query: &str) -> Vec<(usize, Range<usize>)> {
    elements
        .iter()
        .enumerate()
        .flat_map(|(index, element)| {
            editing::matches_in(element.text(), query)
                .into_iter()
                .map(move |range| (index, range))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{preview_matches, Find, Way};
    use crate::preview::ElementMap;

    /// The find state walks the match list: selecting restarts at the
    /// first match, next/previous step and wrap around, and a stale index
    /// clamps to the live match count.
    #[test]
    fn the_current_match_walks_and_wraps() {
        let mut find = Find::new();
        assert_eq!(find.query(), "");
        assert!(!find.is_active());

        find.set_query("word");
        assert_eq!(find.query(), "word");
        assert!(find.is_active());

        assert_eq!(find.current(3), None);
        assert_eq!(find.select(Way::First, 3), Some(0));
        assert_eq!(find.current(3), Some(0));
        assert_eq!(find.select(Way::Next, 3), Some(1));
        assert_eq!(find.select(Way::Next, 3), Some(2));
        assert_eq!(find.select(Way::Next, 3), Some(0)); // wraps
        assert_eq!(find.select(Way::Previous, 3), Some(2)); // wraps back
        assert_eq!(find.select(Way::Previous, 3), Some(1));

        // A stale index clamps to the live count as matches shift under it.
        assert_eq!(find.select(Way::First, 5), Some(0));
        find.select(Way::Next, 5);
        find.select(Way::Next, 5);
        find.select(Way::Next, 5); // current = 3
        assert_eq!(find.current(2), Some(1));

        // No matches: nothing to select, in any direction.
        assert_eq!(find.select(Way::First, 0), None);
        assert_eq!(find.select(Way::Next, 0), None);
        assert_eq!(find.select(Way::Previous, 0), None);

        // Retyping restarts from the first match.
        find.set_query("other");
        assert_eq!(find.current(4), None);
        assert_eq!(find.select(Way::Next, 4), Some(0));
    }

    /// Matches enumerate across the preview elements in document order —
    /// element index and grapheme range — case-insensitively.
    #[test]
    fn preview_matches_walk_elements_in_order() {
        let map = ElementMap::parse("one two\n\nthree ONE");
        let elements = map.elements();

        assert_eq!(preview_matches(elements, "one"), vec![(0, 0..3), (1, 6..9)]);
        assert!(preview_matches(elements, "zzz").is_empty());
        assert!(preview_matches(elements, "").is_empty());
    }
}
