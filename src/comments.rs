//! The comments feature: notes anchored to preview elements, the active
//! comment, and the publish draft.
//!
//! The anchor representation is private to this module — it crosses the seam
//! only as a [`CaretPosition`]. Element indices rot when the preview
//! re-parses after an edit; when anchors move to a stable representation
//! (e.g. source offsets), only this module changes.

use iced::widget::text_editor;

use crate::PreviewElement;

/// How many characters of an element's Markdown source a comment card
/// quotes before cutting it off.
const COMMENT_QUOTE_MAX_CHARS: usize = 60;

/// A caret position in the preview: an element index plus a grapheme column
/// within the element's rendered text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaretPosition {
    pub element: usize,
    pub column: usize,
}

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
pub struct CommentCard<'a> {
    pub text: &'a str,
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
    pub fn cards<'a>(&'a self, source: &str, elements: &[PreviewElement]) -> Vec<CommentCard<'a>> {
        self.comments
            .iter()
            .enumerate()
            .map(|(index, comment)| CommentCard {
                text: &comment.text,
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

/// Returns the Markdown source of the preview element at `index`, trimmed
/// for a comment card: collapsed to one line and cut off after `max_chars`
/// characters with an ellipsis.
fn quote(source: &str, elements: &[PreviewElement], index: usize, max_chars: usize) -> String {
    let Some(element) = elements.get(index) else {
        return String::new();
    };

    let source = source[element.source.clone()].trim();
    let mut collapsed = String::with_capacity(source.len());

    for word in source.split_whitespace() {
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
    use crate::preview_elements;

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
        let elements = preview_elements(markdown);

        let mut comments = Comments::new();
        comments.save("note", at(1));
        comments.save("another", at(9));

        let cards = comments.cards(markdown, &elements);
        assert_eq!(cards[0].quote, "short");
        assert_eq!(cards[1].quote, "");

        // Sources longer than the quote limit are cut off with an ellipsis.
        let long = format!("# {}\n\nbody", "a".repeat(80));
        let elements = preview_elements(&long);

        let mut comments = Comments::new();
        comments.save("note", at(0));

        let cards = comments.cards(&long, &elements);
        assert_eq!(cards[0].quote, format!("# {}…", "a".repeat(58)));
    }
}
