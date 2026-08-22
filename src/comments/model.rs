//! The comments feature: threads of notes anchored to preview elements
//! (a spot or a selected span), the active comment, the edit history, and
//! the publish draft.
//!
//! The anchor representation is private to this module — it crosses the seam
//! only as [`CaretPosition`]s. Element indices rot when the preview
//! re-parses after an edit; when anchors move to a stable representation
//! (e.g. source offsets), only this module changes.
//!
//! A **thread** is the tree the sidebar renders: its root comment plus the
//! replies saved on the same anchor, and a resolved flag. Resolved threads
//! are history — they stop marking elements, cycling skips them, and the
//! sidebar lists them below the open ones. Editing a comment keeps its
//! previous texts as history.

use iced::widget::text_editor;

use crate::preview::{CaretPosition, PreviewElement};

/// How many characters of an element's Markdown source a comment card
/// quotes before cutting it off.
const COMMENT_QUOTE_MAX_CHARS: usize = 60;

/// How many characters of the comment text itself a card shows before
/// cutting it off, so every card keeps the same bounded height in the
/// sidebar and no comment dominates the list.
const COMMENT_TEXT_MAX_CHARS: usize = 100;

/// Whether a preview element carries a comment, and which kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Commented,
    /// The element the currently active comment is anchored to.
    Active,
}

/// A bordered rectangle framing commented text in a preview element: the
/// grapheme range the rectangle surrounds and whether it belongs to the
/// currently active comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    pub range: std::ops::Range<usize>,
    pub mark: Mark,
}

/// What a comment thread is anchored to: a caret position in the preview,
/// a span of selected text, or nothing — a **global comment** about the
/// document as a whole, labeled "Global" in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Caret(CaretPosition),
    Selection(Span),
    Global,
}

/// A span of the preview between two caret positions, normalized so
/// `start <= end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    start: CaretPosition,
    end: CaretPosition,
}

impl Span {
    pub fn new(a: CaretPosition, b: CaretPosition) -> Self {
        if a <= b {
            Self { start: a, end: b }
        } else {
            Self { start: b, end: a }
        }
    }

    /// The position the caret jumps to: the start of the span.
    pub fn start(&self) -> CaretPosition {
        self.start
    }
}

impl Anchor {
    /// The position the caret jumps to when the thread activates: where
    /// the anchor begins. Global anchors have nowhere to jump to.
    fn position(&self) -> Option<CaretPosition> {
        match self {
            Anchor::Caret(position) => Some(*position),
            Anchor::Selection(span) => Some(span.start()),
            Anchor::Global => None,
        }
    }

    /// Whether a caret position falls inside the anchor: the same element
    /// for spot anchors, anywhere within the span for selections. Saving a
    /// note at a covered position replies to that thread.
    fn covers(&self, position: CaretPosition) -> bool {
        match self {
            Anchor::Caret(at) => at.element == position.element,
            Anchor::Selection(span) => span.start <= position && position <= span.end,
            Anchor::Global => false,
        }
    }

    /// Whether the anchor marks the given element: spot anchors mark
    /// their element, selection anchors mark exactly the elements that
    /// contribute at least one character of the span — an endpoint at
    /// column 0 marks nothing.
    fn marks(&self, element: usize, len: usize) -> bool {
        match self {
            Anchor::Caret(at) => at.element == element,
            Anchor::Selection(span) => span_slice(*span, element, len).is_some(),
            Anchor::Global => false,
        }
    }
}

/// One comment in a thread: its current text and the texts it replaced.
#[derive(Debug, Clone)]
struct Entry {
    text: String,
    /// Previous versions of the text, oldest first — the comment history.
    history: Vec<String>,
}

/// A thread of comments sharing one anchor: the root comment plus its
/// replies, and whether the discussion was resolved.
#[derive(Debug, Clone)]
struct Thread {
    entries: Vec<Entry>,
    anchor: Anchor,
    resolved: bool,
}

/// A comment as the sidebar renders it.
#[derive(Debug, Clone)]
pub struct CommentCard {
    /// The thread and entry this card shows — what clicking, deleting,
    /// and editing address.
    pub thread: usize,
    pub entry: usize,
    /// The note text, condensed like the quote: collapsed to one line and
    /// cut off, so every card occupies the same vertical space.
    pub text: String,
    pub quote: String,
    /// The card's label when the comment has no anchor: `"Global"`.
    pub label: Option<&'static str>,
    pub active: bool,
    /// Whether this card is the thread's root — the one quoting the
    /// anchored source.
    pub first: bool,
    /// Whether the thread is resolved history.
    pub resolved: bool,
    /// How deep in the thread the comment sits: replies indent.
    pub depth: usize,
    /// How many previous versions the comment has — its edit history.
    pub history: usize,
}

/// The comments store: threads of comments (oldest first), the active
/// comment, and the publish draft.
pub struct Comments {
    threads: Vec<Thread>,
    active: Option<(usize, usize)>,
    draft: text_editor::Content,
}

impl Comments {
    pub fn new() -> Self {
        Self {
            threads: Vec::new(),
            active: None,
            draft: text_editor::Content::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.threads.is_empty()
    }

    /// The number of comments across all threads.
    pub fn len(&self) -> usize {
        self.threads.iter().map(|thread| thread.entries.len()).sum()
    }

    /// Saves a comment anchored at `at`, replying to the open thread that
    /// covers the position when there is one. Empty notes are discarded.
    pub fn save(&mut self, text: &str, at: CaretPosition) {
        self.save_with(text, Anchor::Caret(at));
    }

    /// Saves a comment anchored to the selected span, replying to the open
    /// thread anchored to the same span when there is one. Empty notes are
    /// discarded.
    pub fn save_selection(&mut self, text: &str, span: Span) {
        self.save_with(text, Anchor::Selection(span));
    }

    /// Saves a global comment — unanchored, about the document as a whole
    /// — as its own thread, and makes it the active one. Empty notes are
    /// discarded.
    #[allow(dead_code)] // exercised by the tests; the sidebar adds globals via the draft
    pub fn save_global(&mut self, text: &str) {
        let text = trimmed(text);

        if text.is_empty() {
            return;
        }

        self.push_new(text, Anchor::Global);
    }

    /// Adds the publish draft as a global comment and clears the draft.
    /// Empty drafts are discarded.
    pub fn add_draft_as_global(&mut self) {
        let text = self.draft.text().trim().to_owned();

        if text.is_empty() {
            return;
        }

        self.push_new(&text, Anchor::Global);
        self.draft = text_editor::Content::new();
    }

    fn save_with(&mut self, text: &str, anchor: Anchor) {
        let text = trimmed(text);
        if text.is_empty() {
            return;
        }

        // A note on an open thread's anchor replies to that thread: the
        // active thread wins when it covers the position, otherwise the
        // most recent open thread that does.
        let target = self.reply_target(&anchor);

        match target {
            Some(index) => {
                let thread = &mut self.threads[index];
                thread.entries.push(Entry {
                    text: text.to_owned(),
                    history: Vec::new(),
                });
                self.active = Some((index, thread.entries.len() - 1));
            }
            None => self.push_new(text, anchor),
        }
    }

    /// The open thread a note with this anchor replies to, if any: the
    /// active thread when it covers the anchor, otherwise the most recent
    /// open thread that does. Resolved threads are history and never take
    /// replies.
    fn reply_target(&self, anchor: &Anchor) -> Option<usize> {
        let covers = |thread: &Thread| match anchor {
            Anchor::Caret(position) => thread.anchor.covers(*position),
            Anchor::Selection(span) => thread.anchor == Anchor::Selection(*span),
            Anchor::Global => false,
        };

        if self.active.is_some_and(|(thread, _)| {
            !self.threads[thread].resolved && covers(&self.threads[thread])
        }) {
            return self.active.map(|(thread, _)| thread);
        }

        self.threads
            .iter()
            .enumerate()
            .rev()
            .find(|(_, thread)| !thread.resolved && covers(thread))
            .map(|(index, _)| index)
    }

    fn push_new(&mut self, text: &str, anchor: Anchor) {
        self.threads.push(Thread {
            entries: vec![Entry {
                text: text.to_owned(),
                history: Vec::new(),
            }],
            anchor,
            resolved: false,
        });
        let last = self.threads.len() - 1;
        self.active = Some((last, 0));
    }

    /// Cycles the active comment forward through the anchored, open
    /// threads, wrapping around at the end, and returns where the caret
    /// should jump to. Global and resolved threads are skipped — there is
    /// nothing to jump to, and history stays history.
    pub fn cycle(&mut self) -> Option<CaretPosition> {
        let start = self.active.map_or(0, |(thread, _)| thread + 1);

        for step in 0..self.threads.len() {
            let index = (start + step) % self.threads.len();
            let thread = &self.threads[index];

            if !thread.resolved {
                if let Some(position) = thread.anchor.position() {
                    self.active = Some((index, 0));
                    return Some(position);
                }
            }
        }

        None
    }

    /// The mark a preview element carries: the active comment's element is
    /// `Active`, any other open thread's element is `Commented`. Resolved
    /// threads mark nothing.
    pub fn mark_for(&self, element: usize, len: usize) -> Mark {
        let marks = |thread: &Thread| !thread.resolved && thread.anchor.marks(element, len);

        if self
            .active
            .is_some_and(|(index, _)| marks(&self.threads[index]))
        {
            return Mark::Active;
        }

        if self.threads.iter().any(marks) {
            Mark::Commented
        } else {
            Mark::None
        }
    }

    /// The bordered rectangles framing this element's commented text: one
    /// per open thread anchored here — exactly its selected span for
    /// selection anchors, the whole element for spot anchors. Commented
    /// outlines come first and the active one last, so the active border
    /// paints on top where threads overlap. Resolved threads frame
    /// nothing.
    pub fn outlines_for(&self, element: usize, len: usize) -> Vec<Outline> {
        let mut outlines = Vec::new();

        for (index, thread) in self.threads.iter().enumerate() {
            if thread.resolved || !thread.anchor.marks(element, len) {
                continue;
            }

            let range = match thread.anchor {
                Anchor::Selection(span) => span_slice(span, element, len),
                _ => (len > 0).then_some(0..len),
            };

            let Some(range) = range else {
                continue;
            };

            let mark = if self.active.is_some_and(|(active, _)| active == index) {
                Mark::Active
            } else {
                Mark::Commented
            };

            outlines.push(Outline { range, mark });
        }

        outlines.sort_by_key(|outline| outline.mark == Mark::Active);
        outlines
    }

    /// Whether the active comment's thread anchors this element — resolved
    /// or not — so activating a comment can scroll its element into view
    /// without moving the caret onto it.
    pub fn anchors_element(&self, element: usize, len: usize) -> bool {
        self.active
            .and_then(|(index, _)| self.threads.get(index))
            .is_some_and(|thread| thread.anchor.marks(element, len))
    }

    /// The slice of the element's text the active selection anchor
    /// covers, so the preview can highlight exactly the selected text:
    /// partial at the span's endpoints, whole elements in between. Spot
    /// and global anchors highlight nothing beyond the element-level mark.
    pub fn anchor_selection_for(
        &self,
        element: usize,
        len: usize,
    ) -> Option<std::ops::Range<usize>> {
        let active = self
            .active
            .and_then(|(index, _)| self.threads.get(index))
            .filter(|thread| !thread.resolved);

        if let Some(Anchor::Selection(span)) = active.map(|thread| thread.anchor) {
            return span_slice(span, element, len);
        }

        None
    }

    /// Activates the comment at `(thread, entry)` — the card that was
    /// clicked — and returns where the caret should jump to. Global
    /// comments activate but have nowhere to jump to; unknown indices
    /// change nothing.
    pub fn activate(&mut self, thread: usize, entry: usize) -> Option<CaretPosition> {
        let target = self.threads.get(thread)?;
        let _ = target.entries.get(entry)?;
        self.active = Some((thread, entry));

        target.anchor.position()
    }

    /// The active comment, as the `(thread, entry)` the popup edits and
    /// the sidebar highlights.
    pub fn active_entry(&self) -> Option<(usize, usize)> {
        self.active
    }

    /// The current text of the active comment, for the edit popup.
    pub fn active_text(&self) -> Option<&str> {
        let (thread, entry) = self.active?;
        self.threads
            .get(thread)
            .and_then(|thread| thread.entries.get(entry))
            .map(|entry| entry.text.as_str())
    }

    /// The edit history of the active comment, oldest first.
    pub fn active_history(&self) -> &[String] {
        match self.active {
            Some((thread, entry)) => self
                .threads
                .get(thread)
                .and_then(|thread| thread.entries.get(entry))
                .map(|entry| entry.history.as_slice())
                .unwrap_or(&[]),
            None => &[],
        }
    }

    /// Replaces the active comment's text, keeping the old text as
    /// history. An empty text changes nothing — deletion has its own path.
    pub fn edit_active(&mut self, text: &str) {
        let text = trimmed(text);

        let Some((thread, entry)) = self.active else {
            return;
        };

        if text.is_empty() {
            return;
        }

        let Some(entry) = self
            .threads
            .get_mut(thread)
            .and_then(|thread| thread.entries.get_mut(entry))
        else {
            return;
        };

        if entry.text != text {
            entry.history.push(entry.text.clone());
            entry.text = text.to_owned();
        }
    }

    /// Deletes the comment at `(thread, entry)`. A thread whose last
    /// comment was deleted disappears; the active comment dies with it.
    /// Deletes the comment at `(thread, entry)`. A thread whose last
    /// comment was deleted disappears; the active comment dies with its
    /// entry and later indices shift down — inside the thread when an
    /// earlier reply went, across threads when a whole thread went.
    pub fn delete(&mut self, thread: usize, entry: usize) {
        let thread_gone = {
            let Some(target) = self.threads.get_mut(thread) else {
                return;
            };

            if entry >= target.entries.len() {
                return;
            }

            target.entries.remove(entry);
            target.entries.is_empty()
        };

        if thread_gone {
            self.threads.remove(thread);
        }

        // Fix the active index to keep naming the same comment — or none,
        // when that comment was the deleted one.
        self.active = self.active.and_then(|(active_thread, active_entry)| {
            if active_thread != thread {
                // A different thread: it only shifts when the whole thread
                // went and it sat after the removed one.
                if thread_gone && active_thread > thread {
                    Some((active_thread - 1, active_entry))
                } else {
                    Some((active_thread, active_entry))
                }
            } else if active_entry == entry || thread_gone {
                // The active comment itself was deleted — with its thread
                // or out of it.
                None
            } else if active_entry > entry {
                // An earlier reply went: the active one shifts down.
                Some((thread, active_entry - 1))
            } else {
                Some((thread, active_entry))
            }
        });
    }

    /// Resolves or reopens the thread at `index`. Resolved threads keep
    /// their comments and history but stop marking elements and drop out
    /// of cycling.
    pub fn resolve(&mut self, thread: usize) {
        let Some(target) = self.threads.get_mut(thread) else {
            return;
        };

        target.resolved = !target.resolved;
    }

    /// Whether the thread at `index` is resolved.
    #[allow(dead_code)] // exercised by the tests; the sidebar reads the cards' flag
    pub fn is_resolved(&self, thread: usize) -> bool {
        self.threads
            .get(thread)
            .is_some_and(|thread| thread.resolved)
    }

    /// The comments as sidebar cards, oldest first: open threads, then
    /// resolved ones — the history section. Replies come after their
    /// thread's root, indented by their depth.
    pub fn cards(&self, source: &str, elements: &[PreviewElement]) -> Vec<CommentCard> {
        let mut cards = Vec::new();

        for resolved in [false, true] {
            for (index, thread) in self.threads.iter().enumerate() {
                if thread.resolved != resolved {
                    continue;
                }

                for (entry, comment) in thread.entries.iter().enumerate() {
                    let (quote, label) = match thread.anchor {
                        Anchor::Caret(position) => (
                            quote(source, elements, position.element, COMMENT_QUOTE_MAX_CHARS),
                            None,
                        ),
                        Anchor::Selection(span) => {
                            (span_quote(elements, span, COMMENT_QUOTE_MAX_CHARS), None)
                        }
                        Anchor::Global => (String::new(), Some("Global")),
                    };

                    cards.push(CommentCard {
                        thread: index,
                        entry,
                        text: condensed(&comment.text, COMMENT_TEXT_MAX_CHARS),
                        quote,
                        label,
                        active: self.active == Some((index, entry)),
                        first: entry == 0,
                        resolved: thread.resolved,
                        depth: entry,
                        history: comment.history.len(),
                    });
                }
            }
        }

        cards
    }

    /// The publish draft, for the sidebar text field.
    pub fn draft(&self) -> &text_editor::Content {
        &self.draft
    }

    pub fn edit_draft(&mut self, action: text_editor::Action) {
        self.draft.perform(action);
    }
}

impl Default for Comments {
    fn default() -> Self {
        Self::new()
    }
}

/// The grapheme slice of `element` a span covers: partial at the span's
/// endpoints, whole elements in between — the same slicing the visual
/// selection paints.
fn span_slice(span: Span, element: usize, len: usize) -> Option<std::ops::Range<usize>> {
    let (lo, hi) = (span.start, span.end);

    if element < lo.element || element > hi.element {
        return None;
    }

    let start = if element == lo.element {
        lo.column.min(len)
    } else {
        0
    };
    let end = if element == hi.element {
        hi.column.min(len)
    } else {
        len
    };

    (end > start).then_some(start..end)
}

/// Returns the Markdown source of the preview element at `index`,
/// condensed for a comment card.
fn quote(source: &str, elements: &[PreviewElement], index: usize, max_chars: usize) -> String {
    let Some(element) = elements.get(index) else {
        return String::new();
    };

    condensed(source[element.source()].trim(), max_chars)
}

/// Returns the selected text a span covers, condensed for a comment
/// card: the endpoint elements contribute exactly their selected
/// characters (the selection's columns are columns of the rendered text),
/// the elements between contribute all of theirs. Elements contributing
/// nothing — like one the span only touches at column 0 — contribute no
/// quote either.
fn span_quote(elements: &[PreviewElement], span: Span, max_chars: usize) -> String {
    let mut joined = String::new();

    for index in span.start.element..=span.end.element {
        let Some(element) = elements.get(index) else {
            continue;
        };

        let start = if index == span.start.element {
            span.start.column
        } else {
            0
        };
        let end = if index == span.end.element {
            span.end.column
        } else {
            usize::MAX
        };

        let text = slice_graphemes(element.text(), start, end);

        if !text.is_empty() {
            if !joined.is_empty() {
                joined.push(' ');
            }

            joined.push_str(&text);
        }
    }

    condensed(&joined, max_chars)
}

/// The substring of `text` between grapheme columns `start` and `end`,
/// clamped to the text's length.
fn slice_graphemes(text: &str, start: usize, end: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;

    let total = text.graphemes(true).count();
    let start = start.min(total);
    let end = end.min(total).max(start);

    text.graphemes(true).skip(start).take(end - start).collect()
}

/// Trims a comment's text.
fn trimmed(text: &str) -> &str {
    text.trim()
}

/// Condenses text for a comment card: collapses it to one line and cuts it
/// off after `max_chars` characters with an ellipsis.
fn condensed(text: &str, max_chars: usize) -> String {
    let mut collapsed = String::with_capacity(text.len());

    for word in text.split_whitespace() {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }

        collapsed.push_str(word);
    }

    if collapsed.chars().count() > max_chars {
        let prefix: String = collapsed.chars().take(max_chars).collect();
        format!("{prefix}…")
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::{CaretPosition, Comments, Mark, Outline, Span};
    use crate::preview::ElementMap;

    fn at(element: usize) -> CaretPosition {
        CaretPosition { element, column: 0 }
    }

    fn pos(element: usize, column: usize) -> CaretPosition {
        CaretPosition { element, column }
    }

    fn span(a: CaretPosition, b: CaretPosition) -> Span {
        Span::new(a, b)
    }

    /// `Ctrl+N` activates the first thread, then walks forward, wrapping
    /// around at the end. Without comments there is nothing to activate.
    #[test]
    fn cycle_walks_threads_and_wraps() {
        let mut comments = Comments::new();
        assert_eq!(comments.cycle(), None);

        comments.save("one", at(1));
        comments.save("two", at(3));
        comments.save("three", at(5));

        // Saving activates the freshest thread; cycling moves past it.
        assert_eq!(comments.cycle(), Some(at(1)));
        assert_eq!(comments.cycle(), Some(at(3)));
        assert_eq!(comments.cycle(), Some(at(5)));
        assert_eq!(comments.cycle(), Some(at(1)));
    }

    /// The active mark follows the active thread's anchor, not its index:
    /// with the second thread active, its element is marked active and the
    /// first thread's element is merely commented.
    #[test]
    fn active_mark_follows_the_anchor_not_the_comment_index() {
        let mut comments = Comments::new();
        comments.save("first", at(1));
        comments.save("second", at(5));

        // The freshly saved comment is active.
        assert_eq!(comments.mark_for(5, 32), Mark::Active);
        assert_eq!(comments.mark_for(1, 32), Mark::Commented);
        assert_eq!(comments.mark_for(2, 32), Mark::None);

        // After cycling, the first thread is active — and its element, not
        // element 0, carries the active mark.
        comments.cycle();
        assert_eq!(comments.mark_for(1, 32), Mark::Active);
        assert_eq!(comments.mark_for(5, 32), Mark::Commented);
    }

    /// Saving trims the note and discards empty ones.
    #[test]
    fn save_trims_and_discards_empty_notes() {
        let mut comments = Comments::new();

        comments.save("   ", at(0));
        assert!(comments.is_empty());

        comments.save("  fix this  \n", at(2));
        assert_eq!(comments.len(), 1);
        assert_eq!(comments.cards("", &[])[0].text, "fix this");
    }

    /// Cards quote the anchored element's source, collapsed to one line and
    /// cut off with an ellipsis; unknown elements quote nothing.
    #[test]
    fn cards_quote_the_anchored_source() {
        let markdown = "# Some rather long heading text here\n\nshort";
        let elements = ElementMap::parse(markdown);

        let mut comments = Comments::new();
        comments.save("note", at(1));
        comments.save("another", at(9));

        let cards = comments.cards(markdown, elements.elements());
        assert_eq!(cards[0].quote, "short");
        assert_eq!(cards[1].quote, "");

        // Sources longer than the quote limit are cut off with an ellipsis.
        let long = format!("# {}\n\nbody", "a".repeat(80));
        let elements = ElementMap::parse(&long);

        let mut comments = Comments::new();
        comments.save("note", at(0));

        let cards = comments.cards(&long, elements.elements());
        assert_eq!(cards[0].quote, format!("# {}…", "a".repeat(58)));
    }

    /// Cards condense the comment text like the quote — one line, cut off
    /// with an ellipsis — so every card keeps the same bounded height.
    #[test]
    fn cards_trim_comment_text_to_a_uniform_length() {
        let mut comments = Comments::new();
        comments.save("short", at(0));
        comments.save(
            &format!("  {}  \n{}", "wordy ".repeat(40), "more\nlines"),
            at(1),
        );

        let cards = comments.cards("", &[]);

        assert_eq!(cards[0].text, "short");
        assert_eq!(cards[0].text.chars().count(), 5);
        // Collapsed to one line and cut off at the limit with an ellipsis.
        assert!(cards[1].text.ends_with('…'));
        assert_eq!(cards[1].text.chars().count(), 101);
        assert!(!cards[1].text.contains('\n'));
    }

    /// Global comments carry the "Global" label and no quote, never mark a
    /// preview element, and are skipped by cycling — there is no position
    /// to jump to.
    #[test]
    fn global_comments_are_unanchored() {
        let mut comments = Comments::new();
        comments.save("anchored", at(2));
        comments.save_global(" overall note ");

        let cards = comments.cards("", &[]);
        assert_eq!(cards[1].label, Some("Global"));
        assert_eq!(cards[1].quote, "");
        assert_eq!(cards[1].text, "overall note");
        assert_eq!(cards[0].label, None);

        // Saving the global comment made it active, but no element carries
        // a mark for it.
        assert_eq!(comments.mark_for(2, 32), Mark::Commented);

        // Cycling skips the global thread and jumps straight to the
        // anchored one, every time.
        assert_eq!(comments.cycle(), Some(at(2)));
        assert_eq!(comments.cycle(), Some(at(2)));

        // With only global comments there is nothing to cycle to.
        let mut globals = Comments::new();
        globals.save_global("one");
        assert_eq!(globals.cycle(), None);
    }

    /// Clicking a card makes its comment the active one and yields its
    /// anchor so the caret can jump there; global comments activate too
    /// but have nowhere to jump to, and unknown indices change nothing.
    #[test]
    fn activating_a_comment_returns_its_anchor() {
        let mut comments = Comments::new();
        comments.save("first", at(1));
        comments.save("second", at(5));
        comments.save_global("overall");

        let cards = comments.cards("", &[]);
        assert_eq!((cards[0].thread, cards[0].entry), (0, 0));
        assert_eq!(cards[2].label, Some("Global"));

        assert_eq!(comments.activate(1, 0), Some(at(5)));
        assert_eq!(comments.mark_for(5, 32), Mark::Active);
        let cards = comments.cards("", &[]);
        assert!(!cards[0].active);
        assert!(cards[1].active);

        // A global comment activates but yields no position.
        assert_eq!(comments.activate(2, 0), None);
        assert!(comments.cards("", &[])[2].active);

        // Unknown indices change nothing.
        assert_eq!(comments.activate(9, 0), None);
        assert!(comments.cards("", &[])[2].active);
        assert_eq!(comments.activate(0, 9), None);
    }

    /// The publish draft becomes a global comment and clears; empty drafts
    /// are discarded.
    #[test]
    fn add_draft_as_global() {
        let mut comments = Comments::new();

        comments.edit_draft(iced::widget::text_editor::Action::Edit(
            iced::widget::text_editor::Edit::Paste(std::sync::Arc::new("  ship it  \n".to_owned())),
        ));
        comments.add_draft_as_global();

        assert_eq!(comments.len(), 1);
        assert_eq!(comments.draft().text(), "");
        assert_eq!(comments.cards("", &[])[0].label, Some("Global"));
        assert_eq!(comments.cards("", &[])[0].text, "ship it");

        comments.add_draft_as_global();
        assert_eq!(comments.len(), 1);
    }

    /// Saving a note where an open thread already lives replies to it: one
    /// thread, two comments, the reply indented under the root in the
    /// sidebar. Saving elsewhere starts a new thread; the active thread
    /// takes the reply when it covers the position.
    #[test]
    fn notes_on_the_same_anchor_grow_a_thread() {
        let mut comments = Comments::new();
        comments.save("root", at(2));
        assert_eq!(comments.len(), 1);

        // Another note on the same element replies to the thread.
        comments.save("reply", at(2));
        assert_eq!(comments.len(), 2);

        let cards = comments.cards("", &[]);
        assert_eq!(cards.len(), 2);
        assert_eq!((cards[0].thread, cards[0].entry), (0, 0));
        assert_eq!((cards[1].thread, cards[1].entry), (0, 1));
        assert!(cards[0].first);
        assert!(!cards[1].first);
        assert_eq!(cards[1].depth, 1);

        // A note elsewhere starts a new thread.
        comments.save("elsewhere", at(6));
        assert_eq!(comments.len(), 3);
        let cards = comments.cards("", &[]);
        assert_eq!(cards[2].thread, 1);
        assert_eq!(cards[2].depth, 0);

        // With a different thread active, a note on element 2 still
        // replies to that thread — the anchor decides, not the cursor.
        comments.activate(1, 0);
        comments.save("second reply", at(2));
        let cards = comments.cards("", &[]);
        assert_eq!(cards.len(), 4);
        // The reply lands in thread 0 — its third entry — so the cards
        // read thread 0's three comments, then thread 1's one.
        assert_eq!((cards[2].thread, cards[2].entry), (0, 2));
        assert_eq!((cards[3].thread, cards[3].entry), (1, 0));
    }

    /// A comment saved over a visual selection anchors to exactly that
    /// span: it marks the covered elements, the sidebar quotes the selected
    /// source, and further notes over the same span reply to the thread.
    /// A note at a spot covered by the span replies to it too.
    #[test]
    fn selection_comments_anchor_to_the_span() {
        let markdown = "alpha\n\nbeta\n\ngamma";
        let elements = ElementMap::parse(markdown);
        let elements = elements.elements();

        let mut comments = Comments::new();
        comments.save_selection("about this", span(pos(1, 1), pos(2, 2)));

        // The covered elements carry the mark.
        assert_eq!(comments.mark_for(0, 32), Mark::None);
        assert_eq!(comments.mark_for(1, 32), Mark::Active);
        assert_eq!(comments.mark_for(2, 32), Mark::Active);
        assert_eq!(comments.mark_for(3, 32), Mark::None);

        // The card quotes exactly the selected text: column 1 of "beta"
        // through column 2 of "gamma".
        let cards = comments.cards(markdown, elements);
        assert_eq!(cards[0].quote, "eta ga");

        // Cycling jumps to the span's start.
        assert_eq!(comments.cycle(), Some(pos(1, 1)));

        // The same selection again replies to the thread.
        comments.save_selection("reply", span(pos(1, 1), pos(2, 2)));
        assert_eq!(comments.len(), 2);
        let cards = comments.cards(markdown, elements);
        assert_eq!(cards[1].depth, 1);

        // A different span over the same elements starts a new thread.
        comments.save_selection("other span", span(pos(1, 0), pos(2, 2)));
        assert_eq!(comments.len(), 3);
        let cards = comments.cards(markdown, elements);
        assert_eq!(cards[2].thread, 1);

        // A note at a spot inside the span replies to the covering thread.
        comments.save("inside", pos(2, 1));
        let cards = comments.cards(markdown, elements);
        assert_eq!((cards[3].thread, cards[3].entry), (1, 1));
    }

    /// The active selection anchor reports its slice per element, like the
    /// visual selection paints: partial at the endpoints, whole in
    /// between, none outside — and only while it is the active thread.
    #[test]
    fn selection_anchors_report_their_slices() {
        let elements = crate::preview::ElementMap::parse("aaaa\n\nbbbb\n\ncccc\n\ndddd")
            .elements()
            .to_vec();
        let lens = |element: usize| elements[element].len();

        let mut comments = Comments::new();
        comments.save_selection("sel", span(pos(1, 2), pos(3, 1)));

        assert_eq!(comments.anchor_selection_for(1, lens(1)), Some(2..4));
        assert_eq!(comments.anchor_selection_for(2, lens(2)), Some(0..4));
        assert_eq!(comments.anchor_selection_for(3, lens(3)), Some(0..1));
        assert_eq!(comments.anchor_selection_for(0, lens(0)), None);
        assert_eq!(comments.anchor_selection_for(9, 4), None);

        // Another thread active: no slice is reported.
        comments.save("plain", at(5));
        assert_eq!(comments.anchor_selection_for(1, lens(1)), None);

        // Resolving the thread also removes the slice.
        comments.activate(0, 0);
        comments.resolve(0);
        assert_eq!(comments.anchor_selection_for(1, lens(1)), None);
    }

    /// Editing the active comment replaces its text and keeps the old text
    /// as history; empty edits change nothing.
    #[test]
    fn editing_keeps_history() {
        let mut comments = Comments::new();
        comments.save("first take", at(1));

        assert_eq!(comments.active_text(), Some("first take"));
        assert!(comments.active_history().is_empty());

        comments.edit_active("  second take  ");
        assert_eq!(comments.active_text(), Some("second take"));
        assert_eq!(comments.active_history(), ["first take"]);
        assert_eq!(comments.len(), 1);

        // Editing to the same text changes nothing.
        comments.edit_active("second take");
        assert_eq!(comments.active_history().len(), 1);

        // An empty edit changes nothing.
        comments.edit_active("   ");
        assert_eq!(comments.active_text(), Some("second take"));

        // The history shows on the card.
        let cards = comments.cards("", &[]);
        assert_eq!(cards[0].history, 1);
    }

    /// Deleting a comment removes it from its thread; a thread without
    /// comments disappears, and indices after it shift down while the
    /// active comment stays on its own thread.
    #[test]
    fn deleting_comments_and_threads() {
        let mut comments = Comments::new();
        comments.save("root", at(1));
        comments.save("reply", at(1));
        comments.save("other", at(4));

        // Delete the reply: the thread keeps its root.
        comments.delete(0, 1);
        assert_eq!(comments.len(), 2);
        let cards = comments.cards("", &[]);
        assert_eq!(cards.len(), 2);
        assert_eq!((cards[0].thread, cards[0].entry), (0, 0));

        // Delete the root: the thread disappears and the later thread
        // shifts down.
        comments.delete(0, 0);
        assert_eq!(comments.len(), 1);
        let cards = comments.cards("", &[]);
        assert_eq!(cards[0].text, "other");
        assert_eq!((cards[0].thread, cards[0].entry), (0, 0));

        // Deleting the active comment deactivates it.
        assert_eq!(comments.active_text(), Some("other"));
        comments.delete(0, 0);
        assert_eq!(comments.active_text(), None);
        assert!(comments.is_empty());

        // Unknown indices change nothing.
        comments.delete(9, 9);
    }

    /// Deleting a reply before the active one shifts the active index
    /// down — the same comment stays active. Deleting the active reply
    /// itself clears the active comment instead of silently renaming
    /// another one.
    #[test]
    fn deleting_keeps_the_active_comment_pointing_at_the_same_comment() {
        let mut comments = Comments::new();
        comments.save("root", at(1));
        comments.save("reply one", at(1));
        comments.save("reply two", at(1));

        comments.activate(0, 2);
        assert_eq!(comments.active_text(), Some("reply two"));

        // Delete the earlier reply: the active one shifts to entry 1.
        comments.delete(0, 1);
        assert_eq!(comments.active_text(), Some("reply two"));
        let cards = comments.cards("", &[]);
        assert!(cards[1].active);

        // Delete the active reply itself: nothing stays active.
        comments.delete(0, 1);
        assert_eq!(comments.active_text(), None);

        // Deleting an earlier whole thread shifts the active thread down.
        comments.save("other", at(4));
        comments.save("anchor thread", at(6));
        comments.activate(2, 0);
        comments.delete(0, 0);
        assert_eq!(comments.active_text(), Some("anchor thread"));
        let cards = comments.cards("", &[]);
        assert_eq!((cards[1].thread, cards[1].entry), (1, 0));
        assert!(cards[1].active);
    }

    /// The outlines framing commented text follow the anchors: selections
    /// frame exactly their slice, spot anchors frame their whole element,
    /// the active thread frames last (on top), and resolved threads frame
    /// nothing — while the reveal query follows the active thread even
    /// into history.
    #[test]
    fn outlines_frame_anchors_and_resolved_threads_frame_nothing() {
        let elements = crate::preview::ElementMap::parse("aaaa\n\nbbbb\n\ncccc")
            .elements()
            .to_vec();
        let lens = |element: usize| elements[element].len();

        let mut comments = Comments::new();
        comments.save_selection("first", span(pos(1, 1), pos(1, 3)));
        comments.save("spot", at(2));

        // The freshest thread is active: its outline comes last.
        assert_eq!(
            comments.outlines_for(1, lens(1)),
            vec![Outline {
                range: 1..3,
                mark: Mark::Commented,
            },]
        );
        assert_eq!(
            comments.outlines_for(2, lens(2)),
            vec![Outline {
                range: 0..4,
                mark: Mark::Active,
            }]
        );
        assert!(comments.anchors_element(2, lens(2)));
        assert!(!comments.anchors_element(1, lens(1)));

        // Activating the selection thread puts its outline on top.
        comments.activate(0, 0);
        assert_eq!(
            comments.outlines_for(1, lens(1)),
            vec![Outline {
                range: 1..3,
                mark: Mark::Active,
            }]
        );
        assert_eq!(
            comments.outlines_for(2, lens(2)),
            vec![Outline {
                range: 0..4,
                mark: Mark::Commented,
            }]
        );

        // Resolved threads frame nothing, but the reveal query still finds
        // the active thread's element — history stays reachable.
        comments.resolve(0);
        assert!(comments.outlines_for(1, lens(1)).is_empty());
        assert!(comments.anchors_element(1, lens(1)));

        // Elements without comments frame nothing.
        assert!(comments.outlines_for(0, lens(0)).is_empty());
        assert!(!comments.anchors_element(0, lens(0)));
    }

    /// A selection touching an element only at its column 0 — an
    /// exclusive end, like the caret sitting at the next element's start —
    /// neither marks that element nor quotes it: the anchor is exactly
    /// the selected text.
    #[test]
    fn zero_width_endpoints_mark_and_quote_nothing() {
        let markdown = "alpha\n\nbeta\n\ngamma";
        let elements = ElementMap::parse(markdown);
        let elements = elements.elements();

        let mut comments = Comments::new();
        // From column 2 of "beta" to the start of "gamma": the selection
        // ends exactly where "gamma" begins, so "gamma" contributes
        // nothing.
        comments.save_selection("touch", span(pos(1, 2), pos(2, 0)));

        // Only "beta" carries a mark — "gamma" contributes no characters.
        assert_eq!(comments.mark_for(1, 4), Mark::Active);
        assert_eq!(comments.mark_for(2, 5), Mark::None);

        // The quote is the tail of "beta" and no "gamma" at all.
        let cards = comments.cards(markdown, elements);
        assert_eq!(cards[0].quote, "ta");

        // A same-element zero-width span marks nothing at all.
        let mut empty = Comments::new();
        empty.save_selection("empty", span(pos(0, 2), pos(0, 2)));
        assert_eq!(empty.mark_for(0, 5), Mark::None);
    }

    /// The span quote slices the endpoint elements by their columns: the
    /// selected characters only, elements between contributing all of
    /// theirs.
    #[test]
    fn span_quotes_slice_the_endpoints() {
        let markdown = "aaaa\n\nbbbb\n\ncccc";
        let elements = ElementMap::parse(markdown);
        let elements = elements.elements();

        let mut comments = Comments::new();
        comments.save_selection("sel", span(pos(0, 1), pos(2, 3)));

        let cards = comments.cards(markdown, elements);
        assert_eq!(cards[0].quote, "aaa bbbb ccc");
    }

    /// Resolving a thread moves it to history: it stops marking elements,
    /// cycling skips it, and further notes on its anchor start a fresh
    /// thread. Reopening restores everything.
    #[test]
    fn resolving_moves_threads_to_history() {
        let mut comments = Comments::new();
        comments.save("done deal", at(2));
        comments.save("live", at(4));

        comments.resolve(0);
        assert!(comments.is_resolved(0));

        // Resolved threads mark nothing and cycle skips them.
        assert_eq!(comments.mark_for(2, 32), Mark::None);
        assert_eq!(comments.cycle(), Some(at(4)));
        assert_eq!(comments.cycle(), Some(at(4)));

        // The resolved card sits below the open one, flagged as history.
        let cards = comments.cards("", &[]);
        assert_eq!(cards[0].text, "live");
        assert!(!cards[0].resolved);
        assert_eq!(cards[1].text, "done deal");
        assert!(cards[1].resolved);

        // A note on the resolved thread's anchor starts a new thread.
        comments.save("fresh", at(2));
        let cards = comments.cards("", &[]);
        assert_eq!(cards.len(), 3);
        assert_eq!((cards[1].thread, cards[1].depth), (2, 0));

        // Reopening restores the marks — element 2 carries the fresh
        // thread's Active mark (it is active) and the reopened one's
        // Commented underneath.
        comments.resolve(0);
        assert!(!comments.is_resolved(0));
        assert_eq!(comments.mark_for(2, 32), Mark::Active);

        // Without a thread active on it, the reopened one shows Commented.
        comments.activate(1, 0);
        assert_eq!(comments.mark_for(2, 32), Mark::Commented);
    }
}
