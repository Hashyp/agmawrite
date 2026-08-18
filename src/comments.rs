//! The comments feature: notes anchored to preview elements, the active
//! comment, and the publish draft.
//!
//! The anchor representation is private to this module — it crosses the seam
//! only as a [`CaretPosition`]. Element indices rot when the preview
//! re-parses after an edit; when anchors move to a stable representation
//! (e.g. source offsets), only this module changes.

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

/// A saved comment: the note text plus the anchor it was written for.
#[derive(Debug, Clone)]
struct Comment {
    text: String,
    anchor: CaretPosition,
}

/// A comment as the sidebar renders it: the note text, a quote of the
/// anchored element's source, and whether it is the active comment.
pub struct CommentCard {
    /// The note text, condensed like the quote: collapsed to one line and
    /// cut off, so every card occupies the same vertical space.
    pub text: String,
    pub quote: String,
    pub active: bool,
}

/// The comments store: saved comments (oldest first), the active one, and
/// the publish draft.
pub struct Comments {
    comments: Vec<Comment>,
    active: Option<usize>,
    draft: text_editor::Content,
}

impl Comments {
    pub fn new() -> Self {
        Self {
            comments: Vec::new(),
            active: None,
            draft: text_editor::Content::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }

    pub fn len(&self) -> usize {
        self.comments.len()
    }

    /// Saves a comment anchored at `at` and makes it the active one. Empty
    /// notes are discarded.
    pub fn save(&mut self, text: &str, at: CaretPosition) {
        let text = text.trim();

        if text.is_empty() {
            return;
        }

        self.active = Some(self.comments.len());
        self.comments.push(Comment {
            text: text.to_owned(),
            anchor: at,
        });
    }

    /// Cycles the active comment forward, wrapping around at the end, and
    /// returns its anchor so the caret can jump to the mark. With no
    /// comments there is nothing to activate.
    pub fn cycle(&mut self) -> Option<CaretPosition> {
        if self.comments.is_empty() {
            return None;
        }

        let index = match self.active {
            None => 0,
            Some(index) => (index + 1) % self.comments.len(),
        };
        self.active = Some(index);

        Some(self.comments[index].anchor)
    }

    /// The mark a preview element carries.
    pub fn mark_for(&self, element: usize) -> Mark {
        if self
            .active
            .is_some_and(|index| self.comments[index].anchor.element == element)
        {
            return Mark::Active;
        }

        if self
            .comments
            .iter()
            .any(|comment| comment.anchor.element == element)
        {
            Mark::Commented
        } else {
            Mark::None
        }
    }

    /// The comments as sidebar cards, oldest first.
    pub fn cards(&self, source: &str, elements: &[PreviewElement]) -> Vec<CommentCard> {
        self.comments
            .iter()
            .enumerate()
            .map(|(index, comment)| CommentCard {
                text: condensed(&comment.text, COMMENT_TEXT_MAX_CHARS),
                quote: quote(
                    source,
                    elements,
                    comment.anchor.element,
                    COMMENT_QUOTE_MAX_CHARS,
                ),
                active: self.active == Some(index),
            })
            .collect()
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

/// Returns the Markdown source of the preview element at `index`,
/// condensed for a comment card.
fn quote(source: &str, elements: &[PreviewElement], index: usize, max_chars: usize) -> String {
    let Some(element) = elements.get(index) else {
        return String::new();
    };

    condensed(source[element.source()].trim(), max_chars)
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
    use super::{CaretPosition, Comments, Mark};
    use crate::preview::ElementMap;

    fn at(element: usize) -> CaretPosition {
        CaretPosition { element, column: 0 }
    }

    /// `Ctrl+N` activates the first comment, then walks forward, wrapping
    /// around at the end. Without comments there is nothing to activate.
    #[test]
    fn cycle_walks_comments_and_wraps() {
        let mut comments = Comments::new();
        assert_eq!(comments.cycle(), None);

        comments.save("one", at(1));
        comments.save("two", at(3));
        comments.save("three", at(5));

        // Saving activates the freshest comment; cycling moves past it.
        assert_eq!(comments.cycle(), Some(at(1)));
        assert_eq!(comments.cycle(), Some(at(3)));
        assert_eq!(comments.cycle(), Some(at(5)));
        assert_eq!(comments.cycle(), Some(at(1)));
    }

    /// The active mark follows the active comment's anchor, not its index:
    /// with the second comment active, its element is marked active and the
    /// first comment's element is merely commented.
    #[test]
    fn active_mark_follows_the_anchor_not_the_comment_index() {
        let mut comments = Comments::new();
        comments.save("first", at(1));
        comments.save("second", at(5));

        // The freshly saved comment is active.
        assert_eq!(comments.mark_for(5), Mark::Active);
        assert_eq!(comments.mark_for(1), Mark::Commented);
        assert_eq!(comments.mark_for(2), Mark::None);

        // After cycling, the first comment is active — and its element, not
        // element 0, carries the active mark.
        comments.cycle();
        assert_eq!(comments.mark_for(1), Mark::Active);
        assert_eq!(comments.mark_for(5), Mark::Commented);
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
}
