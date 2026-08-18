//! The preview pane: the parsed element list and the caret that moves
//! through it.
//!
//! [`Caret`] owns the caret state — the element, the grapheme column, and
//! the sticky target column `j`/`k` aim for — behind a small motion
//! interface, so the sticky-column invariant holds by construction rather
//! than by convention at every call site. All motion runs against the
//! parsed [`PreviewElement`] list.

use unicode_segmentation::UnicodeSegmentation;

use std::cell::Cell;

use iced::widget::text_editor;

/// A caret position in the preview: an element index plus a grapheme column
/// within the element's rendered text.
///
/// The ordering is lexicographic — by element, then by column — so two
/// positions compare like document positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CaretPosition {
    pub element: usize,
    pub column: usize,
}

/// A caret motion in the preview, usable with the arrow keys or the vim keys
/// `h`, `j`, `k`, and `l`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Up,
    Down,
    Left,
    Right,
}

/// A word motion in the preview, like the vim keys `w`, `b`, `e`, and `ge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordMotion {
    /// `w` — forward to the start of the next word.
    NextStart,
    /// `b` — backward to the start of the previous word.
    PreviousStart,
    /// `e` — forward to the end of the next word.
    NextEnd,
    /// `ge` — backward to the end of the previous word.
    PreviousEnd,
}

/// A document jump in the preview, like the vim keys `gg` and `G`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// `gg` — to the first element.
    First,
    /// `G` — to the last element.
    Last,
}

/// The kind of a preview element, used for grid-style navigation in tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    /// A heading, paragraph, quote, or list item.
    Text,
    /// A non-empty table cell.
    Cell { row: usize, column: usize },
}

/// A source range paired with the kind of element the preview renders it as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewElement {
    source: std::ops::Range<usize>,
    kind: ElementKind,
    /// The rendered text of the element — markdown markup like `**bold**`
    /// is not part of it.
    text: String,
    /// The number of graphemes in `text`.
    len: usize,
}

impl PreviewElement {
    /// The element's Markdown source range.
    pub fn source(&self) -> std::ops::Range<usize> {
        self.source.clone()
    }

    /// The number of graphemes in the rendered text.
    pub fn len(&self) -> usize {
        self.len
    }

    /// The rendered text of the element, without markdown markup.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// The parsed preview elements, owned as one list with one numbering.
///
/// The map owns both sides of the numbering seam: the `pulldown-cmark`
/// walk that mirrors how `iced` itemizes Markdown, and the [`Claims`]
/// protocol the Markdown viewer uses to fetch its element per rendered
/// item — the viewer claims, it never counts. The parity invariant between
/// the two lives here and nowhere else.
#[derive(Debug, Default)]
pub struct ElementMap {
    elements: Vec<PreviewElement>,
}

impl ElementMap {
    /// Parses the Markdown source into the numbered element list.
    pub fn parse(markdown: &str) -> Self {
        Self {
            elements: parse(markdown),
        }
    }

    /// The numbered elements, in document order — the list all caret
    /// motion runs against.
    pub fn elements(&self) -> &[PreviewElement] {
        &self.elements
    }

    /// Starts a claim run for the viewer: each [`Claims::claim`] hands out
    /// the next numbered element in document order, mirroring how `iced`
    /// numbers Markdown items.
    pub fn claims(&self) -> Claims<'_> {
        Claims {
            map: self,
            next: Cell::new(0),
        }
    }
}

/// A claim run over an [`ElementMap`]: the viewer claims the next numbered
/// element per item it renders, so the numbering lives in the map rather
/// than in a counter the viewer carries.
pub struct Claims<'a> {
    map: &'a ElementMap,
    next: Cell<usize>,
}

impl<'a> Claims<'a> {
    /// Claims the next numbered element, returning its index and the
    /// element itself. `None` once every element has been claimed.
    pub fn claim(&self) -> Option<(usize, &'a PreviewElement)> {
        let index = self.next.get();
        let element = self.map.elements.get(index)?;
        self.next.set(index + 1);
        Some((index, element))
    }
}

/// The preview caret: the element it sits on, the grapheme column within
/// it, and the sticky target column `j`/`k` aim for, like vim.
///
/// The three pieces move together through the motion methods; the sticky
/// column survives vertical moves and clamps to the destination element by
/// construction.
#[derive(Debug, Clone, Copy, Default)]
pub struct Caret {
    element: usize,
    column: usize,
    column_target: usize,
}

impl Caret {
    pub fn new() -> Self {
        Self::default()
    }

    /// The caret as a typed position: the element and grapheme column.
    pub fn position(&self) -> CaretPosition {
        CaretPosition {
            element: self.element,
            column: self.column,
        }
    }

    /// Places the caret at `position`, aiming the sticky column at its
    /// column — a fresh placement forgets the old target, like a click.
    pub fn place(&mut self, position: CaretPosition) {
        self.element = position.element;
        self.column = position.column;
        self.column_target = position.column;
    }

    /// Applies a caret [`Motion`] (`h`/`j`/`k`/`l`), returning whether the
    /// caret moved.
    ///
    /// `h`/`l` move the caret one grapheme within the current element,
    /// crossing to the end of the previous element or the start of the next
    /// one at the edges, like moving along wrapped lines. `j`/`k` move
    /// between elements (rows of the same column in tables) and aim for the
    /// sticky target column, clamped to the destination element like vim.
    pub fn move_by(&mut self, elements: &[PreviewElement], motion: Motion) -> bool {
        let Some(len) = elements.get(self.element).map(PreviewElement::len) else {
            return false;
        };

        let next = (|| match motion {
            Motion::Left => {
                if self.column > 0 {
                    Some((self.element, self.column - 1, self.column - 1))
                } else {
                    let previous = self.element.checked_sub(1)?;
                    let len = elements[previous].len;
                    Some((previous, len, len))
                }
            }
            Motion::Right => {
                if self.column < len {
                    Some((self.element, self.column + 1, self.column + 1))
                } else {
                    let next = self
                        .element
                        .checked_add(1)
                        .filter(|next| *next < elements.len())?;
                    Some((next, 0, 0))
                }
            }
            Motion::Up | Motion::Down => {
                let next = element_after(elements, self.element, motion)?;
                let column = self.column_target.min(elements[next].len);
                Some((next, column, self.column_target))
            }
        })();

        let Some((element, column, column_target)) = next else {
            return false;
        };

        self.element = element;
        self.column = column;
        self.column_target = column_target;
        true
    }

    /// Applies a [`WordMotion`] (`w`, `b`, `e`, `ge`), returning whether the
    /// caret moved.
    ///
    /// The caret column counts grapheme boundaries, and the caret is a bar
    /// drawn between characters: `w`/`b` place it at the start of a word,
    /// while `e`/`ge` place it just past the last character of a word. Word
    /// motions cross element boundaries like vim crosses lines: `w`/`e`
    /// continue in the next element, `b`/`ge` in the previous one. The
    /// sticky column follows the destination.
    pub fn move_word(&mut self, elements: &[PreviewElement], motion: WordMotion) -> bool {
        let Some(element) = elements.get(self.element) else {
            return false;
        };

        let (starts, ends) = word_columns(&element.text);

        // End motions aim just past the word's last character, since the
        // caret is a bar drawn between characters — unlike vim's block
        // cursor, which sits on the last character itself.
        let end_column = |end: usize| end + 1;

        let column = self.column;
        let in_element = match motion {
            WordMotion::NextStart => starts.iter().copied().find(|&start| start > column),
            WordMotion::NextEnd => ends
                .iter()
                .copied()
                .map(end_column)
                .find(|&end| end > column),
            WordMotion::PreviousStart => starts.iter().copied().rev().find(|&start| start < column),
            WordMotion::PreviousEnd => ends
                .iter()
                .copied()
                .rev()
                .map(end_column)
                .find(|&end| end < column),
        };

        let destination = if let Some(column) = in_element {
            (self.element, column)
        } else {
            // Cross to the neighbouring element, like crossing a line in
            // vim.
            let (neighbor, columns) = match motion {
                WordMotion::NextStart | WordMotion::NextEnd => {
                    let next = self
                        .element
                        .checked_add(1)
                        .filter(|&next| next < elements.len());
                    match next {
                        Some(next) => (next, word_columns(&elements[next].text)),
                        None => return false,
                    }
                }
                WordMotion::PreviousStart | WordMotion::PreviousEnd => {
                    match self.element.checked_sub(1) {
                        Some(previous) => (previous, word_columns(&elements[previous].text)),
                        None => return false,
                    }
                }
            };

            let (starts, ends) = columns;

            let column = match motion {
                WordMotion::NextStart => starts.first().copied().unwrap_or(0),
                WordMotion::NextEnd => ends.first().copied().map_or(0, end_column),
                WordMotion::PreviousStart => starts.last().copied().unwrap_or(0),
                WordMotion::PreviousEnd => ends.last().copied().map_or(0, end_column),
            };

            (neighbor, column)
        };

        self.element = destination.0;
        self.column = destination.1;
        self.column_target = destination.1;
        true
    }

    /// Jumps to the first (`gg`) or last (`G`) element, keeping the sticky
    /// target column like vim, clamped to the destination element.
    pub fn jump(&mut self, elements: &[PreviewElement], jump: Jump) -> bool {
        let index = match jump {
            Jump::First => {
                if elements.is_empty() {
                    return false;
                }
                0
            }
            Jump::Last => match elements.len().checked_sub(1) {
                Some(last) => last,
                None => return false,
            },
        };

        self.element = index;
        self.column = self.column_target.min(elements[index].len);
        true
    }

    /// Places the caret on the element containing the source editor's
    /// cursor, at column 0 — used when switching from write mode into the
    /// preview.
    pub fn move_to_source_cursor(
        &mut self,
        content: &text_editor::Content,
        elements: &[PreviewElement],
    ) {
        self.element = element_for_source_cursor(content, elements);
        self.column = 0;
        self.column_target = 0;
    }
}

/// Returns the Markdown elements the preview numbers as caret positions, in
/// order, each paired with its source range and kind.
///
/// This mirrors how `iced`'s Markdown parser turns `pulldown-cmark` events
/// into items: only headings and paragraphs become caret elements, and a
/// paragraph item is only produced while there is pending inline text. Lists,
/// quotes, tables, code blocks, images, and rules are containers or unnumbered
/// items — but their contents still produce numbered elements.
fn parse(markdown: &str) -> Vec<PreviewElement> {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

    let options = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;

    let mut elements = Vec::new();
    // Mirrors iced's `spans` buffer: some inline text is pending since the
    // last produced item.
    let mut pending_text = false;
    // The rendered text of the pending item, mirroring how iced builds spans
    // (soft breaks become spaces, hard breaks newlines).
    let mut rendered = String::new();
    let mut metadata = false;
    let mut code_block = false;
    // End of the last recorded element; pending text always lies after it.
    let mut last_end = 0;
    // Grid position of the current table cell; only meaningful inside a
    // table, where the header row is row `0`.
    let mut cell_row = 0;
    let mut cell_column = 0;

    // Records the pending rendered text as a finished caret element.
    let finish = |source: std::ops::Range<usize>,
                  kind: ElementKind,
                  elements: &mut Vec<PreviewElement>,
                  rendered: &mut String,
                  last_end: &mut usize| {
        let text = rendered.clone();
        let len = rendered.graphemes(true).count();
        rendered.clear();
        *last_end = source.end;
        elements.push(PreviewElement {
            source,
            kind,
            text,
            len,
        });
    };

    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        match event {
            Event::Text(text) | Event::Code(text) if !metadata && !code_block => {
                pending_text = true;
                rendered.push_str(text.as_ref());
            }
            Event::SoftBreak if !metadata && !code_block => {
                pending_text = true;
                rendered.push(' ');
            }
            Event::HardBreak if !metadata && !code_block => {
                pending_text = true;
                rendered.push('\n');
            }
            Event::Start(Tag::MetadataBlock(_)) => metadata = true,
            Event::End(TagEnd::MetadataBlock(_)) => metadata = false,
            // A new table starts with its header row.
            Event::Start(Tag::TableHead) => {
                cell_row = 0;
                cell_column = 0;
            }
            Event::Start(Tag::TableRow) => {
                cell_row += 1;
                cell_column = 0;
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) if !metadata => {
                code_block = true;

                // A code block interrupts a paragraph in progress and flushes
                // it as its own element first.
                if pending_text {
                    finish(
                        last_end.min(range.start)..range.start,
                        ElementKind::Text,
                        &mut elements,
                        &mut rendered,
                        &mut last_end,
                    );
                    pending_text = false;
                }
            }
            Event::Text(text) | Event::Code(text) if code_block && !metadata => {
                // The code block's lines, collected as its rendered text —
                // each event ends with a newline, trimmed when the block
                // closes.
                rendered.push_str(text.as_ref());
            }
            Event::End(TagEnd::CodeBlock) if !metadata => {
                code_block = false;

                // A fenced code block is itself a caret element: the caret
                // moves through its code and comments can anchor on it. Its
                // source spans the fences; its text is the code between
                // them, without the trailing newline.
                rendered.pop();
                finish(
                    range,
                    ElementKind::Text,
                    &mut elements,
                    &mut rendered,
                    &mut last_end,
                );
            }
            // Lists and quotes likewise interrupt a paragraph in progress.
            // The pending text lies between the previous element and the
            // interrupting block, so record that span.
            Event::Start(Tag::List(_) | Tag::BlockQuote(_)) if !metadata && pending_text => {
                finish(
                    last_end.min(range.start)..range.start,
                    ElementKind::Text,
                    &mut elements,
                    &mut rendered,
                    &mut last_end,
                );
                pending_text = false;
            }
            // Images drain the pending text into an unnumbered item.
            Event::End(TagEnd::Image) => {
                pending_text = false;
                rendered.clear();
            }
            Event::End(TagEnd::Heading(_)) if !metadata => {
                finish(
                    range,
                    ElementKind::Text,
                    &mut elements,
                    &mut rendered,
                    &mut last_end,
                );
                pending_text = false;
            }
            Event::End(TagEnd::TableCell) if !metadata => {
                if pending_text {
                    finish(
                        range,
                        ElementKind::Cell {
                            row: cell_row,
                            column: cell_column,
                        },
                        &mut elements,
                        &mut rendered,
                        &mut last_end,
                    );
                    pending_text = false;
                }

                // Empty cells render no element but still occupy a column.
                cell_column += 1;
            }
            Event::End(TagEnd::Paragraph | TagEnd::Item) if !metadata && pending_text => {
                finish(
                    range,
                    ElementKind::Text,
                    &mut elements,
                    &mut rendered,
                    &mut last_end,
                );
                pending_text = false;
            }
            _ => {}
        }
    }

    elements
}

/// The index of the preview element containing the source editor's cursor,
/// or the nearest element before it.
fn element_for_source_cursor(content: &text_editor::Content, elements: &[PreviewElement]) -> usize {
    if elements.is_empty() {
        return 0;
    }

    let cursor = content.cursor().position;
    let mut offset = 0;

    for line_index in 0..cursor.line {
        if let Some(line) = content.line(line_index) {
            offset += line.text.len() + line.ending.as_str().len();
        }
    }

    if let Some(line) = content.line(cursor.line) {
        offset += line
            .text
            .char_indices()
            .nth(cursor.column)
            .map_or(line.text.len(), |(index, _)| index);
    }

    elements
        .iter()
        .position(|element| element.source.contains(&offset))
        .unwrap_or_else(|| {
            elements
                .iter()
                .rposition(|element| element.source.start <= offset)
                .unwrap_or(0)
        })
}

/// Resolves a vertical caret [`Motion`] (`j`/`k`) from the element at
/// `current` to the index of the next element, if the motion leads anywhere.
///
/// The preview list is flat, so outside tables the motion is simply the
/// previous or next element. Inside a table, it moves between rows of the
/// same column (exiting the table at its edges) — like navigating a grid
/// in vim.
fn element_after(elements: &[PreviewElement], current: usize, motion: Motion) -> Option<usize> {
    let last = elements.len().checked_sub(1)?;
    let element = elements.get(current)?;

    match element.kind {
        ElementKind::Text => match motion {
            Motion::Up => current.checked_sub(1),
            Motion::Down => (current < last).then_some(current + 1),
            Motion::Left | Motion::Right => None,
        },
        ElementKind::Cell { row, column } => match motion {
            Motion::Down => elements[current + 1..]
                .iter()
                .position(|element| match element.kind {
                    // The first element past the table exits below it.
                    ElementKind::Text => true,
                    ElementKind::Cell { row: r, column: c } => r == row + 1 && c == column,
                })
                .map(|offset| current + 1 + offset),
            Motion::Up => elements[..current]
                .iter()
                .rposition(|element| match element.kind {
                    // The last element before the table exits above it.
                    ElementKind::Text => true,
                    ElementKind::Cell { row: r, column: c } => r + 1 == row && c == column,
                }),
            Motion::Left | Motion::Right => None,
        },
    }
}

/// The word class of a grapheme, like vim's notion of words: alphanumeric
/// characters and `_` form words, whitespace separates them, and any other
/// punctuation forms words of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Word,
    Punct,
    Space,
}

fn char_class(grapheme: &str) -> CharClass {
    let Some(character) = grapheme.chars().next() else {
        return CharClass::Space;
    };

    if character.is_alphanumeric() || character == '_' {
        CharClass::Word
    } else if character.is_whitespace() {
        CharClass::Space
    } else {
        CharClass::Punct
    }
}

/// Returns the grapheme columns where words start and end in the rendered
/// text of an element. A word is a maximal run of word characters or of
/// punctuation, like vim's `w`/`b`/`e`.
fn word_columns(text: &str) -> (Vec<usize>, Vec<usize>) {
    let classes: Vec<CharClass> = text.graphemes(true).map(char_class).collect();

    let mut starts = Vec::new();
    let mut ends = Vec::new();

    for (i, &class) in classes.iter().enumerate() {
        if class == CharClass::Space {
            continue;
        }

        let previous = i.checked_sub(1).map(|p| classes[p]);
        let next = classes.get(i + 1).copied();

        let at_word_start = previous.is_none_or(|p| p == CharClass::Space || p != class);
        let at_word_end = next.is_none_or(|n| n == CharClass::Space || n != class);

        if at_word_start {
            starts.push(i);
        }

        if at_word_end {
            ends.push(i);
        }
    }

    (starts, ends)
}

/// Computes the selected grapheme range of a single preview element for a
/// visual-mode selection between the `anchor` and `caret` positions: the
/// endpoints select partial elements, everything between them fully.
pub fn element_selection(
    anchor: CaretPosition,
    caret: CaretPosition,
    index: usize,
    len: usize,
) -> Option<std::ops::Range<usize>> {
    let (lo, hi) = if anchor <= caret {
        (anchor, caret)
    } else {
        (caret, anchor)
    };

    if index < lo.element || index > hi.element {
        return None;
    }

    let start = if index == lo.element {
        lo.column.min(len)
    } else {
        0
    };
    let end = if index == hi.element {
        hi.column.min(len)
    } else {
        len
    };

    (end > start).then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::{
        element_selection, parse, Caret, CaretPosition, ElementKind, ElementMap, Jump, Motion,
        WordMotion,
    };

    fn at(element: usize, column: usize) -> CaretPosition {
        CaretPosition { element, column }
    }

    /// Claiming hands out every element in document order, then stops —
    /// the viewer's numbering is the map's numbering.
    #[test]
    fn claims_walk_the_elements_in_order() {
        let map = ElementMap::parse("# Title\n\nbody\n\nmore");
        let claims = map.claims();

        for index in 0..map.elements().len() {
            let (claimed, element) = claims.claim().expect("element not claimed");
            assert_eq!(claimed, index);
            assert!(std::ptr::eq(element, &map.elements()[index]));
        }

        assert!(claims.claim().is_none());
        assert!(map.claims().claim().is_some_and(|(index, _)| index == 0));
    }

    /// The preview caret stops early when the parsed elements disagree with
    /// the number of elements the Markdown viewer actually numbers (list
    /// items are `viewer.paragraph` calls too). This pins the parity, and
    /// the grid position of table cells.
    #[test]
    fn elements_match_preview_viewer_numbering() {
        let markdown = "\
# Title

Intro paragraph.

> quote

| A | B |
|---|---|
| 1 | 2 |

- one
- two
  - nested

1. first

- [ ] todo
- [x] done

## Tail

```rust
fn main() {}
```
";

        let elements = parse(markdown);
        let texts: Vec<&str> = elements
            .iter()
            .map(|element| &markdown[element.source()])
            .collect();

        // Headings, paragraphs, quotes, table cells, every list item
        // (tight, ordered, task, and nested), and fenced code blocks.
        assert_eq!(texts.len(), 15);
        for (text, expected) in texts.iter().zip([
            "Title", "Intro", "quote", "A", "B", "1", "2", "one", "two", "nested", "first", "todo",
            "done", "Tail", "```rust",
        ]) {
            assert!(text.contains(expected), "expected '{expected}' in '{text}'");
        }

        // Table cells carry their grid position: A B / 1 2.
        let kinds: Vec<ElementKind> = elements.iter().map(|element| element.kind).collect();
        assert_eq!(
            kinds[3..7],
            [
                ElementKind::Cell { row: 0, column: 0 },
                ElementKind::Cell { row: 0, column: 1 },
                ElementKind::Cell { row: 1, column: 0 },
                ElementKind::Cell { row: 1, column: 1 },
            ]
        );
    }

    /// Fenced code blocks are caret elements like any other: the source
    /// spans the fences, the text is the code between them (without the
    /// trailing newline), and `j`/`k` walk into and out of them.
    #[test]
    fn fenced_code_blocks_are_elements() {
        let markdown = "before\n\n```rust\nfn main() {}\n```\n\nafter";
        let elements = parse(markdown);

        assert_eq!(elements.len(), 3);
        assert_eq!(&markdown[elements[1].source()], "```rust\nfn main() {}\n```");
        assert_eq!(elements[1].text(), "fn main() {}");
        assert_eq!(elements[1].len(), 12);

        // The caret walks the code like any element, character by
        // character, and `j`/`k` cross its edges.
        let mut caret = Caret::new();
        caret.place(at(0, 0));
        assert!(caret.move_by(&elements, Motion::Down));
        assert_eq!(caret.position(), at(1, 0));
        assert!(caret.move_by(&elements, Motion::Right));
        assert_eq!(caret.position(), at(1, 1));
        assert!(caret.move_by(&elements, Motion::Down));
        assert_eq!(caret.position(), at(2, 1));

        // Word motions treat the code's words like any other element's.
        caret.place(at(1, 0));
        assert!(caret.move_word(&elements, WordMotion::NextStart));
        assert_eq!(caret.position(), at(1, 3)); // → main
        assert!(caret.move_word(&elements, WordMotion::NextStart));
        assert_eq!(caret.position(), at(1, 7)); // → (
    }

    /// `j`/`k` navigate tables as a grid: they move between rows of the same
    /// column and exit the table at its edges.
    #[test]
    fn vim_motions_navigate_tables_by_grid() {
        let markdown = "\
intro

| A | B |
|---|---|
| 1 | 2 |

outro
";

        // intro, A, B, 1, 2, outro
        let elements = parse(markdown);
        let mut caret = Caret::new();
        let mut navigate = |from, motion| {
            caret.place(at(from, 0));
            caret
                .move_by(&elements, motion)
                .then(|| caret.position().element)
        };

        // Down descends a column and exits the table below it.
        assert_eq!(navigate(0, Motion::Down), Some(1)); // intro → A
        assert_eq!(navigate(1, Motion::Down), Some(3)); // A → 1
        assert_eq!(navigate(3, Motion::Down), Some(5)); // 1 → outro
        assert_eq!(navigate(5, Motion::Down), None); // end of document

        // Up climbs the column and exits the table above it.
        assert_eq!(navigate(4, Motion::Up), Some(2)); // 2 → B
        assert_eq!(navigate(2, Motion::Up), Some(0)); // B → intro
        assert_eq!(navigate(0, Motion::Up), None); // start of document
    }

    /// `h`/`l` move the caret one character at a time; at the edges of an
    /// element they cross to the end of the previous element or the start of
    /// the next one, like moving along wrapped lines.
    #[test]
    fn h_and_l_move_one_character() {
        // Two text elements: "aa bb" and "cc dd".
        let elements = parse("aa bb\n\ncc dd");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].len(), 5);
        assert_eq!(elements[1].len(), 5);

        let mut caret = Caret::new();
        let mut motion = |from: CaretPosition, motion| {
            caret.place(from);
            caret.move_by(&elements, motion).then(|| caret.position())
        };

        // `l` walks the element one character at a time.
        assert_eq!(motion(at(0, 0), Motion::Right), Some(at(0, 1)));
        assert_eq!(motion(at(0, 4), Motion::Right), Some(at(0, 5)));
        // At the end of the element, `l` crosses to the next one.
        assert_eq!(motion(at(0, 5), Motion::Right), Some(at(1, 0)));
        assert_eq!(motion(at(1, 5), Motion::Right), None); // end of document

        // `h` mirrors it, crossing to the end of the previous element.
        assert_eq!(motion(at(1, 0), Motion::Left), Some(at(0, 5)));
        assert_eq!(motion(at(0, 1), Motion::Left), Some(at(0, 0)));
        assert_eq!(motion(at(0, 0), Motion::Left), None); // start of document
    }

    /// `j`/`k` keep the vim sticky column: the target column survives moves
    /// and clamps to the destination element's length.
    #[test]
    fn j_and_k_keep_the_sticky_column() {
        // "aaaa", "bb", "ccccc"
        let elements = parse("aaaa\n\nbb\n\nccccc");
        assert_eq!(elements.len(), 3);

        let mut caret = Caret::new();
        caret.place(at(0, 3));

        // The column clamps when moving to a shorter element...
        assert!(caret.move_by(&elements, Motion::Down));
        assert_eq!(caret.position(), at(1, 2));
        // ...and returns when moving on to a longer one.
        assert!(caret.move_by(&elements, Motion::Down));
        assert_eq!(caret.position(), at(2, 3));
        assert!(caret.move_by(&elements, Motion::Up));
        assert_eq!(caret.position(), at(1, 2));

        // In tables the sticky column applies to the destination cell.
        let markdown = "\
| Feature | Editor |
|---|---|
| Headings | ✅ |
";
        let cells = parse(markdown);
        // Feature → Headings in the same table column.
        caret.place(at(0, 5));
        assert!(caret.move_by(&cells, Motion::Down));
        assert_eq!(caret.position(), at(2, 5));
        // ✅ is one grapheme, so the column clamps to 1.
        caret.place(at(1, 3));
        assert!(caret.move_by(&cells, Motion::Down));
        assert_eq!(caret.position(), at(3, 1));
    }

    /// `w`/`b` jump between word starts and `e`/`ge` between word ends,
    /// treating punctuation runs as words like vim; at element edges they
    /// cross to the neighbouring element like crossing a line.
    #[test]
    fn w_b_e_and_ge_move_by_words() {
        // "one two, three" — words: one, two, ",", three.
        let elements = parse("one two, three\n\nnext");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].text, "one two, three");

        let mut caret = Caret::new();
        let mut word = |from: CaretPosition, motion| {
            caret.place(from);
            caret.move_word(&elements, motion).then(|| caret.position())
        };

        // w walks the word starts, comma included.
        assert_eq!(word(at(0, 0), WordMotion::NextStart), Some(at(0, 4))); // → two
        assert_eq!(word(at(0, 4), WordMotion::NextStart), Some(at(0, 7))); // → ,
        assert_eq!(word(at(0, 7), WordMotion::NextStart), Some(at(0, 9))); // → three
        assert_eq!(word(at(0, 9), WordMotion::NextStart), Some(at(1, 0))); // → next
        assert_eq!(word(at(1, 4), WordMotion::NextStart), None); // end of document

        // b walks them backwards.
        assert_eq!(word(at(0, 9), WordMotion::PreviousStart), Some(at(0, 7)));
        assert_eq!(word(at(0, 4), WordMotion::PreviousStart), Some(at(0, 0)));
        assert_eq!(word(at(0, 0), WordMotion::PreviousStart), None); // start of document

        // e places the caret just past the last character of each word.
        assert_eq!(word(at(0, 0), WordMotion::NextEnd), Some(at(0, 3))); // one|
        assert_eq!(word(at(0, 3), WordMotion::NextEnd), Some(at(0, 7))); // two|
        assert_eq!(word(at(0, 7), WordMotion::NextEnd), Some(at(0, 8))); // ,|
        assert_eq!(word(at(0, 8), WordMotion::NextEnd), Some(at(0, 14))); // three|
        assert_eq!(word(at(0, 14), WordMotion::NextEnd), Some(at(1, 4))); // next| (crosses)
        assert_eq!(word(at(1, 4), WordMotion::NextEnd), None); // end of document

        // ge places it just past the last character of the previous word,
        // crossing elements at the start.
        assert_eq!(word(at(0, 9), WordMotion::PreviousEnd), Some(at(0, 8)));
        assert_eq!(word(at(0, 4), WordMotion::PreviousEnd), Some(at(0, 3)));
        assert_eq!(word(at(0, 0), WordMotion::PreviousEnd), None);
        assert_eq!(word(at(1, 0), WordMotion::PreviousEnd), Some(at(0, 14)));
    }

    /// `gg` and `G` jump to the first and last element, keeping vim's sticky
    /// column clamped to the destination element.
    #[test]
    fn gg_and_g_jump_between_document_ends() {
        // "aaaa", "b", "ccccc"
        let elements = parse("aaaa\n\nb\n\nccccc");
        assert_eq!(elements.len(), 3);

        let mut caret = Caret::new();
        caret.place(at(0, 3));

        assert!(caret.jump(&elements, Jump::First));
        assert_eq!(caret.position(), at(0, 3));
        assert!(caret.jump(&elements, Jump::Last));
        assert_eq!(caret.position(), at(2, 3));

        // The sticky column clamps to shorter elements.
        caret.place(at(1, 9));
        assert!(caret.jump(&elements, Jump::Last));
        assert_eq!(caret.position(), at(2, 5));
        assert!(caret.jump(&elements, Jump::First));
        assert_eq!(caret.position(), at(0, 4));

        let mut caret = Caret::new();
        assert!(!caret.jump(&[], Jump::First));
        assert!(!caret.jump(&[], Jump::Last));
    }

    /// Visual mode anchors one end and the caret forms the other; motions
    /// between them select partial elements at the ends and full elements
    /// in between, regardless of direction.
    #[test]
    fn visual_selection_spans_elements() {
        // "aaaa", "bb", "cc" — element lengths 4, 2, 2.
        let selection = |anchor, caret, index, len| element_selection(anchor, caret, index, len);

        // Within one element: `v` then `l` three times selects 3 chars.
        assert_eq!(selection(at(0, 1), at(0, 4), 0, 4), Some(1..4));
        assert_eq!(selection(at(0, 1), at(0, 4), 1, 2), None);

        // Across elements: the start selects to the end of its element, the
        // middle is full, the destination selects up to the caret column.
        assert_eq!(selection(at(0, 1), at(2, 1), 0, 4), Some(1..4));
        assert_eq!(selection(at(0, 1), at(2, 1), 1, 2), Some(0..2));
        assert_eq!(selection(at(0, 1), at(2, 1), 2, 2), Some(0..1));

        // Direction does not matter.
        assert_eq!(selection(at(2, 1), at(0, 1), 1, 2), Some(0..2));

        // Columns beyond an element's length clamp away.
        assert_eq!(selection(at(1, 9), at(1, 10), 1, 2), None);
        assert_eq!(selection(at(0, 0), at(0, 0), 0, 4), None);
    }
}
