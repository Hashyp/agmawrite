mod interactive_text;

use std::cell::Cell;
use std::collections::HashSet;

use unicode_segmentation::UnicodeSegmentation;

use iced::advanced::widget::operation::{Outcome, Scrollable};
use iced::advanced::widget::Operation;
use iced::widget::{
    button, canvas, column, container, markdown, mouse_area, operation::focus,
    operation::focus_next, operation::scroll_by, operation::AbsoluteOffset, row, scrollable, stack,
    text, text_editor, tooltip, Id, Space,
};
use iced::{
    alignment, application, keyboard, mouse, Background, Border, Color, Element, Font, Length,
    Point, Rectangle, Renderer, Subscription, Task, Theme, Vector,
};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
const PREVIEW_SCROLL_ID: &str = "preview-scroll";
const PREVIEW_CARET_ID: &str = "preview-caret";
const NOTE_EDITOR_ID: &str = "note-editor";
/// How many characters of an element's Markdown source a comment card
/// quotes before cutting it off.
const COMMENT_QUOTE_MAX_CHARS: usize = 60;
/// Margin kept between the preview caret and the viewport edges while scrolling.
const CARET_MARGIN: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WordSelection {
    paragraph: usize,
    word: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ParagraphSelection(usize);

/// A saved comment: the note text plus the anchor it was written for, as a
/// caret `(element, column)` position.
#[derive(Debug, Clone)]
struct Comment {
    text: String,
    anchor: (usize, usize),
}

/// A caret motion in the preview, usable with the arrow keys or the vim keys
/// `h`, `j`, `k`, and `l`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Up,
    Down,
    Left,
    Right,
}

/// A word motion in the preview, like the vim keys `w`, `b`, `e`, and `ge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordMotion {
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
enum Jump {
    /// `gg` — to the first element.
    First,
    /// `G` — to the last element.
    Last,
}

/// The kind of a preview element, used for grid-style navigation in tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementKind {
    /// A heading, paragraph, quote, or list item.
    Text,
    /// A non-empty table cell.
    Cell { row: usize, column: usize },
}

/// A source range paired with the kind of element the preview renders it as.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewElement {
    source: std::ops::Range<usize>,
    kind: ElementKind,
    /// The rendered text of the element — markdown markup like `**bold**`
    /// is not part of it.
    text: String,
    /// The number of graphemes in `text`.
    len: usize,
}

struct Editor {
    content: text_editor::Content,
    markdown: markdown::Content,
    preview: bool,
    preview_only: bool,
    selected_words: HashSet<WordSelection>,
    selected_paragraphs: HashSet<ParagraphSelection>,
    preview_cursor: usize,
    /// The grapheme column of the caret within the current element.
    preview_column: usize,
    /// The column `j`/`k` aim for, like vim's sticky column.
    preview_column_target: usize,
    preview_elements: Vec<PreviewElement>,
    /// Whether a lone `g` is awaiting its second key of a `gg`/`ge` sequence.
    pending_g: bool,
    /// The fixed end of the visual-mode selection, as an (element, column)
    /// caret position. `None` outside visual mode.
    visual_anchor: Option<(usize, usize)>,
    /// Whether the note popup is open over the preview.
    note_open: bool,
    /// The text of the note popup.
    note_text: text_editor::Content,
    /// Saved comments, oldest first.
    comments: Vec<Comment>,
    /// The text of the sidebar publish field.
    publish_text: text_editor::Content,
}

#[derive(Debug, Clone)]
enum Message {
    Edit(text_editor::Action),
    OpenFile,
    FileLoaded(Option<String>),
    TogglePreview,
    LinkClicked(markdown::Uri),
    SelectWord(WordSelection),
    SelectParagraph(ParagraphSelection),
    MovePreviewCursor(Motion),
    MovePreviewWord(WordMotion),
    MovePreviewJump(Jump),
    PreviewGPressed,
    PreviewCancel,
    ToggleVisualMode,
    ScrollPreviewBy(f32),
    OpenNotePopup,
    CloseNotePopup,
    EditNote(text_editor::Action),
    SaveNote,
    NoteCardPressed,
    EditPublish(text_editor::Action),
    PublishPressed,
}

struct OpenFileIcon;

impl<Message> canvas::Program<Message> for OpenFileIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let folder = canvas::Path::new(|path| {
            path.move_to(Point::new(2.5, 13.0));
            path.line_to(Point::new(2.5, 3.5));
            path.line_to(Point::new(6.5, 3.5));
            path.line_to(Point::new(8.5, 5.5));
            path.line_to(Point::new(13.5, 5.5));
            path.line_to(Point::new(13.5, 13.0));
            path.close();
        });

        frame.stroke(
            &folder,
            canvas::Stroke::default()
                .with_color(Color::from_rgb(0.65, 0.65, 0.65))
                .with_width(1.4)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round),
        );

        vec![frame.into_geometry()]
    }
}

struct PreviewIcon;

impl<Message> canvas::Program<Message> for PreviewIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let eye = canvas::Path::new(|path| {
            path.move_to(Point::new(1.5, 8.0));
            path.quadratic_curve_to(Point::new(8.0, 1.0), Point::new(14.5, 8.0));
            path.quadratic_curve_to(Point::new(8.0, 15.0), Point::new(1.5, 8.0));
            path.close();
        });

        frame.stroke(
            &eye,
            canvas::Stroke::default()
                .with_color(Color::from_rgb(0.65, 0.65, 0.65))
                .with_width(1.4)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round),
        );

        let pupil = canvas::Path::circle(Point::new(8.0, 8.0), 2.8);
        frame.fill(&pupil, Color::from_rgb(0.65, 0.65, 0.65));

        vec![frame.into_geometry()]
    }
}

async fn open_file() -> Option<String> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Open Markdown file")
        .pick_file()
        .await?;

    Some(String::from_utf8_lossy(&file.read().await).into_owned())
}

fn handle_key_press(
    event: keyboard::Event,
    preview: bool,
    preview_only: bool,
    pending_g: bool,
    note_open: bool,
) -> Option<Message> {
    let keyboard::Event::KeyPressed {
        modified_key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };

    // The note popup swallows plain keys for its text area; only Escape
    // closes it and Ctrl+S saves the comment.
    if preview && note_open && !modifiers.alt() && !modifiers.logo() {
        return match modified_key.as_ref() {
            keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                Some(Message::CloseNotePopup)
            }
            keyboard::Key::Character("s" | "S") if modifiers.control() && !repeat => {
                Some(Message::SaveNote)
            }
            _ => None,
        };
    }

    if preview && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
        // Auto-repeat is welcome for motions (holding `j`/`k`/`h`/`l`, `w`,
        // `b`, `e`, or `G` keeps moving), but one-shot actions must not
        // repeat.
        return match modified_key.as_ref() {
            keyboard::Key::Named(keyboard::key::Named::ArrowUp)
            | keyboard::Key::Character("k" | "K") => Some(Message::MovePreviewCursor(Motion::Up)),
            keyboard::Key::Named(keyboard::key::Named::ArrowDown)
            | keyboard::Key::Character("j" | "J") => Some(Message::MovePreviewCursor(Motion::Down)),
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
            | keyboard::Key::Character("h" | "H") => Some(Message::MovePreviewCursor(Motion::Left)),
            keyboard::Key::Named(keyboard::key::Named::ArrowRight)
            | keyboard::Key::Character("l" | "L") => {
                Some(Message::MovePreviewCursor(Motion::Right))
            }
            // `gg` — the second `g` of the sequence is always a fresh press.
            keyboard::Key::Character("g") if pending_g && !repeat => {
                Some(Message::MovePreviewJump(Jump::First))
            }
            // The first `g` of a `gg`/`ge` sequence.
            keyboard::Key::Character("g") if !repeat => Some(Message::PreviewGPressed),
            // `ge` — the `e` of the sequence is always a fresh press.
            keyboard::Key::Character("e" | "E") if pending_g && !repeat => {
                Some(Message::MovePreviewWord(WordMotion::PreviousEnd))
            }
            keyboard::Key::Character("e" | "E") => {
                Some(Message::MovePreviewWord(WordMotion::NextEnd))
            }
            keyboard::Key::Character("w" | "W") => {
                Some(Message::MovePreviewWord(WordMotion::NextStart))
            }
            keyboard::Key::Character("b" | "B") => {
                Some(Message::MovePreviewWord(WordMotion::PreviousStart))
            }
            keyboard::Key::Character("G") => Some(Message::MovePreviewJump(Jump::Last)),
            keyboard::Key::Character("v" | "V") if !repeat => Some(Message::ToggleVisualMode),
            keyboard::Key::Character("c" | "C") if !repeat => Some(Message::OpenNotePopup),
            keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                Some(Message::PreviewCancel)
            }
            _ => None,
        };
    }

    if modifiers.control() && !repeat {
        match modified_key.as_ref() {
            keyboard::Key::Character("o" | "O") => Some(Message::OpenFile),
            keyboard::Key::Character("p" | "P") if !preview_only => Some(Message::TogglePreview),
            _ => None,
        }
    } else {
        None
    }
}

fn subscription(editor: &Editor) -> Subscription<Message> {
    keyboard::listen()
        .with((
            editor.preview,
            editor.preview_only,
            editor.pending_g,
            editor.note_open,
        ))
        .filter_map(|((preview, preview_only, pending_g, note_open), event)| {
            handle_key_press(event, preview, preview_only, pending_g, note_open)
        })
}

fn update(editor: &mut Editor, message: Message) -> Task<Message> {
    // Any key other than a lone `g` ends a pending `gg`/`ge` sequence.
    if !matches!(message, Message::PreviewGPressed) {
        editor.pending_g = false;
    }

    match message {
        Message::Edit(action) => editor.content.perform(action),
        Message::OpenFile => return Task::perform(open_file(), Message::FileLoaded),
        Message::FileLoaded(Some(contents)) => {
            editor.preview_elements = preview_elements(&contents);
            editor.preview_cursor = 0;
            editor.preview_column = 0;
            editor.preview_column_target = 0;
            editor.note_open = false;
            editor.visual_anchor = None;
            editor.note_text = text_editor::Content::new();
            editor.comments.clear();
            editor.publish_text = text_editor::Content::new();
            editor.content = text_editor::Content::with_text(&contents);
            editor.markdown = markdown::Content::parse(&contents);
        }
        Message::FileLoaded(None) => {}
        Message::TogglePreview => {
            if !editor.preview_only {
                editor.preview = !editor.preview;
                editor.note_open = false;
                editor.visual_anchor = None;

                if editor.preview {
                    let contents = editor.content.text();
                    editor.preview_elements = preview_elements(&contents);
                    editor.preview_cursor =
                        element_for_source_cursor(&editor.content, &editor.preview_elements);
                    editor.preview_column = 0;
                    editor.preview_column_target = 0;
                    editor.markdown = markdown::Content::parse(&contents);

                    return reveal_preview_caret();
                }
            }
        }
        Message::LinkClicked(_uri) => {
            // TODO: open links in the default browser
        }
        Message::SelectWord(word) => {
            editor.selected_words.insert(word);
        }
        Message::SelectParagraph(paragraph) => {
            editor.selected_paragraphs.insert(paragraph);
        }
        Message::MovePreviewCursor(motion) => {
            if let Some((cursor, column, column_target)) = move_caret(
                &editor.preview_elements,
                editor.preview_cursor,
                editor.preview_column,
                editor.preview_column_target,
                motion,
            ) {
                editor.preview_cursor = cursor;
                editor.preview_column = column;
                editor.preview_column_target = column_target;

                // The caret always advances; the page only scrolls as much
                // as needed to keep the caret visible.
                return reveal_preview_caret();
            }
        }
        Message::MovePreviewWord(motion) => {
            if let Some((cursor, column)) = move_word(
                &editor.preview_elements,
                editor.preview_cursor,
                editor.preview_column,
                motion,
            ) {
                editor.preview_cursor = cursor;
                editor.preview_column = column;
                editor.preview_column_target = column;

                return reveal_preview_caret();
            }
        }
        Message::MovePreviewJump(jump) => {
            if let Some((cursor, column)) =
                jump_caret(&editor.preview_elements, editor.preview_column_target, jump)
            {
                editor.preview_cursor = cursor;
                editor.preview_column = column;

                return reveal_preview_caret();
            }
        }
        Message::PreviewGPressed => {
            editor.pending_g = true;
        }
        Message::PreviewCancel => {
            editor.visual_anchor = None;
        }
        Message::ToggleVisualMode => {
            editor.visual_anchor = if editor.visual_anchor.is_some() {
                None
            } else {
                Some((editor.preview_cursor, editor.preview_column))
            };
        }
        Message::ScrollPreviewBy(y) => {
            return scroll_by(Id::new(PREVIEW_SCROLL_ID), AbsoluteOffset { x: 0.0, y });
        }
        Message::OpenNotePopup => {
            if !editor.note_open {
                editor.note_open = true;

                return focus(Id::new(NOTE_EDITOR_ID));
            }
        }
        Message::CloseNotePopup => {
            editor.note_open = false;
        }
        Message::EditNote(action) => editor.note_text.perform(action),
        Message::EditPublish(action) => editor.publish_text.perform(action),
        Message::PublishPressed => {
            // TODO: publish the comments
        }
        Message::SaveNote => {
            if editor.note_open {
                save_note(editor);
            }
        }
        // Clicks on the card itself are swallowed so they neither close the
        // popup nor reach the preview beneath.
        Message::NoteCardPressed => {}
    }

    Task::none()
}

/// Returns the Markdown elements the preview numbers as caret positions, in
/// order, each paired with its source range and kind.
///
/// This mirrors how `iced`'s Markdown parser turns `pulldown-cmark` events
/// into items: only headings and paragraphs become caret elements, and a
/// paragraph item is only produced while there is pending inline text. Lists,
/// quotes, tables, code blocks, images, and rules are containers or unnumbered
/// items — but their contents still produce numbered elements.
fn preview_elements(markdown: &str) -> Vec<PreviewElement> {
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
            Event::End(TagEnd::CodeBlock) => code_block = false,
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

/// Resolves a caret [`Motion`] from the element at `current` to the index of
/// the next element, if the motion leads anywhere.
///
/// The preview list is flat, so outside tables every motion degenerates to
/// the previous or next element. Inside a table, `Up`/`Down` move between
/// rows of the same column (exiting the table at its edges) while
/// `Left`/`Right` move between the cells of a row — like navigating a grid
/// in vim.
/// Resolves a vertical caret [`Motion`] (`j`/`k`) from the element at
/// `current` to the index of the next element, if the motion leads anywhere.
///
/// The preview list is flat, so outside tables the motion is simply the
/// previous or next element. Inside a table, it moves between rows of the
/// same column (exiting the table at its edges) — like navigating a grid
/// in vim.
fn preview_element_after(
    elements: &[PreviewElement],
    current: usize,
    motion: Motion,
) -> Option<usize> {
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

/// Applies a caret [`Motion`] to the caret position and returns the new
/// element, column, and sticky target column — if the motion leads anywhere.
///
/// `h`/`l` move the caret one grapheme within the current element, crossing
/// to the end of the previous element or the start of the next one at the
/// edges, like moving along wrapped lines. `j`/`k` move between elements
/// (rows of the same column in tables) and aim for the sticky target column,
/// clamped to the destination element like vim.
fn move_caret(
    elements: &[PreviewElement],
    cursor: usize,
    column: usize,
    column_target: usize,
    motion: Motion,
) -> Option<(usize, usize, usize)> {
    let len = elements.get(cursor)?.len;

    match motion {
        Motion::Left => {
            if column > 0 {
                Some((cursor, column - 1, column - 1))
            } else {
                let previous = cursor.checked_sub(1)?;
                let len = elements[previous].len;
                Some((previous, len, len))
            }
        }
        Motion::Right => {
            if column < len {
                Some((cursor, column + 1, column + 1))
            } else {
                let next = cursor
                    .checked_add(1)
                    .filter(|next| *next < elements.len())?;
                Some((next, 0, 0))
            }
        }
        Motion::Up | Motion::Down => {
            let next = preview_element_after(elements, cursor, motion)?;
            let column = column_target.min(elements[next].len);
            Some((next, column, column_target))
        }
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

/// Applies a [`WordMotion`] (`w`, `b`, `e`, `ge`) to the caret position and
/// returns the new element and column — if the motion leads anywhere.
///
/// The caret column counts grapheme boundaries, and the caret is a bar
/// drawn between characters: `w`/`b` place it at the start of a word, while
/// `e`/`ge` place it just past the last character of a word. Word motions
/// cross element boundaries like vim crosses lines: `w`/`e` continue in the
/// next element, `b`/`ge` in the previous one.
fn move_word(
    elements: &[PreviewElement],
    cursor: usize,
    column: usize,
    motion: WordMotion,
) -> Option<(usize, usize)> {
    let element = elements.get(cursor)?;
    let (starts, ends) = word_columns(&element.text);

    // End motions aim just past the word's last character, since the caret
    // is a bar drawn between characters — unlike vim's block cursor, which
    // sits on the last character itself.
    let end_column = |end: usize| end + 1;

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

    if let Some(column) = in_element {
        return Some((cursor, column));
    }

    // Cross to the neighbouring element, like crossing a line in vim.
    let (neighbor, columns) = match motion {
        WordMotion::NextStart | WordMotion::NextEnd => {
            let next = cursor
                .checked_add(1)
                .filter(|&next| next < elements.len())?;
            (next, word_columns(&elements[next].text))
        }
        WordMotion::PreviousStart | WordMotion::PreviousEnd => {
            let previous = cursor.checked_sub(1)?;
            (previous, word_columns(&elements[previous].text))
        }
    };

    let (starts, ends) = columns;

    let column = match motion {
        WordMotion::NextStart => starts.first().copied().unwrap_or(0),
        WordMotion::NextEnd => ends.first().copied().map_or(0, end_column),
        WordMotion::PreviousStart => starts.last().copied().unwrap_or(0),
        WordMotion::PreviousEnd => ends.last().copied().map_or(0, end_column),
    };

    Some((neighbor, column))
}

/// Jumps to the first (`gg`) or last (`G`) element, keeping the sticky
/// target column like vim.
fn jump_caret(
    elements: &[PreviewElement],
    column_target: usize,
    jump: Jump,
) -> Option<(usize, usize)> {
    let index = match jump {
        Jump::First => elements.first().map(|_| 0)?,
        Jump::Last => elements.len().checked_sub(1)?,
    };

    let column = column_target.min(elements[index].len);

    Some((index, column))
}

/// Computes the selected grapheme range of a single preview element for a
/// visual-mode selection between the `anchor` and `caret` `(element,
/// column)` positions: the endpoints select partial elements, everything
/// between them fully.
fn element_selection(
    anchor: (usize, usize),
    caret: (usize, usize),
    index: usize,
    len: usize,
) -> Option<std::ops::Range<usize>> {
    let ((lo_element, lo_column), (hi_element, hi_column)) = if anchor <= caret {
        (anchor, caret)
    } else {
        (caret, anchor)
    };

    if index < lo_element || index > hi_element {
        return None;
    }

    let start = if index == lo_element {
        lo_column.min(len)
    } else {
        0
    };
    let end = if index == hi_element {
        hi_column.min(len)
    } else {
        len
    };

    (end > start).then_some(start..end)
}

/// Saves the note popup text as a comment anchored at the preview caret,
/// then closes the popup with a fresh note. Empty notes are discarded.
fn save_note(editor: &mut Editor) {
    let text = editor.note_text.text().trim().to_string();

    if !text.is_empty() {
        editor.comments.push(Comment {
            text,
            anchor: (editor.preview_cursor, editor.preview_column),
        });
    }

    editor.note_open = false;
    editor.note_text = text_editor::Content::new();
}

/// Returns the Markdown source of the preview element at `index`, trimmed
/// for display in the comments sidebar: collapsed to one line and cut off
/// after `max_chars` characters with an ellipsis.
fn trimmed_element_source(
    markdown: &str,
    elements: &[PreviewElement],
    index: usize,
    max_chars: usize,
) -> String {
    let Some(element) = elements.get(index) else {
        return String::new();
    };

    let source = markdown[element.source.clone()].trim();
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

/// Measures the preview scrollable and the caret element in the widget tree,
/// then scrolls the minimum amount needed to bring the caret back into view.
///
/// The page keeps its position once the caret is visible — unlike a
/// proportional scroll, the caret can never outrun the end of the page.
fn reveal_preview_caret() -> Task<Message> {
    iced::advanced::widget::operate(RevealCaret {
        scroll_id: Id::new(PREVIEW_SCROLL_ID),
        caret_id: Id::new(PREVIEW_CARET_ID),
        viewport: None,
        caret: None,
    })
}

struct RevealCaret {
    scroll_id: Id,
    caret_id: Id,
    viewport: Option<(Rectangle, Vector)>,
    caret: Option<Rectangle>,
}

impl Operation<Message> for RevealCaret {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if Some(&self.scroll_id) == id {
            self.viewport = Some((bounds, translation));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if Some(&self.caret_id) == id {
            self.caret = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        let Some((viewport, translation)) = self.viewport else {
            return Outcome::None;
        };
        let Some(caret) = self.caret else {
            return Outcome::None;
        };

        // Child layouts live in unscrolled content space; the viewport shows
        // them shifted up by the current translation.
        let caret_top = caret.y - translation.y;
        let caret_bottom = caret_top + caret.height;
        let viewport_bottom = viewport.y + viewport.height;

        let delta = if caret_top < viewport.y + CARET_MARGIN {
            caret_top - viewport.y - CARET_MARGIN
        } else if caret_bottom > viewport_bottom - CARET_MARGIN {
            caret_bottom - viewport_bottom + CARET_MARGIN
        } else {
            // Already visible; keep the page where it is.
            return Outcome::None;
        };

        Outcome::Some(Message::ScrollPreviewBy(delta))
    }
}

fn editor_style(_theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::BLACK),
        border: Border::default(),
        placeholder: Color::WHITE,
        value: Color::WHITE,
        selection: Color::from_rgb(0.25, 0.25, 0.25),
    }
}

fn markdown_style() -> markdown::Style {
    markdown::Style {
        font: EDITOR_FONT,
        inline_code_font: EDITOR_FONT,
        code_block_font: EDITOR_FONT,
        ..markdown::Style::from(&Theme::Dark)
    }
}

fn icon_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => Color::from_rgb(0.7, 0.7, 0.7),
            _ => Color::WHITE,
        },
        ..Default::default()
    }
}

fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.15, 0.15, 0.15))),
        text_color: Some(Color::WHITE),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
        ..Default::default()
    }
}

struct PreviewViewer<'a> {
    next_paragraph: Cell<usize>,
    selected_words: &'a HashSet<WordSelection>,
    selected_paragraphs: &'a HashSet<ParagraphSelection>,
    focused_element: usize,
    caret_column: usize,
    /// The `(anchor, caret)` endpoints of the visual-mode selection.
    visual: Option<((usize, usize), (usize, usize))>,
    /// Per-element lengths for visual-mode selections.
    elements: &'a [PreviewElement],
}

impl<'a> markdown::Viewer<'a, Message> for PreviewViewer<'a> {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::LinkClicked(url)
    }

    fn heading(
        &self,
        mut settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text: &'a markdown::Text,
        _index: usize,
    ) -> Element<'a, Message> {
        settings.text_size = match level {
            markdown::HeadingLevel::H1 => settings.h1_size,
            markdown::HeadingLevel::H2 => settings.h2_size,
            markdown::HeadingLevel::H3 => settings.h3_size,
            markdown::HeadingLevel::H4 => settings.h4_size,
            markdown::HeadingLevel::H5 => settings.h5_size,
            markdown::HeadingLevel::H6 => settings.h6_size,
        };
        self.text_element(settings, text)
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        self.text_element(settings, text)
    }
}

impl<'a> PreviewViewer<'a> {
    fn text_element(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        let element = ParagraphSelection(self.next_paragraph.get());
        self.next_paragraph.set(element.0 + 1);

        let focused = self.focused_element == element.0;
        let len = self
            .elements
            .get(element.0)
            .map_or(0, |element| element.len);
        let selection = self
            .visual
            .and_then(|(anchor, caret)| element_selection(anchor, caret, element.0, len));

        interactive_text::paragraph(
            settings,
            text,
            element,
            self.selected_words,
            self.selected_paragraphs,
            selection,
            focused.then_some(self.caret_column),
            focused.then(|| Id::new(PREVIEW_CARET_ID)),
        )
    }
}

fn view(editor: &Editor) -> Element<'_, Message> {
    let visual = editor
        .visual_anchor
        .map(|anchor| (anchor, (editor.preview_cursor, editor.preview_column)));

    let base_area: Element<'_, Message> = if editor.preview {
        scrollable(
            container(markdown::view_with(
                editor.markdown.items(),
                markdown::Settings::with_text_size(20.0, markdown_style()),
                &PreviewViewer {
                    next_paragraph: Cell::new(0),
                    selected_words: &editor.selected_words,
                    selected_paragraphs: &editor.selected_paragraphs,
                    focused_element: editor.preview_cursor,
                    caret_column: editor.preview_column,
                    visual,
                    elements: &editor.preview_elements,
                },
            ))
            .width(Length::Fill)
            .padding([0, 8]),
        )
        .id(Id::new(PREVIEW_SCROLL_ID))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::hidden(),
        ))
        .height(Length::Fill)
        .into()
    } else {
        text_editor(&editor.content)
            .on_action(Message::Edit)
            .font(EDITOR_FONT)
            .size(20)
            .height(Length::Fill)
            .padding(0)
            .line_height(1.8)
            .style(editor_style)
            .into()
    };

    // The note popup floats above the editing area; the backdrop closes it
    // on click and shields the area beneath from events.
    let editing_area: Element<'_, Message> = if editor.note_open {
        stack![base_area, note_popup(editor)].into()
    } else {
        base_area
    };

    let open_button = tooltip(
        button(
            canvas(OpenFileIcon)
                .width(Length::Fixed(16.0))
                .height(Length::Fixed(16.0)),
        )
        .on_press(Message::OpenFile)
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0))
        .padding(0)
        .style(icon_button_style),
        container(text("Ctrl + o, Open").font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    let preview_button = tooltip(
        button(
            canvas(PreviewIcon)
                .width(Length::Fixed(16.0))
                .height(Length::Fixed(16.0)),
        )
        .on_press(Message::TogglePreview)
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0))
        .padding(0)
        .style(icon_button_style),
        container(text("Ctrl + p, Preview").font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    // The main column: top margin, the writing area, and the bottom
    // controls. With comments saved, the comments sidebar sits beside it
    // and spans the whole window height.
    let main = column![
        Space::new()
            .width(Length::Fill)
            .height(Length::FillPortion(1)),
        row![
            Space::new()
                .width(Length::FillPortion(1))
                .height(Length::Fill),
            container(editing_area)
                .width(Length::FillPortion(9))
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::FillPortion(8)),
        {
            let mut controls: Vec<Element<'_, Message>> = vec![
                Space::new()
                    .width(Length::FillPortion(1))
                    .height(Length::Fill)
                    .into(),
                open_button.into(),
            ];

            if !editor.preview_only {
                controls.push(preview_button.into());
            }

            controls.push(mode_badge(editor));

            controls.push(
                Space::new()
                    .width(Length::FillPortion(9))
                    .height(Length::Fill)
                    .into(),
            );

            row(controls)
                .width(Length::Fill)
                .height(Length::FillPortion(1))
                .spacing(4)
                .align_y(alignment::Vertical::Bottom)
        },
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let content: Element<'_, Message> = if editor.comments.is_empty() {
        container(main)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(background_style)
            .into()
    } else {
        container(
            row![
                container(main)
                    .width(Length::FillPortion(9))
                    .height(Length::Fill),
                container(comments_sidebar(editor))
                    .width(Length::FillPortion(2))
                    .height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(background_style)
        .into()
    };

    content
}

/// The comments sidebar: a full-height panel with a scrollable list of
/// saved comments (each quoting the Markdown source it was written for,
/// collapsed to one line and trimmed) and a free text field with a Publish
/// button at the bottom.
fn comments_sidebar<'a>(editor: &'a Editor) -> Element<'a, Message> {
    let source = editor.content.text();

    let cards: Vec<Element<'_, Message>> = editor
        .comments
        .iter()
        .map(|comment| {
            let quoted = trimmed_element_source(
                &source,
                &editor.preview_elements,
                comment.anchor.0,
                COMMENT_QUOTE_MAX_CHARS,
            );

            container(
                column![
                    text(quoted)
                        .font(EDITOR_FONT)
                        .size(11)
                        .color(Color::from_rgb(0.45, 0.45, 0.45)),
                    text(comment.text.clone())
                        .font(EDITOR_FONT)
                        .size(13)
                        .color(Color::WHITE),
                ]
                .spacing(4)
                .width(Length::Fill),
            )
            .padding(8)
            .width(Length::Fill)
            .style(comment_card_style)
            .into()
        })
        .collect();

    container(
        column![
            container(
                text(format!("COMMENTS ({})", editor.comments.len()))
                    .font(EDITOR_FONT)
                    .size(11)
                    .color(Color::from_rgb(0.6, 0.6, 0.6)),
            )
            .padding(iced::Padding {
                top: 4.0,
                ..iced::Padding::new(0.0)
            }),
            scrollable(column(cards).spacing(8).width(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::hidden(),
                )),
            container(
                column![
                    text("Write a comment…")
                        .font(EDITOR_FONT)
                        .size(11)
                        .color(Color::from_rgb(0.5, 0.5, 0.5)),
                    text_editor(&editor.publish_text)
                        .on_action(Message::EditPublish)
                        .font(EDITOR_FONT)
                        .size(14)
                        .height(Length::Fixed(72.0))
                        .padding(6)
                        .style(publish_editor_style),
                ]
                .spacing(4)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(6)
            .style(publish_field_style),
            button(
                text("Publish")
                    .font(EDITOR_FONT)
                    .size(13)
                    .color(Color::WHITE),
            )
            .on_press(Message::PublishPressed)
            .width(Length::Fill)
            .padding([6, 12])
            .style(publish_button_style),
        ]
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(iced::Padding {
        top: 8.0,
        bottom: 8.0,
        left: 8.0,
        right: 8.0,
    })
    .style(sidebar_style)
    .into()
}

fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.03, 0.03, 0.03))),
        border: Border {
            color: Color::from_rgb(0.2, 0.2, 0.2),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn comment_card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.08, 0.08, 0.08))),
        border: Border {
            color: Color::from_rgb(0.25, 0.25, 0.25),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The publish text field stands out with a lighter surface than the
/// comment cards and a clearly visible border.
fn publish_field_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.17, 0.17, 0.17))),
        border: Border {
            color: Color::from_rgb(0.55, 0.55, 0.55),
            width: 1.5,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The editor inside the publish field keeps its own light surface so the
/// two nested boxes read as one input control.
fn publish_editor_style(_theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::from_rgb(0.22, 0.22, 0.22)),
        border: Border::default(),
        placeholder: Color::WHITE,
        value: Color::WHITE,
        selection: Color::from_rgb(0.35, 0.35, 0.35),
    }
}

/// The Publish button spans the sidebar width and reads as the primary
/// action of the panel.
fn publish_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(Color::from_rgb(0.25, 0.5, 1.0)))
            }
            _ => Some(Background::Color(Color::from_rgb(0.15, 0.3, 0.7))),
        },
        border: Border {
            color: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb(0.55, 0.7, 1.0)
                }
                _ => Color::from_rgb(0.35, 0.5, 0.9),
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The bottom-bar mode indicator: which navigation mode the editor is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Editing the Markdown source.
    Write,
    /// Previewing, caret moves without selecting.
    View,
    /// Previewing, motions extend the selection.
    Visual,
}

fn editor_mode(editor: &Editor) -> Mode {
    if editor.visual_anchor.is_some() {
        Mode::Visual
    } else if editor.preview {
        Mode::View
    } else {
        Mode::Write
    }
}

/// A small badge naming the current mode, placed next to the open icon in
/// the bottom bar. Visual mode is highlighted with the selection blue so
/// the active selection state is obvious at a glance.
fn mode_badge(editor: &Editor) -> Element<'_, Message> {
    let mode = editor_mode(editor);

    let (label, color) = match mode {
        Mode::Visual => ("VISUAL", Color::from_rgb(0.4, 0.65, 1.0)),
        Mode::View => ("VIEW", Color::from_rgb(0.6, 0.6, 0.6)),
        Mode::Write => ("WRITE", Color::from_rgb(0.6, 0.6, 0.6)),
    };

    container(text(label).font(EDITOR_FONT).size(12).color(color))
        .padding([0, 8])
        .height(Length::Fixed(28.0))
        .align_y(alignment::Vertical::Center)
        .style(move |_theme| mode_badge_style(color))
        .into()
}

fn mode_badge_style(color: Color) -> container::Style {
    container::Style {
        text_color: Some(color),
        border: Border {
            color: Color { a: 0.4, ..color },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn background_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::BLACK)),
        ..Default::default()
    }
}

/// The note popup: a translucent backdrop with a centered card holding a
/// text area and a button. Clicking the backdrop closes the popup; clicks
/// on the card are swallowed.
fn note_popup(editor: &Editor) -> Element<'_, Message> {
    let card = mouse_area(
        container(
            column![
                text("Note")
                    .font(EDITOR_FONT)
                    .size(12)
                    .color(Color::from_rgb(0.6, 0.6, 0.6)),
                text_editor(&editor.note_text)
                    .id(Id::new(NOTE_EDITOR_ID))
                    .on_action(Message::EditNote)
                    .font(EDITOR_FONT)
                    .size(20)
                    .height(Length::Fixed(160.0))
                    .padding(8)
                    .style(editor_style),
                row![
                    button(
                        text("Close")
                            .font(EDITOR_FONT)
                            .size(14)
                            .color(Color::from_rgb(0.6, 0.6, 0.6)),
                    )
                    .on_press(Message::CloseNotePopup)
                    .padding([6, 12])
                    .style(popup_button_style),
                    Space::new().width(Length::Fill),
                    button(text("Save").font(EDITOR_FONT).size(14).color(Color::WHITE),)
                        .on_press(Message::SaveNote)
                        .padding([6, 12])
                        .style(popup_button_style),
                ]
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .width(Length::Fixed(440.0))
        .padding(16)
        .style(note_card_style),
    )
    .on_press(Message::NoteCardPressed);

    mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(note_backdrop_style),
    )
    .on_press(Message::CloseNotePopup)
    .into()
}

fn note_backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
        ..Default::default()
    }
}

fn note_card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.1, 0.1, 0.1))),
        border: Border {
            color: Color::from_rgb(0.3, 0.3, 0.3),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn popup_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(Color::from_rgb(0.2, 0.2, 0.2)))
            }
            _ => None,
        },
        border: Border {
            color: Color::from_rgb(0.3, 0.3, 0.3),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

struct Args {
    path: Option<String>,
    preview: bool,
}

const USAGE: &str = "Usage: agmawrite [FILE] [--preview]

Arguments:
  FILE         Path to a Markdown file to open

Options:
  --preview    Open FILE in preview-only mode; editing and switching to
               write mode are disabled
  -h, --help   Print this message";

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut args = Args {
        path: None,
        preview: false,
    };

    for arg in argv {
        match arg.as_str() {
            "--preview" => args.preview = true,
            path if !path.starts_with('-') => {
                if args.path.replace(path.to_string()).is_some() {
                    return Err("unexpected extra file argument".to_string());
                }
            }
            other => return Err(format!("unexpected argument '{other}'")),
        }
    }

    if args.preview && args.path.is_none() {
        return Err("--preview requires a FILE".to_string());
    }

    Ok(args)
}

fn boot(args: &Args) -> (Editor, Task<Message>) {
    let contents = args
        .path
        .as_ref()
        .map(|path| match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("agmawrite: cannot read '{path}': {error}");
                std::process::exit(1);
            }
        });

    let preview_elements = contents
        .as_deref()
        .map(preview_elements)
        .unwrap_or_default();

    let editor = Editor {
        content: contents
            .as_deref()
            .map(text_editor::Content::with_text)
            .unwrap_or_default(),
        markdown: contents
            .as_deref()
            .map(markdown::Content::parse)
            .unwrap_or_default(),
        preview: args.preview,
        preview_only: args.preview,
        selected_words: HashSet::new(),
        selected_paragraphs: HashSet::new(),
        preview_cursor: 0,
        preview_column: 0,
        preview_column_target: 0,
        preview_elements,
        pending_g: false,
        visual_anchor: None,
        note_open: false,
        note_text: text_editor::Content::new(),
        comments: Vec::new(),
        publish_text: text_editor::Content::new(),
    };

    let task = if args.preview {
        Task::none()
    } else {
        focus_next()
    };

    (editor, task)
}

fn theme(_editor: &Editor) -> Theme {
    Theme::Dark
}

fn main() -> iced::Result {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    if argv.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return Ok(());
    }

    let args = match parse_args(argv) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("agmawrite: {error}\n\n{USAGE}");
            std::process::exit(1);
        }
    };

    application(move || boot(&args), update, view)
        .title("agmawrite")
        .theme(theme)
        .subscription(subscription)
        .font(include_bytes!("../fonts/iAWriterMonoS-Regular.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Italic.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Bold.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-BoldItalic.ttf").as_slice())
        .run()
}

#[cfg(test)]
mod tests {
    use super::{
        editor_mode, element_selection, handle_key_press, jump_caret, move_caret, move_word,
        preview_element_after, preview_elements, save_note, trimmed_element_source, Editor,
        ElementKind, Jump, Message, Mode, Motion, WordMotion,
    };
    use iced::keyboard::{self, key, Modifiers};

    fn key_pressed_with(
        character: &str,
        repeat: bool,
        pending_g: bool,
        note_open: bool,
    ) -> Option<Message> {
        let key = keyboard::Key::Character(character.into());

        handle_key_press(
            keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::default(),
                text: None,
                repeat,
            },
            true,
            false,
            pending_g,
            note_open,
        )
    }

    fn key_pressed(character: &str, repeat: bool) -> Option<Message> {
        key_pressed_with(character, repeat, false, false)
    }

    /// Holding a motion key auto-repeats the motion, like holding `j` or `k`
    /// in vim; one-shot actions and the `g`-prefix never repeat.
    #[test]
    fn held_motion_keys_repeat() {
        // Plain presses and repeats both move.
        assert!(matches!(
            key_pressed("j", false),
            Some(Message::MovePreviewCursor(Motion::Down))
        ));
        assert!(matches!(
            key_pressed("j", true),
            Some(Message::MovePreviewCursor(Motion::Down))
        ));
        assert!(matches!(
            key_pressed("k", true),
            Some(Message::MovePreviewCursor(Motion::Up))
        ));
        assert!(matches!(
            key_pressed("h", true),
            Some(Message::MovePreviewCursor(Motion::Left))
        ));
        assert!(matches!(
            key_pressed("l", true),
            Some(Message::MovePreviewCursor(Motion::Right))
        ));
        assert!(matches!(
            key_pressed("w", true),
            Some(Message::MovePreviewWord(WordMotion::NextStart))
        ));
        assert!(matches!(
            key_pressed("b", true),
            Some(Message::MovePreviewWord(WordMotion::PreviousStart))
        ));
        assert!(matches!(
            key_pressed("e", true),
            Some(Message::MovePreviewWord(WordMotion::NextEnd))
        ));

        // One-shot actions ignore repeats.
        assert!(matches!(
            key_pressed("c", false),
            Some(Message::OpenNotePopup)
        ));
        assert!(key_pressed("c", true).is_none());

        // The `g` prefix never arms or fires on repeat — `gg` and `ge` need
        // two fresh presses.
        assert!(matches!(
            key_pressed("g", false),
            Some(Message::PreviewGPressed)
        ));
        assert!(key_pressed("g", true).is_none());

        // `v` toggles visual mode on fresh presses only.
        assert!(matches!(
            key_pressed("v", false),
            Some(Message::ToggleVisualMode)
        ));
        assert!(key_pressed("v", true).is_none());
    }

    /// Visual mode anchors one end and the caret forms the other; motions
    /// between them select partial elements at the ends and full elements
    /// in between, regardless of direction.
    #[test]
    fn visual_selection_spans_elements() {
        // "aaaa", "bb", "cc" — element lengths 4, 2, 2.
        let selection =
            |anchor: (usize, usize), caret: (usize, usize), index: usize, len: usize| {
                element_selection(anchor, caret, index, len)
            };

        // Within one element: `v` then `l` three times selects 3 chars.
        assert_eq!(selection((0, 1), (0, 4), 0, 4), Some(1..4));
        assert_eq!(selection((0, 1), (0, 4), 1, 2), None);

        // Across elements: the start selects to the end of its element, the
        // middle is full, the destination selects up to the caret column.
        assert_eq!(selection((0, 1), (2, 1), 0, 4), Some(1..4));
        assert_eq!(selection((0, 1), (2, 1), 1, 2), Some(0..2));
        assert_eq!(selection((0, 1), (2, 1), 2, 2), Some(0..1));

        // Direction does not matter.
        assert_eq!(selection((2, 1), (0, 1), 1, 2), Some(0..2));

        // Columns beyond an element's length clamp away.
        assert_eq!(selection((1, 9), (1, 10), 1, 2), None);
        assert_eq!(selection((0, 0), (0, 0), 0, 4), None);
    }

    /// The bottom bar names the navigation mode: write when editing, view
    /// when previewing, and visual while a selection is anchored.
    #[test]
    fn mode_badge_reflects_editor_state() {
        let mut editor = Editor {
            content: iced::widget::text_editor::Content::new(),
            markdown: iced::widget::markdown::Content::parse(""),
            preview: false,
            preview_only: false,
            selected_words: Default::default(),
            selected_paragraphs: Default::default(),
            preview_cursor: 0,
            preview_column: 0,
            preview_column_target: 0,
            preview_elements: Vec::new(),
            pending_g: false,
            visual_anchor: None,
            note_open: false,
            note_text: iced::widget::text_editor::Content::new(),
            comments: Vec::new(),
            publish_text: iced::widget::text_editor::Content::new(),
        };

        assert_eq!(editor_mode(&editor), Mode::Write);

        editor.preview = true;
        assert_eq!(editor_mode(&editor), Mode::View);

        editor.visual_anchor = Some((0, 0));
        assert_eq!(editor_mode(&editor), Mode::Visual);

        // Leaving visual mode returns to view.
        editor.visual_anchor = None;
        assert_eq!(editor_mode(&editor), Mode::View);
    }

    /// Saving the popup note stores a comment anchored at the caret and
    /// resets the popup; empty notes are discarded.
    #[test]
    fn saving_a_note_adds_a_comment() {
        let mut editor = Editor {
            content: iced::widget::text_editor::Content::with_text("# Title\n\nbody"),
            markdown: iced::widget::markdown::Content::parse(""),
            preview: true,
            preview_only: false,
            selected_words: Default::default(),
            selected_paragraphs: Default::default(),
            preview_cursor: 1,
            preview_column: 2,
            preview_column_target: 2,
            preview_elements: preview_elements("# Title\n\nbody"),
            pending_g: false,
            visual_anchor: None,
            note_open: true,
            note_text: iced::widget::text_editor::Content::with_text("  fix this  \n"),
            comments: Vec::new(),
            publish_text: iced::widget::text_editor::Content::new(),
        };

        save_note(&mut editor);

        assert!(!editor.note_open);
        assert_eq!(editor.note_text.text(), "");
        assert_eq!(editor.comments.len(), 1);
        assert_eq!(editor.comments[0].text, "fix this");
        assert_eq!(editor.comments[0].anchor, (1, 2));

        // An empty note only closes the popup; the first comment stays.
        editor.note_open = true;
        editor.note_text = iced::widget::text_editor::Content::with_text("   ");
        save_note(&mut editor);
        assert_eq!(editor.comments.len(), 1);
        assert!(!editor.note_open);
    }

    /// The sidebar quotes the Markdown source of the commented element,
    /// collapsed to one line and trimmed to the card width.
    #[test]
    fn comment_quotes_trimmed_element_source() {
        let markdown = "# Some rather long heading text here\n\nshort";
        let elements = preview_elements(markdown);

        // Short elements keep their source, collapsed onto one line.
        assert_eq!(trimmed_element_source(markdown, &elements, 1, 60), "short");

        // Long element sources are cut off after `max_chars` with an
        // ellipsis.
        let quoted = trimmed_element_source(markdown, &elements, 0, 10);
        assert_eq!(quoted, "# Some rat…");

        // Unknown indices quote nothing.
        assert_eq!(trimmed_element_source(markdown, &elements, 9, 60), "");
    }

    /// While the note popup is open, plain keys go to its text area — only
    /// Escape closes it and Ctrl+S saves the comment.
    #[test]
    fn note_popup_swallows_keys() {
        let escape = || {
            let key = keyboard::Key::Named(keyboard::key::Named::Escape);

            handle_key_press(
                keyboard::Event::KeyPressed {
                    key: key.clone(),
                    modified_key: key,
                    physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::default(),
                    text: None,
                    repeat: false,
                },
                true,
                false,
                false,
                true,
            )
        };

        assert!(matches!(escape(), Some(Message::CloseNotePopup)));

        // Motions, `c`, and typing characters produce nothing while the
        // popup is open.
        for key in ["j", "k", "h", "l", "w", "b", "e", "g", "c", "G", "x"] {
            assert!(
                key_pressed_with(key, false, false, true).is_none(),
                "'{key}' should be swallowed by the note popup"
            );
        }

        // Ctrl+S saves the note while the popup is open.
        let ctrl_s = keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("s".into()),
            modified_key: keyboard::Key::Character("s".into()),
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::CTRL,
            text: None,
            repeat: false,
        };

        assert!(matches!(
            handle_key_press(ctrl_s, true, false, false, true),
            Some(Message::SaveNote)
        ));
    }

    /// The preview caret stops early when `preview_elements` disagrees
    /// with the number of elements the Markdown viewer actually numbers
    /// (list items are `viewer.paragraph` calls too). This pins the parity,
    /// and the grid position of table cells.
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

        let elements = preview_elements(markdown);
        let texts: Vec<&str> = elements
            .iter()
            .map(|element| &markdown[element.source.clone()])
            .collect();

        // Headings, paragraphs, quotes, table cells, and every list item
        // (tight, ordered, task, and nested) — but not the code block.
        assert_eq!(texts.len(), 14);
        for (text, expected) in texts.iter().zip([
            "Title", "Intro", "quote", "A", "B", "1", "2", "one", "two", "nested", "first", "todo",
            "done", "Tail",
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
        let elements = preview_elements(markdown);
        let navigate = |from, motion| preview_element_after(&elements, from, motion);

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
        let elements = preview_elements("aa bb\n\ncc dd");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].len, 5);
        assert_eq!(elements[1].len, 5);

        let motion =
            |cursor, column, target, motion| move_caret(&elements, cursor, column, target, motion);

        // `l` walks the element one character at a time.
        assert_eq!(motion(0, 0, 0, Motion::Right), Some((0, 1, 1)));
        assert_eq!(motion(0, 4, 4, Motion::Right), Some((0, 5, 5)));
        // At the end of the element, `l` crosses to the next one.
        assert_eq!(motion(0, 5, 5, Motion::Right), Some((1, 0, 0)));
        assert_eq!(motion(1, 5, 5, Motion::Right), None); // end of document

        // `h` mirrors it, crossing to the end of the previous element.
        assert_eq!(motion(1, 0, 0, Motion::Left), Some((0, 5, 5)));
        assert_eq!(motion(0, 1, 1, Motion::Left), Some((0, 0, 0)));
        assert_eq!(motion(0, 0, 0, Motion::Left), None); // start of document
    }

    /// `j`/`k` keep the vim sticky column: the target column survives moves
    /// and clamps to the destination element's length.
    #[test]
    fn j_and_k_keep_the_sticky_column() {
        // "aaaa", "bb", "ccccc"
        let elements = preview_elements("aaaa\n\nbb\n\nccccc");
        assert_eq!(elements.len(), 3);

        let motion =
            |cursor, column, target, motion| move_caret(&elements, cursor, column, target, motion);

        // The column clamps when moving to a shorter element...
        assert_eq!(motion(0, 3, 3, Motion::Down), Some((1, 2, 3)));
        // ...and returns when moving on to a longer one.
        assert_eq!(motion(1, 2, 3, Motion::Down), Some((2, 3, 3)));
        assert_eq!(motion(2, 3, 3, Motion::Up), Some((1, 2, 3)));

        // In tables the sticky column applies to the destination cell.
        let markdown = "\
| Feature | Editor |
|---|---|
| Headings | ✅ |
";
        let cells = preview_elements(markdown);
        // Feature → Headings in the same table column.
        let moved = move_caret(&cells, 0, 5, 5, Motion::Down);
        assert_eq!(moved, Some((2, 5, 5)));
        let moved = move_caret(&cells, 1, 3, 3, Motion::Down);
        // ✅ is one grapheme, so the column clamps to 1.
        assert_eq!(moved, Some((3, 1, 3)));
    }

    /// `w`/`b` jump between word starts and `e`/`ge` between word ends,
    /// treating punctuation runs as words like vim; at element edges they
    /// cross to the neighbouring element like crossing a line.
    #[test]
    fn w_b_e_and_ge_move_by_words() {
        // "one two, three" — words: one, two, ",", three.
        let elements = preview_elements("one two, three\n\nnext");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].text, "one two, three");

        let word = |cursor, column, motion| move_word(&elements, cursor, column, motion);

        // w walks the word starts, comma included.
        assert_eq!(word(0, 0, WordMotion::NextStart), Some((0, 4))); // → two
        assert_eq!(word(0, 4, WordMotion::NextStart), Some((0, 7))); // → ,
        assert_eq!(word(0, 7, WordMotion::NextStart), Some((0, 9))); // → three
        assert_eq!(word(0, 9, WordMotion::NextStart), Some((1, 0))); // → next
        assert_eq!(word(1, 4, WordMotion::NextStart), None); // end of document

        // b walks them backwards.
        assert_eq!(word(0, 9, WordMotion::PreviousStart), Some((0, 7)));
        assert_eq!(word(0, 4, WordMotion::PreviousStart), Some((0, 0)));
        assert_eq!(word(0, 0, WordMotion::PreviousStart), None); // start of document

        // e places the caret just past the last character of each word.
        assert_eq!(word(0, 0, WordMotion::NextEnd), Some((0, 3))); // one|
        assert_eq!(word(0, 3, WordMotion::NextEnd), Some((0, 7))); // two|
        assert_eq!(word(0, 7, WordMotion::NextEnd), Some((0, 8))); // ,|
        assert_eq!(word(0, 8, WordMotion::NextEnd), Some((0, 14))); // three|
        assert_eq!(word(0, 14, WordMotion::NextEnd), Some((1, 4))); // next| (crosses)
        assert_eq!(word(1, 4, WordMotion::NextEnd), None); // end of document

        // ge places it just past the last character of the previous word,
        // crossing elements at the start.
        assert_eq!(word(0, 9, WordMotion::PreviousEnd), Some((0, 8)));
        assert_eq!(word(0, 4, WordMotion::PreviousEnd), Some((0, 3)));
        assert_eq!(word(0, 0, WordMotion::PreviousEnd), None);
        assert_eq!(word(1, 0, WordMotion::PreviousEnd), Some((0, 14)));
    }

    /// `gg` and `G` jump to the first and last element, keeping vim's sticky
    /// column clamped to the destination element.
    #[test]
    fn gg_and_g_jump_between_document_ends() {
        // "aaaa", "b", "ccccc"
        let elements = preview_elements("aaaa\n\nb\n\nccccc");
        assert_eq!(elements.len(), 3);

        assert_eq!(jump_caret(&elements, 3, Jump::First), Some((0, 3)));
        assert_eq!(jump_caret(&elements, 3, Jump::Last), Some((2, 3)));
        // The sticky column clamps to shorter elements.
        assert_eq!(jump_caret(&elements, 9, Jump::Last), Some((2, 5)));
        assert_eq!(jump_caret(&elements, 9, Jump::First), Some((0, 4)));

        assert_eq!(jump_caret(&[], 0, Jump::First), None);
        assert_eq!(jump_caret(&[], 0, Jump::Last), None);
    }
}
