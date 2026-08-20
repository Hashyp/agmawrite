mod cli;
mod command;
mod comments;
mod document;
mod editing;
mod find;
mod help;
mod highlight;
mod interactive_text;
mod keymap;
mod preview;
mod theme;
mod watch;

use cli::{Args, ParseOutcome};
use command::{
    Command, CommentsCommand, DocumentCommand, FindCommand, HelpCommand, PreviewCommand,
};
use comments::{Comments, Mark, Span};
use keymap::{Keymap, Mode, Transition};
use preview::{Caret, CaretPosition, ElementMap, Jump, Motion, Page, Placement, WordMotion};
use theme::Palette;

use iced::advanced::widget::operation::{Outcome, Scrollable};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{layout, renderer, Clipboard, Layout, Shell, Widget};
use iced::widget::markdown::Catalog as _;
use iced::widget::{
    button, canvas, column, container, markdown, mouse_area, operation::focus,
    operation::focus_next, operation::scroll_by, operation::AbsoluteOffset, row, scrollable, stack,
    text, text_editor, text_input, tooltip, Id, Space,
};
use iced::{
    alignment, application, keyboard, mouse, Background, Border, Color, Element, Font, Length,
    Point, Rectangle, Renderer, Size, Subscription, Task, Theme, Vector,
};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
const PREVIEW_SCROLL_ID: &str = "preview-scroll";
const PREVIEW_CARET_ID: &str = "preview-caret";
const NOTE_EDITOR_ID: &str = "note-editor";
/// The id of the find popup's query field, focused when the popup opens.
const FIND_INPUT_ID: &str = "find-input";
/// The design space the icon glyphs are drawn in, before scaling to the
/// canvas size.
const ICON_DESIGN_SIZE: f32 = 16.0;
/// The bottom-bar icon glyph size: 150% of the original 16px.
const ICON_SIZE: f32 = 24.0;
/// The bottom-bar icon button size: 150% of the original 28px.
const ICON_BUTTON_SIZE: f32 = 42.0;
/// The id of the source text editor, refocused when switching back to
/// write mode so the cursor reappears where it was left.
const SOURCE_EDITOR_ID: &str = "source-editor";
/// Margin kept between the preview caret and the viewport edges while scrolling.
const CARET_MARGIN: f32 = 8.0;

struct Editor {
    document: document::State,
    markdown: markdown::Content,
    /// The input mode stack — write, view, visual, note — owning key
    /// handling and the mode badge's state.
    keymap: Keymap,
    /// The preview caret: element, grapheme column, and sticky target
    /// column, owned by the preview module.
    caret: Caret,
    /// The numbered preview elements, owned by the preview module.
    preview_elements: ElementMap,
    /// The fixed end of the visual-mode selection, as a caret position.
    /// `None` outside visual mode.
    visual_anchor: Option<CaretPosition>,
    /// The text of the note popup.
    note_text: text_editor::Content,
    /// Saved comments, the active one, and the publish draft.
    comments: Comments,
    /// Manual override for the comments sidebar's visibility (`Ctrl+B`):
    /// `None` follows the default — shown once there are comments.
    sidebar_override: Option<bool>,
    /// The find popup's state: the query and the current match.
    find: find::Find,
    /// The shortcuts Help window's query state.
    help: help::Help,
    /// The comment the note popup edits, when it is open for editing an
    /// existing comment instead of writing a fresh one.
    editing_comment: Option<(usize, usize)>,
    /// The omarchy color scheme the interface paints with.
    palette: Palette,
}

#[derive(Debug, Clone)]
enum Message {
    Document(document::Message),
    TogglePreview,
    LinkClicked(markdown::Uri),
    MovePreviewCursor(Motion, usize),
    MovePreviewWord(WordMotion, usize),
    MovePreviewJump(Jump, usize),
    PreviewGPressed,
    PreviewZPressed,
    PreviewCountPressed(u32),
    PreviewCancel,
    ToggleVisualMode,
    ScrollPreviewBy(f32),
    ScrollPreviewPage(Page, usize),
    ScrollPreviewCaret(Placement),
    OpenNotePopup,
    CloseNotePopup,
    EditNote(text_editor::Action),
    SaveNote,
    EditActiveComment,
    NoteCardPressed,
    CommentCardPressed(usize, usize),
    DeleteComment(usize, usize),
    ResolveComment(usize),
    EditPublish(text_editor::Action),
    PublishPressed,
    AddGlobalComment,
    NextComment,
    ToggleSidebar,
    OpenFind,
    CloseFind,
    OpenHelp,
    Help(help::Message),
    FindQueryChanged(String),
    FindNext,
    FindPrevious,
    PaletteChanged,
}

/// Maps the input layer's semantic command into the application's current
/// root message vocabulary.
fn message_for_command(command: Command) -> Message {
    match command {
        Command::Document(command) => Message::Document(match command {
            DocumentCommand::Open => document::Message::OpenRequested,
            DocumentCommand::Save => document::Message::SaveRequested,
            DocumentCommand::CancelUnsaved => document::Message::UnsavedCancel,
        }),
        Command::Preview(command) => match command {
            PreviewCommand::Toggle => Message::TogglePreview,
            PreviewCommand::Move(motion, count) => Message::MovePreviewCursor(motion, count),
            PreviewCommand::MoveWord(motion, count) => Message::MovePreviewWord(motion, count),
            PreviewCommand::Jump(jump, count) => Message::MovePreviewJump(jump, count),
            PreviewCommand::ArmG => Message::PreviewGPressed,
            PreviewCommand::ArmZ => Message::PreviewZPressed,
            PreviewCommand::Count(digit) => Message::PreviewCountPressed(digit),
            PreviewCommand::Cancel => Message::PreviewCancel,
            PreviewCommand::ToggleVisual => Message::ToggleVisualMode,
            PreviewCommand::ScrollPage(page, count) => Message::ScrollPreviewPage(page, count),
            PreviewCommand::ScrollCaret(placement) => Message::ScrollPreviewCaret(placement),
        },
        Command::Comments(command) => match command {
            CommentsCommand::OpenNote => Message::OpenNotePopup,
            CommentsCommand::CloseNote => Message::CloseNotePopup,
            CommentsCommand::SaveNote => Message::SaveNote,
            CommentsCommand::EditActive => Message::EditActiveComment,
            CommentsCommand::Next => Message::NextComment,
            CommentsCommand::ToggleSidebar => Message::ToggleSidebar,
            CommentsCommand::AddGlobal => Message::AddGlobalComment,
            CommentsCommand::Publish => Message::PublishPressed,
        },
        Command::Find(command) => match command {
            FindCommand::Open => Message::OpenFind,
            FindCommand::Close => Message::CloseFind,
            FindCommand::Next => Message::FindNext,
            FindCommand::Previous => Message::FindPrevious,
        },
        Command::Help(command) => match command {
            HelpCommand::Open => Message::OpenHelp,
            HelpCommand::Close => Message::Help(help::Message::Close),
        },
    }
}

/// Projects root activity onto the keymap's input-local transition model.
fn input_transition(message: &Message) -> Transition {
    match message {
        Message::Document(document::Message::OpenLoaded(Some(_))) => Transition::DocumentLoaded,
        Message::TogglePreview => Transition::PreviewToggled,
        Message::ToggleVisualMode => Transition::VisualToggled,
        Message::PreviewCancel => Transition::PreviewCancelled,
        Message::PreviewGPressed => Transition::GArmed,
        Message::PreviewZPressed => Transition::ZArmed,
        Message::PreviewCountPressed(digit) => Transition::CountPressed(*digit),
        Message::OpenNotePopup => Transition::NoteOpened,
        Message::CloseNotePopup | Message::SaveNote => Transition::NoteClosed,
        Message::OpenFind => Transition::FindOpened,
        Message::CloseFind => Transition::FindClosed,
        Message::OpenHelp => Transition::HelpOpened,
        Message::Help(help::Message::Close) => Transition::HelpClosed,
        Message::Help(_) => Transition::HelpActivity,
        Message::Document(
            document::Message::UnsavedCancel
            | document::Message::UnsavedSave
            | document::Message::UnsavedDiscard,
        ) => Transition::UnsavedClosed,
        _ => Transition::Activity,
    }
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
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

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
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

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

struct SaveIcon;

/// The write-mode icon of the preview toggle: a pencil, drawn in the same
/// stroke style as the eye so the toggle reads as one button with two
/// glyphs.
struct WriteIcon;

impl<Message> canvas::Program<Message> for WriteIcon {
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
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let stroke = || {
            canvas::Stroke::default()
                .with_color(Color::from_rgb(0.65, 0.65, 0.65))
                .with_width(1.4)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round)
        };

        // A pencil lying diagonal: a triangular tip at the bottom-left, a
        // band above it, and the body running to the top-right.
        let body = canvas::Path::new(|path| {
            path.move_to(Point::new(2.5, 13.5));
            path.line_to(Point::new(3.35, 10.95));
            path.line_to(Point::new(9.07, 5.23));
            path.line_to(Point::new(10.77, 6.93));
            path.line_to(Point::new(5.05, 12.65));
            path.close();
        });
        let band = canvas::Path::new(|path| {
            path.move_to(Point::new(3.35, 10.95));
            path.line_to(Point::new(5.05, 12.65));
        });
        let edge = canvas::Path::new(|path| {
            path.move_to(Point::new(9.6, 4.7));
            path.line_to(Point::new(11.3, 6.4));
        });

        frame.stroke(&body, stroke());
        frame.stroke(&band, stroke());
        frame.stroke(&edge, stroke());

        vec![frame.into_geometry()]
    }
}

/// The comments icon of the collapsed sidebar rail: a speech bubble in
/// the same stroke style as the other icons.
struct CommentsIcon;

impl<Message> canvas::Program<Message> for CommentsIcon {
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
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let stroke = || {
            canvas::Stroke::default()
                .with_color(Color::from_rgb(0.65, 0.65, 0.65))
                .with_width(1.4)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round)
        };

        // A rounded speech bubble with a tail at the bottom-left.
        let bubble = canvas::Path::new(|path| {
            path.move_to(Point::new(8.0, 2.5));
            path.quadratic_curve_to(Point::new(13.5, 2.5), Point::new(13.5, 7.0));
            path.quadratic_curve_to(Point::new(13.5, 10.5), Point::new(9.5, 10.8));
            path.line_to(Point::new(6.0, 13.0));
            path.line_to(Point::new(6.4, 10.5));
            path.quadratic_curve_to(Point::new(2.5, 10.2), Point::new(2.5, 7.0));
            path.quadratic_curve_to(Point::new(2.5, 2.5), Point::new(8.0, 2.5));
            path.close();
        });

        frame.stroke(&bubble, stroke());

        vec![frame.into_geometry()]
    }
}

impl<Message> canvas::Program<Message> for SaveIcon {
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
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let stroke = || {
            canvas::Stroke::default()
                .with_color(Color::from_rgb(0.65, 0.65, 0.65))
                .with_width(1.4)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round)
        };

        // A floppy disk: the body with a beveled corner, the shutter notch
        // on top, and the label slot at the bottom.
        let body = canvas::Path::new(|path| {
            path.move_to(Point::new(2.5, 1.5));
            path.line_to(Point::new(11.0, 1.5));
            path.line_to(Point::new(13.5, 4.0));
            path.line_to(Point::new(13.5, 14.5));
            path.line_to(Point::new(2.5, 14.5));
            path.close();
        });
        let shutter = canvas::Path::new(|path| {
            path.move_to(Point::new(5.0, 1.5));
            path.line_to(Point::new(5.0, 6.0));
            path.line_to(Point::new(10.5, 6.0));
            path.line_to(Point::new(10.5, 1.5));
        });
        let slot = canvas::Path::new(|path| {
            path.move_to(Point::new(4.5, 14.5));
            path.line_to(Point::new(4.5, 10.0));
            path.line_to(Point::new(11.5, 10.0));
            path.line_to(Point::new(11.5, 14.5));
        });

        frame.stroke(&body, stroke());
        frame.stroke(&shutter, stroke());
        frame.stroke(&slot, stroke());

        vec![frame.into_geometry()]
    }
}

fn subscription(editor: &Editor) -> Subscription<Message> {
    let keys = keyboard::listen()
        .with(editor.keymap)
        .filter_map(|(keymap, event)| keymap.handle(event))
        .map(message_for_command);

    // Close requests and file changes enter the document feature as local
    // messages, then cross the app boundary through one nested variant.
    let close = iced::window::close_requests()
        .map(document::Message::CloseRequested)
        .map(Message::Document);

    match editor.document.path() {
        Some(path) => Subscription::batch([
            keys,
            close,
            document::subscription(path).map(Message::Document),
            watch_palette(),
        ]),
        None => Subscription::batch([keys, close, watch_palette()]),
    }
}

/// Watches the omarchy current-theme state so switching themes re-paints
/// the interface live.
fn watch_palette() -> Subscription<Message> {
    let state = std::env::var_os("HOME")
        .map(|home| std::path::PathBuf::from(home).join(".local/state/omarchy/current"))
        .unwrap_or_else(|| std::path::PathBuf::from(".local/state/omarchy/current"));

    Subscription::run_with(("omarchy-palette", state), move |(_, state)| {
        let state = state.clone();
        iced::stream::channel(1, move |sender| async move {
            watch::spawn_directory_events(state, sender, Message::PaletteChanged);
            // The events arrive on the watcher thread; this runner only
            // keeps the stream alive.
            std::future::pending::<()>().await;
        })
    })
}

/// Why the source and its temporary preview projection are being synchronized.
/// Each reason carries the policy that currently belongs to the app; Task 14
/// will move these operations behind `preview::State`.
enum SourceSynchronization {
    InitialLoad,
    SourceReplaced(document::SourceReplacement),
    EnterPreview,
}

/// Coordinates source replacement with every derived preview value. Returns
/// whether the refreshed caret should be revealed in the current viewport.
fn synchronize_source(editor: &mut Editor, reason: SourceSynchronization) -> bool {
    let (reset_document_bound_state, clear_visual_selection, place_from_source, reveal) =
        match reason {
            SourceSynchronization::InitialLoad => (true, true, false, false),
            SourceSynchronization::SourceReplaced(document::SourceReplacement::Loaded) => {
                (true, true, false, false)
            }
            SourceSynchronization::SourceReplaced(document::SourceReplacement::External) => {
                (false, false, false, editor.keymap.preview())
            }
            SourceSynchronization::EnterPreview => (false, true, true, true),
        };

    let contents = editor.document.text();
    editor.markdown = markdown::Content::parse(&contents);
    editor.preview_elements = ElementMap::parse(&contents);

    if place_from_source {
        editor.caret.move_to_source_cursor(
            editor.document.content(),
            editor.preview_elements.elements(),
        );
    } else {
        editor.caret = Caret::new();
    }

    if clear_visual_selection {
        editor.visual_anchor = None;
    }

    if reset_document_bound_state {
        editor.note_text = text_editor::Content::new();
        editor.comments = Comments::new();
        editor.editing_comment = None;
    }

    reveal
}

fn handle_document_event(editor: &mut Editor, event: document::Event) -> Task<Message> {
    match event {
        document::Event::SourceReplaced { reason } => {
            if synchronize_source(editor, SourceSynchronization::SourceReplaced(reason)) {
                reveal_preview_caret()
            } else {
                Task::none()
            }
        }
        document::Event::CloseWindow(id) => iced::window::close(id),
        document::Event::UnsavedVisibilityChanged(visible) => {
            editor.keymap.note(if visible {
                Transition::UnsavedOpened
            } else {
                Transition::UnsavedClosed
            });
            Task::none()
        }
    }
}

fn update(editor: &mut Editor, message: Message) -> Task<Message> {
    // The note popup's text area owns the draft until the popup closes; a
    // save started from the open popup still lands after the keymap has
    // marked the popup closed.
    let note_was_open = editor.keymap.note_open();
    editor.keymap.note(input_transition(&message));

    match message {
        Message::Document(message) => {
            let document::Update { task, event } = document::update(&mut editor.document, message);
            let event_task =
                event.map_or_else(Task::none, |event| handle_document_event(editor, event));

            return Task::batch([task.map(Message::Document), event_task]);
        }
        Message::PaletteChanged => editor.palette = Palette::current(),
        Message::TogglePreview => {
            if !editor.keymap.preview_only() {
                if editor.keymap.preview() {
                    if synchronize_source(editor, SourceSynchronization::EnterPreview) {
                        return reveal_preview_caret();
                    }
                } else {
                    editor.visual_anchor = None;

                    // Switching back to write mode: the editor content kept its
                    // cursor, it only needs focus for the caret to show again.
                    return focus(Id::new(SOURCE_EDITOR_ID));
                }
            }
        }
        Message::LinkClicked(_uri) => {
            // TODO: open links in the default browser
        }
        Message::MovePreviewCursor(motion, count) => {
            // The caret always advances; the page only scrolls as much as
            // needed to keep the caret visible.
            if editor
                .caret
                .move_by(editor.preview_elements.elements(), motion, count)
            {
                return reveal_preview_caret();
            }
        }
        Message::MovePreviewWord(motion, count) => {
            if editor
                .caret
                .move_word(editor.preview_elements.elements(), motion, count)
            {
                return reveal_preview_caret();
            }
        }
        Message::MovePreviewJump(jump, count) => {
            if editor
                .caret
                .jump(editor.preview_elements.elements(), jump, count)
            {
                return reveal_preview_caret();
            }
        }
        Message::PreviewGPressed | Message::PreviewZPressed | Message::PreviewCountPressed(_) => {}
        Message::PreviewCancel => {
            editor.visual_anchor = None;
        }
        Message::ToggleVisualMode => {
            editor.visual_anchor = if editor.keymap.visual() {
                Some(editor.caret.position())
            } else {
                None
            };
        }
        Message::ScrollPreviewBy(y) => {
            return scroll_by(Id::new(PREVIEW_SCROLL_ID), AbsoluteOffset { x: 0.0, y });
        }
        Message::ScrollPreviewPage(page, count) => {
            return iced::advanced::widget::operate(PageScroll {
                scroll_id: Id::new(PREVIEW_SCROLL_ID),
                viewport: None,
                page,
                count,
            });
        }
        Message::ScrollPreviewCaret(placement) => {
            let scroll = match placement {
                Placement::Center => CaretScroll::Center,
                Placement::Top => CaretScroll::Top,
                Placement::Bottom => CaretScroll::Bottom,
            };

            return iced::advanced::widget::operate(RevealCaret {
                scroll_id: Id::new(PREVIEW_SCROLL_ID),
                caret_id: Id::new(PREVIEW_CARET_ID),
                viewport: None,
                caret: None,
                scroll,
            });
        }
        Message::OpenNotePopup => {
            return focus(Id::new(NOTE_EDITOR_ID));
        }
        Message::CloseNotePopup => {
            // Dismissing the popup discards the draft, so the next note
            // starts fresh instead of inheriting the escaped one.
            editor.note_text = text_editor::Content::new();
            editor.editing_comment = None;
        }
        Message::EditNote(action) => editor.note_text.perform(action),
        Message::EditActiveComment => {
            // Enter over the active comment opens it for editing, its text
            // preloaded in the popup; without one, Enter does nothing.
            if let Some(text) = editor.comments.active_text().map(str::to_owned) {
                editor.note_text = text_editor::Content::with_text(&text);
                editor.editing_comment = editor.comments.active_entry();
                editor.keymap.note(Transition::NoteOpened);

                return focus(Id::new(NOTE_EDITOR_ID));
            }
        }
        Message::EditPublish(action) => editor.comments.edit_draft(action),
        Message::AddGlobalComment => editor.comments.add_draft_as_global(),
        Message::PublishPressed => {
            // TODO: publish the comments
        }
        Message::NextComment => {
            if let Some(anchor) = editor.comments.cycle() {
                // Jump the caret to the comment and reveal it, so the mark
                // is actually in view.
                editor.visual_anchor = None;
                editor.caret.place(anchor);

                return reveal_preview_caret();
            }
        }
        Message::SaveNote => {
            if note_was_open {
                save_note(editor);
            }
        }
        Message::DeleteComment(thread, entry) => {
            editor.comments.delete(thread, entry);

            // Deleting the very comment the popup is editing closes the
            // popup with a fresh note, like dismissing it would.
            if editor.editing_comment == Some((thread, entry)) {
                editor.editing_comment = None;
                editor.note_text = text_editor::Content::new();
                editor.keymap.note(Transition::NoteClosed);
            }
        }
        Message::ResolveComment(thread) => editor.comments.resolve(thread),
        Message::ToggleSidebar => {
            editor.sidebar_override = Some(!sidebar_shown(editor));
        }
        Message::OpenFind => {
            return focus(Id::new(FIND_INPUT_ID));
        }
        Message::CloseFind => {
            // Give the source editor its focus back so its caret resumes.
            if !editor.keymap.preview() {
                return focus(Id::new(SOURCE_EDITOR_ID));
            }
        }
        Message::FindQueryChanged(query) => {
            editor.find.set_query(&query);

            return select_find_match(editor, find::Way::First);
        }
        Message::FindNext => return select_find_match(editor, find::Way::Next),
        Message::FindPrevious => return select_find_match(editor, find::Way::Previous),
        // Help owns its query and internal events. The root only composes
        // the window and restores whichever field was underneath it.
        Message::OpenHelp => return help::focus_input(),
        Message::Help(message) => {
            let closed = matches!(message, help::Message::Close);
            editor.help.update(message);

            if closed {
                if editor.keymap.find_open() {
                    return focus(Id::new(FIND_INPUT_ID));
                }
                if editor.keymap.note_open() {
                    return focus(Id::new(NOTE_EDITOR_ID));
                }
                if !editor.keymap.preview() {
                    return focus(Id::new(SOURCE_EDITOR_ID));
                }
            }
        }
        // Clicks on the card itself are swallowed so they neither close the
        // popup nor reach the preview beneath.
        Message::NoteCardPressed => {}
        Message::CommentCardPressed(thread, entry) => {
            // Clicking a card makes its comment the active one and moves the
            // cursor to the anchored element: the preview caret jumps there
            // and the page reveals it; in write mode the source cursor lands
            // on the element's source instead.
            if let Some(anchor) = editor.comments.activate(thread, entry) {
                if editor.keymap.preview() {
                    editor.visual_anchor = None;
                    editor.caret.place(anchor);

                    return reveal_preview_caret();
                }

                let source = editor.document.text();
                let element = editor.preview_elements.elements().get(anchor.element);

                if let Some(element) = element {
                    editor.document.move_to(text_editor::Cursor {
                        position: editing::position_at(&source, element.source().start),
                        selection: None,
                    });

                    return focus(Id::new(SOURCE_EDITOR_ID));
                }
            }
        }
    }

    Task::none()
}

/// Selects the current find match, live as the query is typed and as
/// navigation steps through the matches: in write mode the match becomes
/// the editor's selection, in the preview the caret jumps to the match's
/// element and the matches paint as highlights with the current one
/// distinct.
fn select_find_match(editor: &mut Editor, way: find::Way) -> Task<Message> {
    if !editor.find.is_active() {
        return Task::none();
    }

    if editor.keymap.preview() {
        let matches =
            find::preview_matches(editor.preview_elements.elements(), editor.find.query());
        let Some(index) = editor.find.select(way, matches.len()) else {
            return Task::none();
        };

        let (element, range) = matches[index].clone();
        editor.caret.place(CaretPosition {
            element,
            column: range.start,
        });

        return reveal_preview_caret();
    }

    let source = editor.document.text();
    let matches = editing::source_matches(&source, editor.find.query());
    let Some(index) = editor.find.select(way, matches.len()) else {
        return Task::none();
    };

    let editing::SourceMatch { line, columns } = matches[index].clone();
    editor.document.move_to(text_editor::Cursor {
        position: text_editor::Position {
            line,
            column: columns.start,
        },
        selection: Some(text_editor::Position {
            line,
            column: columns.end,
        }),
    });

    Task::none()
}

/// All find matches of the mode's surface — the preview's elements or
/// the source text — for the popup's live match counter.
fn find_match_count(editor: &Editor) -> usize {
    if !editor.find.is_active() {
        return 0;
    }

    if editor.keymap.preview() {
        find::preview_matches(editor.preview_elements.elements(), editor.find.query()).len()
    } else {
        editing::source_matches(&editor.document.text(), editor.find.query()).len()
    }
}

/// The find popup: a small card in the top right corner with the query
/// field, a live `n/m` match counter, and ▲/▼ buttons stepping through
/// the matches.
fn find_popup(editor: &Editor, palette: Palette) -> Element<'_, Message> {
    let total = find_match_count(editor);
    let counter = editor.find.is_active().then(|| {
        if total == 0 {
            ("no match".to_owned(), palette.red)
        } else {
            let current = editor.find.current(total).map_or(1, |index| index + 1);
            (format!("{}/{}", current, total), palette.dark_foreground)
        }
    });

    let mut card_row = row![text_input("Search…", editor.find.query())
        .id(Id::new(FIND_INPUT_ID))
        .on_input(Message::FindQueryChanged)
        .font(EDITOR_FONT)
        .size(14)
        .padding(6)
        .width(Length::Fixed(220.0))]
    .spacing(8)
    .align_y(alignment::Vertical::Center);

    if let Some((counter, color)) = counter {
        card_row = card_row.push(text(counter).font(EDITOR_FONT).size(12).color(color));
    }

    // ▲ steps back, ▼ steps forward — the mouse path of Enter and
    // Shift+Enter.
    card_row = card_row
        .push(
            button(
                text("▲")
                    .font(EDITOR_FONT)
                    .size(12)
                    .color(palette.light_foreground),
            )
            .on_press(Message::FindPrevious)
            .padding([4, 8])
            .style(move |theme, status| popup_button_style(&palette, theme, status)),
        )
        .push(
            button(
                text("▼")
                    .font(EDITOR_FONT)
                    .size(12)
                    .color(palette.light_foreground),
            )
            .on_press(Message::FindNext)
            .padding([4, 8])
            .style(move |theme, status| popup_button_style(&palette, theme, status)),
        );

    container(
        container(card_row)
            .padding(8)
            .style(move |_theme| note_card_style(&palette)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(alignment::Horizontal::Right)
    .align_y(alignment::Vertical::Top)
    .padding(16)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyboardGuardAction {
    Pass,
    Capture,
    OpenHelp,
    CloseHelp,
    CloseFind,
    CloseNote,
    CancelUnsaved,
}

fn keyboard_guard_action(keymap: Keymap, event: &iced::Event) -> KeyboardGuardAction {
    // Help owns its chord and modal keyboard policy. It sits above every
    // other modal, so its decisions have first refusal here.
    match help::event_action(keymap.help_open(), event) {
        help::EventAction::Open => return KeyboardGuardAction::OpenHelp,
        help::EventAction::Close => return KeyboardGuardAction::CloseHelp,
        help::EventAction::Capture => return KeyboardGuardAction::Capture,
        help::EventAction::Pass => {}
    }

    // While Help is open, passed events belong to its focused search field,
    // never to another modal beneath it.
    if keymap.help_open() {
        return KeyboardGuardAction::Pass;
    }

    // Do not let an active input method commit or alter pre-edit text
    // behind the unsaved-changes dialog.
    if keymap.unsaved_open() && matches!(event, iced::Event::InputMethod(_)) {
        return KeyboardGuardAction::Capture;
    }

    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        modifiers,
        repeat,
        ..
    }) = event
    else {
        return KeyboardGuardAction::Pass;
    };

    // The unsaved-changes dialog captures every key press so the editing
    // surface beneath cannot react; Escape cancels it.
    if keymap.unsaved_open() {
        return if !repeat
            && matches!(
                modified_key.as_ref(),
                keyboard::Key::Named(keyboard::key::Named::Escape)
            ) {
            KeyboardGuardAction::CancelUnsaved
        } else {
            KeyboardGuardAction::Capture
        };
    }

    if !repeat
        && !modifiers.control()
        && !modifiers.alt()
        && !modifiers.logo()
        && matches!(
            modified_key.as_ref(),
            keyboard::Key::Named(keyboard::key::Named::Escape)
        )
    {
        if keymap.find_open() {
            return KeyboardGuardAction::CloseFind;
        }
        if keymap.note_open() {
            return KeyboardGuardAction::CloseNote;
        }
    }

    KeyboardGuardAction::Pass
}

/// Wraps the complete interface and intercepts the help chord before
/// focused widgets can consume it. While help is open, every key press but
/// the closers and Tab reaches the window's search field — focus lives on
/// the overlay, so the interface underneath never reacts; closing returns
/// focus to the field that was active before it opened.
fn keyboard_guard<'a>(content: Element<'a, Message>, keymap: Keymap) -> Element<'a, Message> {
    struct KeyboardGuard<'a> {
        content: Element<'a, Message>,
        keymap: Keymap,
    }

    impl Widget<Message, Theme, Renderer> for KeyboardGuard<'_> {
        fn tag(&self) -> iced::advanced::widget::tree::Tag {
            self.content.as_widget().tag()
        }

        fn state(&self) -> iced::advanced::widget::tree::State {
            self.content.as_widget().state()
        }

        fn children(&self) -> Vec<Tree> {
            self.content.as_widget().children()
        }

        fn diff(&self, tree: &mut Tree) {
            self.content.as_widget().diff(tree);
        }

        fn size(&self) -> Size<Length> {
            self.content.as_widget().size()
        }

        fn size_hint(&self) -> Size<Length> {
            self.content.as_widget().size_hint()
        }

        fn layout(
            &mut self,
            tree: &mut Tree,
            renderer: &Renderer,
            limits: &layout::Limits,
        ) -> layout::Node {
            self.content.as_widget_mut().layout(tree, renderer, limits)
        }

        fn draw(
            &self,
            tree: &Tree,
            renderer: &mut Renderer,
            theme: &Theme,
            style: &renderer::Style,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
        ) {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        }

        fn operate(
            &mut self,
            tree: &mut Tree,
            layout: Layout<'_>,
            renderer: &Renderer,
            operation: &mut dyn Operation,
        ) {
            self.content
                .as_widget_mut()
                .operate(tree, layout, renderer, operation);
        }

        fn update(
            &mut self,
            tree: &mut Tree,
            event: &iced::Event,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            renderer: &Renderer,
            clipboard: &mut dyn Clipboard,
            shell: &mut Shell<'_, Message>,
            viewport: &Rectangle,
        ) {
            match keyboard_guard_action(self.keymap, event) {
                KeyboardGuardAction::OpenHelp => shell.publish(Message::OpenHelp),
                KeyboardGuardAction::CloseHelp => {
                    shell.publish(Message::Help(help::Message::Close))
                }
                KeyboardGuardAction::CloseFind => shell.publish(Message::CloseFind),
                KeyboardGuardAction::CloseNote => shell.publish(Message::CloseNotePopup),
                KeyboardGuardAction::CancelUnsaved => {
                    shell.publish(Message::Document(document::Message::UnsavedCancel))
                }
                KeyboardGuardAction::Capture => {}
                KeyboardGuardAction::Pass => {
                    self.content.as_widget_mut().update(
                        tree, event, layout, cursor, renderer, clipboard, shell, viewport,
                    );
                    return;
                }
            }

            shell.capture_event();
        }

        fn mouse_interaction(
            &self,
            tree: &Tree,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
            renderer: &Renderer,
        ) -> mouse::Interaction {
            self.content
                .as_widget()
                .mouse_interaction(tree, layout, cursor, viewport, renderer)
        }

        fn overlay<'b>(
            &'b mut self,
            tree: &'b mut Tree,
            layout: Layout<'b>,
            renderer: &Renderer,
            viewport: &Rectangle,
            translation: Vector,
        ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
            self.content
                .as_widget_mut()
                .overlay(tree, layout, renderer, viewport, translation)
        }
    }

    Element::new(KeyboardGuard { content, keymap })
}

/// Whether the comments sidebar is showing: the manual `Ctrl+B` override
/// wins, otherwise it appears once there are comments.
fn sidebar_shown(editor: &Editor) -> bool {
    editor
        .sidebar_override
        .unwrap_or(!editor.comments.is_empty())
}

/// Saves the note popup text as a comment — editing the comment the popup
/// was opened for, anchoring to the visual selection when there is one,
/// or anchoring at the preview caret — then closes the popup with a fresh
/// note. Empty notes are discarded.
fn save_note(editor: &mut Editor) {
    let text = editor.note_text.text();

    if let Some((thread, entry)) = editor.editing_comment.take() {
        editor.comments.activate(thread, entry);
        editor.comments.edit_active(&text);
    } else if let Some(anchor) = editor.visual_anchor {
        // The popup opened over a visual selection: the comment anchors to
        // exactly the selected text.
        editor
            .comments
            .save_selection(&text, Span::new(anchor, editor.caret.position()));
    } else {
        editor.comments.save(&text, editor.caret.position());
    }

    editor.note_text = text_editor::Content::new();
}

/// Measures the preview scrollable and the caret element in the widget tree,
/// then scrolls the minimum amount needed to bring the caret back into
/// view, or the amount that places the caret as `zz`/`zt`/`zb` ask.
///
/// The page keeps its position once the caret is visible — unlike a
/// proportional scroll, the caret can never outrun the end of the page.
fn reveal_preview_caret() -> Task<Message> {
    iced::advanced::widget::operate(RevealCaret {
        scroll_id: Id::new(PREVIEW_SCROLL_ID),
        caret_id: Id::new(PREVIEW_CARET_ID),
        viewport: None,
        caret: None,
        scroll: CaretScroll::Reveal,
    })
}

/// How a [`RevealCaret`] operation scrolls the caret into place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaretScroll {
    /// The minimum scroll that brings the caret back into view.
    Reveal,
    /// `zz` — the caret lands in the middle of the viewport.
    Center,
    /// `zt` — the caret lands at the top of the viewport.
    Top,
    /// `zb` — the caret lands at the bottom of the viewport.
    Bottom,
}

struct RevealCaret {
    scroll_id: Id,
    caret_id: Id,
    viewport: Option<(Rectangle, Vector)>,
    caret: Option<Rectangle>,
    scroll: CaretScroll,
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

        let delta = match self.scroll {
            CaretScroll::Reveal => {
                if caret_top < viewport.y + CARET_MARGIN {
                    caret_top - viewport.y - CARET_MARGIN
                } else if caret_bottom > viewport_bottom - CARET_MARGIN {
                    caret_bottom - viewport_bottom + CARET_MARGIN
                } else {
                    // Already visible; keep the page where it is.
                    return Outcome::None;
                }
            }
            // `zz` — the caret lands in the middle of the viewport.
            CaretScroll::Center => {
                let caret_middle = caret_top + caret.height / 2.0;
                caret_middle - (viewport.y + viewport.height / 2.0)
            }
            // `zt` — the caret lands at the top of the viewport.
            CaretScroll::Top => caret_top - viewport.y - CARET_MARGIN,
            // `zb` — the caret lands at the bottom of the viewport.
            CaretScroll::Bottom => caret_bottom - viewport_bottom + CARET_MARGIN,
        };

        if delta.abs() < 0.5 {
            // Already there; keep the page where it is.
            return Outcome::None;
        }

        Outcome::Some(Message::ScrollPreviewBy(delta))
    }
}

/// Measures the preview scrollable's viewport, then scrolls by whole or
/// half pages — `Ctrl+D`/`Ctrl+U` and `PageDown`/`PageUp`, count times.
struct PageScroll {
    scroll_id: Id,
    viewport: Option<Rectangle>,
    page: Page,
    count: usize,
}

impl Operation<Message> for PageScroll {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        _translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if Some(&self.scroll_id) == id {
            self.viewport = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        let Some(viewport) = self.viewport else {
            return Outcome::None;
        };

        // Like vim's scroll step, a page keeps a little context visible.
        let full = (viewport.height - 2.0 * CARET_MARGIN).max(1.0);
        let half = full / 2.0;

        let (distance, sign) = match self.page {
            Page::HalfDown => (half, 1.0),
            Page::HalfUp => (half, -1.0),
            Page::FullDown => (full, 1.0),
            Page::FullUp => (full, -1.0),
        };

        Outcome::Some(Message::ScrollPreviewBy(
            sign * distance * self.count.max(1) as f32,
        ))
    }
}

fn editor_style(
    palette: &Palette,
    _theme: &Theme,
    _status: text_editor::Status,
) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(palette.background),
        border: Border::default(),
        placeholder: palette.foreground,
        value: palette.foreground,
        selection: palette.selection,
    }
}

fn comment_colors(palette: &Palette) -> interactive_text::CommentColors {
    interactive_text::CommentColors {
        // Open comments keep the palette's warning/annotation color, while
        // the active comment uses the same accent as its sidebar card.
        commented_bar: palette.tint(palette.yellow, 0.75),
        active_bar: palette.accent,
        active_tint: palette.tint(palette.accent, 0.09),
        commented_span_tint: palette.tint(palette.yellow, 0.22),
    }
}

fn markdown_style(palette: &Palette) -> markdown::Style {
    let theme = if palette.light {
        &Theme::Light
    } else {
        &Theme::Dark
    };

    markdown::Style {
        font: EDITOR_FONT,
        inline_code_font: EDITOR_FONT,
        code_block_font: EDITOR_FONT,
        ..markdown::Style::from(theme)
    }
}

fn icon_button_style(palette: &Palette, _theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => palette.light_foreground,
            _ => palette.foreground,
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
    /// Claims the next numbered element per rendered item from the map —
    /// the viewer never counts itself, so the numbering parity between the
    /// parse walk and the viewer lives in the preview module alone.
    claims: preview::Claims<'a>,
    focused_element: usize,
    caret_column: usize,
    /// The `(anchor, caret)` endpoints of the visual-mode selection.
    visual: Option<(CaretPosition, CaretPosition)>,
    /// Saved comments, to know which elements carry one.
    comments: &'a Comments,
    /// The find popup's query; its matches paint as highlights.
    find_query: &'a str,
    /// The current find match, as the element and range it lives in.
    current_match: Option<(usize, std::ops::Range<usize>)>,
    /// The color the caret and decorations paint with — the palette's
    /// foreground.
    text_color: Color,
    /// The current theme's comment bar and tint colors.
    comment_colors: interactive_text::CommentColors,
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

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        // The map numbers code blocks like any element; if they ever
        // disagree, fall back to the plain, non-interactive look.
        let Some((element, preview_element)) = self.claims.claim() else {
            return markdown::code_block(settings, lines, Message::LinkClicked);
        };

        let decorations = self.decorations(element, preview_element);

        // The code block keeps the default look — dark surface, inset —
        // with the interactive code inside instead of the plain lines.
        container(interactive_text::code(
            settings,
            preview_element.text(),
            decorations.selection,
            decorations.caret,
            decorations.id,
            decorations.commented,
            decorations.active_comment,
            decorations.comment_span,
            self.comment_colors,
            self.text_color,
            decorations.find,
        ))
        .width(Length::Fill)
        .padding(settings.code_size / 4.0)
        .class(Theme::code_block())
        .into()
    }
}

impl<'a> PreviewViewer<'a> {
    /// The decorations the claimed `element` carries: its slice of the
    /// visual selection, the caret when focused, its comment mark and
    /// anchored span, and its find matches.
    fn decorations(
        &self,
        element: usize,
        preview_element: &preview::PreviewElement,
    ) -> Decorations {
        let focused = self.focused_element == element;

        Decorations {
            selection: self.visual.and_then(|(anchor, caret)| {
                preview::element_selection(anchor, caret, element, preview_element.len())
            }),
            caret: focused.then_some(self.caret_column),
            id: focused.then(|| Id::new(PREVIEW_CARET_ID)),
            commented: matches!(
                self.comments.mark_for(element, preview_element.len()),
                Mark::Commented | Mark::Active
            ),
            active_comment: self.comments.mark_for(element, preview_element.len()) == Mark::Active,
            comment_span: self
                .comments
                .anchor_selection_for(element, preview_element.len()),
            find: interactive_text::FindHighlights {
                matches: if self.find_query.is_empty() {
                    Vec::new()
                } else {
                    editing::matches_in(preview_element.text(), self.find_query)
                },
                current: match self.current_match {
                    Some((match_element, ref range)) if match_element == element => {
                        Some(range.clone())
                    }
                    _ => None,
                },
            },
        }
    }

    fn text_element(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        // The map numbers elements exactly like the viewer numbers items;
        // if they ever disagree the item renders plainly, without caret,
        // selection, or comment mark.
        let Some((element, preview_element)) = self.claims.claim() else {
            return interactive_text::paragraph(
                settings,
                text,
                None,
                None,
                None,
                false,
                false,
                None,
                self.comment_colors,
                self.text_color,
                interactive_text::FindHighlights::none(),
            );
        };

        let decorations = self.decorations(element, preview_element);

        interactive_text::paragraph(
            settings,
            text,
            decorations.selection,
            decorations.caret,
            decorations.id,
            decorations.commented,
            decorations.active_comment,
            decorations.comment_span,
            self.comment_colors,
            self.text_color,
            decorations.find,
        )
    }
}

/// The interactive decorations of one preview element: its slice of the
/// visual-mode selection, the caret when it is the focused element, its
/// comment mark and anchored span, and its find highlights.
struct Decorations {
    selection: Option<std::ops::Range<usize>>,
    caret: Option<usize>,
    id: Option<Id>,
    commented: bool,
    active_comment: bool,
    /// The selected-text span a comment was written for, when its anchor
    /// is a span.
    comment_span: Option<std::ops::Range<usize>>,
    find: interactive_text::FindHighlights,
}

/// Stable bottom-to-top ordering of the root editing surface and modal
/// layers. The view consumes this order directly when constructing its
/// stack, so integration tests can assert the same composition contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootLayer {
    EditingSurface,
    Find,
    Unsaved,
    Help,
}

fn root_layer_order(editor: &Editor) -> Vec<RootLayer> {
    let mut layers = vec![RootLayer::EditingSurface];

    if editor.keymap.find_open() {
        layers.push(RootLayer::Find);
    }
    if editor.keymap.unsaved_open() && editor.document.pending_action().is_some() {
        layers.push(RootLayer::Unsaved);
    }
    if editor.keymap.help_open() {
        layers.push(RootLayer::Help);
    }

    layers
}

fn view(editor: &Editor) -> Element<'_, Message> {
    let palette = editor.palette;
    let position = editor.caret.position();
    let visual = editor.visual_anchor.map(|anchor| (anchor, position));

    // The current find match, resolved against the live elements so the
    // viewer can paint it in its distinct color.
    let find_matches =
        find::preview_matches(editor.preview_elements.elements(), editor.find.query());
    let current_match = editor
        .find
        .current(find_matches.len())
        .and_then(|index| find_matches.get(index).cloned());

    let base_area: Element<'_, Message> = if editor.keymap.preview() {
        scrollable(
            container(markdown::view_with(
                editor.markdown.items(),
                markdown::Settings::with_text_size(20.0, markdown_style(&palette)),
                &PreviewViewer {
                    claims: editor.preview_elements.claims(),
                    focused_element: position.element,
                    caret_column: position.column,
                    visual,
                    comments: &editor.comments,
                    find_query: editor.find.query(),
                    current_match,
                    text_color: palette.foreground,
                    comment_colors: comment_colors(&palette),
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
        text_editor(editor.document.content())
            .id(Id::new(SOURCE_EDITOR_ID))
            .on_action(|action| Message::Document(document::Message::Edit(action)))
            .font(EDITOR_FONT)
            .size(20)
            .height(Length::Fill)
            .padding(0)
            .line_height(1.8)
            .highlight_with::<highlight::MarkdownMarkers>(
                editor.find.query().to_owned(),
                highlight::format,
            )
            .style(move |theme, status| editor_style(&palette, theme, status))
            .into()
    };

    // The note popup floats above the editing area; the backdrop closes it
    // on click and shields the area beneath from events. The stack keeps
    // the editing surface as its base layer whether or not the popup is
    // open, so toggling the popup never rebuilds the tree beneath it and
    // the preview keeps its scroll position and caret.
    let mut editing_stack = stack![base_area];

    if editor.keymap.note_open() {
        editing_stack = editing_stack.push(note_popup(editor));
    }

    let editing_area: Element<'_, Message> = editing_stack.into();

    let open_button = tooltip(
        button(
            canvas(OpenFileIcon)
                .width(Length::Fixed(ICON_SIZE))
                .height(Length::Fixed(ICON_SIZE)),
        )
        .on_press(Message::Document(document::Message::OpenRequested))
        .width(Length::Fixed(ICON_BUTTON_SIZE))
        .height(Length::Fixed(ICON_BUTTON_SIZE))
        .padding(0)
        .style(move |theme, status| icon_button_style(&palette, theme, status)),
        container(text("Ctrl + o, Open").font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    // The preview toggle carries two icons: the eye invites switching to
    // the preview while writing, and the pencil switches back to writing
    // while previewing. One button, one shortcut — `Ctrl+P`.
    let toggle_label = if editor.keymap.preview() {
        "Ctrl + p, Write"
    } else {
        "Ctrl + p, Preview"
    };

    let icon_canvas: Element<'_, Message> = if editor.keymap.preview() {
        canvas(WriteIcon)
            .width(Length::Fixed(ICON_SIZE))
            .height(Length::Fixed(ICON_SIZE))
            .into()
    } else {
        canvas(PreviewIcon)
            .width(Length::Fixed(ICON_SIZE))
            .height(Length::Fixed(ICON_SIZE))
            .into()
    };

    let toggle_button = tooltip(
        button(icon_canvas)
            .on_press(Message::TogglePreview)
            .width(Length::Fixed(ICON_BUTTON_SIZE))
            .height(Length::Fixed(ICON_BUTTON_SIZE))
            .padding(0)
            .style(move |theme, status| icon_button_style(&palette, theme, status)),
        container(text(toggle_label).font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    let save_button = tooltip(
        button(
            canvas(SaveIcon)
                .width(Length::Fixed(ICON_SIZE))
                .height(Length::Fixed(ICON_SIZE)),
        )
        .on_press(Message::Document(document::Message::SaveRequested))
        .width(Length::Fixed(ICON_BUTTON_SIZE))
        .height(Length::Fixed(ICON_BUTTON_SIZE))
        .padding(0)
        .style(move |theme, status| icon_button_style(&palette, theme, status)),
        container(text("Ctrl + s, Save").font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    // The main column: top margin, the writing area, and the bottom
    // controls. It fills the space between the window's 5% side margins,
    // which stay symmetric whether or not the sidebar is showing.
    let main = column![
        Space::new()
            .width(Length::Fill)
            .height(Length::FillPortion(1)),
        container(editing_area)
            .width(Length::Fill)
            .height(Length::FillPortion(8)),
        {
            let mut controls: Vec<Element<'_, Message>> =
                vec![open_button.into(), save_button.into()];

            if !editor.keymap.preview_only() {
                controls.push(toggle_button.into());
            }

            controls.push(mode_badge(editor));

            controls.push(Space::new().width(Length::Fill).height(Length::Fill).into());

            row(controls)
                .width(Length::Fill)
                .height(Length::FillPortion(1))
                .spacing(4)
                .align_y(alignment::Vertical::Bottom)
        },
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    // The editor keeps its symmetric 5% margins; the comments sidebar sits
    // to the right of them, flush with the window's edge. When it is
    // hidden, a slim rail marks where it collapsed to and brings it back.
    let main_with_margins = row![
        Space::new()
            .width(Length::FillPortion(5))
            .height(Length::Fill),
        container(main)
            .width(Length::FillPortion(90))
            .height(Length::Fill),
        Space::new()
            .width(Length::FillPortion(5))
            .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let sidebar_area: Element<'_, Message> = if sidebar_shown(editor) {
        container(comments_sidebar(editor, palette))
            .width(Length::Fixed(SIDEBAR_WIDTH))
            .height(Length::Fill)
            .into()
    } else {
        collapsed_sidebar_rail(editor, palette)
    };

    let content: Element<'_, Message> = container(
        row![container(main_with_margins)
            .width(Length::Fill)
            .height(Length::Fill)]
        .push(sidebar_area)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme| background_style(&palette))
    .into();

    // Root layers are pushed in their stable bottom-to-top order. Help is
    // topmost; unsaved changes remains beneath it and above find and the
    // persistent editing surface, preserving the trees and focus below.
    let mut layers = stack![content];
    for layer in root_layer_order(editor).into_iter().skip(1) {
        layers = match layer {
            RootLayer::EditingSurface => unreachable!("the editing surface is always first"),
            RootLayer::Find => layers.push(find_popup(editor, palette)),
            RootLayer::Unsaved => {
                let action = editor
                    .document
                    .pending_action()
                    .expect("a visible unsaved modal has a pending action");
                layers.push(document::unsaved_view::view(action, palette).map(Message::Document))
            }
            RootLayer::Help => layers.push(help::view(&editor.help, palette).map(Message::Help)),
        };
    }

    keyboard_guard(layers.into(), editor.keymap)
}

/// The width of the comments sidebar on the right.
const SIDEBAR_WIDTH: f32 = 340.0;

/// The collapsed sidebar's rail at the window's right edge: the visual
/// indication that a sidebar exists and where it went. Its button expands
/// the sidebar (`Ctrl+B` too), and the count under the icon shows how many
/// comments wait inside.
fn collapsed_sidebar_rail(editor: &Editor, palette: Palette) -> Element<'_, Message> {
    let expand_button = button(
        column![
            canvas(CommentsIcon)
                .width(Length::Fixed(ICON_SIZE))
                .height(Length::Fixed(ICON_SIZE)),
            text(if editor.comments.is_empty() {
                String::new()
            } else {
                editor.comments.len().to_string()
            })
            .font(EDITOR_FONT)
            .size(11)
            .color(if editor.comments.is_empty() {
                palette.dark_foreground
            } else {
                palette.accent
            }),
        ]
        .spacing(2)
        .align_x(alignment::Horizontal::Center),
    )
    .on_press(Message::ToggleSidebar)
    .width(Length::Fill)
    .padding([10, 4])
    .style(move |theme, status| rail_button_style(palette, theme, status));

    tooltip(
        container(
            container(expand_button)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(alignment::Vertical::Center),
        )
        .width(Length::Fixed(44.0))
        .height(Length::Fill)
        .padding(0)
        .style(move |_theme| sidebar_rail_style(palette)),
        container(
            text(format!("Ctrl + b, Comments ({})", editor.comments.len()))
                .font(EDITOR_FONT)
                .size(12),
        )
        .padding([4, 8])
        .style(tooltip_style),
        iced::widget::tooltip::Position::Left,
    )
    .into()
}

/// The comments sidebar: a full-height panel with a scrollable tree of
/// comment threads — the root quoting the Markdown source it was written
/// for, replies indented under it — a resolved-history section below, and
/// a free text field with Add and Publish buttons at the bottom.
fn comments_sidebar<'a>(editor: &'a Editor, palette: Palette) -> Element<'a, Message> {
    let source = editor.document.text();
    let cards = editor
        .comments
        .cards(&source, editor.preview_elements.elements());

    let open: Vec<Element<'_, Message>> = cards
        .iter()
        .filter(|card| !card.resolved)
        .map(|card| comment_card(card.clone(), palette))
        .collect();
    let resolved: Vec<Element<'_, Message>> = cards
        .iter()
        .filter(|card| card.resolved)
        .map(|card| comment_card(card.clone(), palette))
        .collect();

    let mut list: Vec<Element<'_, Message>> = Vec::new();

    if open.is_empty() && resolved.is_empty() {
        list.push(
            text("No comments yet — press c in the preview or write one below.")
                .font(EDITOR_FONT)
                .size(13)
                .color(palette.dark_foreground)
                .into(),
        );
    }

    list.extend(open);

    // Resolved threads are the history section — kept, dimmed, reopenable.
    if !resolved.is_empty() {
        list.push(
            text(format!("RESOLVED ({})", resolved.len()))
                .font(EDITOR_FONT)
                .size(13)
                .color(palette.dark_foreground)
                .into(),
        );
        list.extend(resolved);
    }

    container(
        column![
            container(
                text(format!("COMMENTS ({})", editor.comments.len()))
                    .font(EDITOR_FONT)
                    .size(13)
                    .color(palette.light_foreground),
            )
            .padding(iced::Padding {
                top: 4.0,
                ..iced::Padding::new(0.0)
            }),
            scrollable(column(list).spacing(8).width(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::hidden(),
                )),
            container(
                column![
                    text("Write a comment…")
                        .font(EDITOR_FONT)
                        .size(13)
                        .color(palette.dark_foreground),
                    text_editor(editor.comments.draft())
                        .on_action(Message::EditPublish)
                        .font(EDITOR_FONT)
                        .size(16)
                        .height(Length::Fixed(72.0))
                        .padding(6)
                        .style(move |theme, status| {
                            publish_editor_style(&palette, theme, status)
                        }),
                ]
                .spacing(4)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(6)
            .style(move |_theme| publish_field_style(&palette)),
            row![
                button(
                    text("Add")
                        .font(EDITOR_FONT)
                        .size(15)
                        .color(palette.foreground),
                )
                .on_press(Message::AddGlobalComment)
                .width(Length::Fill)
                .padding([6, 12])
                .style(move |_theme, status| add_button_style(&palette, status)),
                button(
                    text("Publish")
                        .font(EDITOR_FONT)
                        .size(15)
                        .color(palette.foreground),
                )
                .on_press(Message::PublishPressed)
                .width(Length::Fill)
                .padding([6, 12])
                .style(move |_theme, status| publish_button_style(&palette, status)),
            ]
            .spacing(8)
            .width(Length::Fill),
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
    .style(move |_theme| sidebar_style(&palette))
    .into()
}

/// One comment card: the root of its thread quotes the anchored source (or
/// carries the Global label), replies indent under it. Every card carries
/// its resolve toggle and delete; the active comment glows.
fn comment_card(card: comments::CommentCard, palette: Palette) -> Element<'static, Message> {
    let thread = card.thread;
    let entry = card.entry;

    // Anchored comments quote their element's source; global comments show
    // their label instead. Only the thread's root carries it.
    let caption = card
        .label
        .map(str::to_owned)
        .unwrap_or_else(|| card.quote.clone());

    let mut body = column![].spacing(4);

    if card.first && !caption.is_empty() {
        body = body.push(
            text(if card.resolved {
                format!("✓ {caption}")
            } else {
                caption
            })
            .font(EDITOR_FONT)
            .size(13)
            .color(if card.active {
                palette.accent
            } else {
                palette.dark_foreground
            }),
        );
    }

    // The comment text, with its edit count when it has history.
    let mut text_row =
        row![text(card.text.clone())
            .font(EDITOR_FONT)
            .size(15)
            .color(if card.resolved {
                palette.dark_foreground
            } else {
                palette.foreground
            }),]
        .align_y(alignment::Vertical::Top);

    if card.history > 0 {
        text_row = text_row.push(
            text(format!("(edited ×{})", card.history))
                .font(EDITOR_FONT)
                .size(11)
                .color(palette.dark_foreground),
        );
    }

    body = body.push(text_row);

    // Per-card actions: resolve or reopen the thread, delete the comment.
    body = body.push(
        row![
            button(
                text(if card.resolved {
                    "↺ Reopen"
                } else {
                    "✓ Resolve"
                })
                .font(EDITOR_FONT)
                .size(11)
                .color(palette.light_foreground),
            )
            .on_press(Message::ResolveComment(thread))
            .padding([2, 6])
            .style(move |theme, status| card_button_style(palette, theme, status)),
            button(
                text("× Delete")
                    .font(EDITOR_FONT)
                    .size(11)
                    .color(palette.light_foreground),
            )
            .on_press(Message::DeleteComment(thread, entry))
            .padding([2, 6])
            .style(move |theme, status| card_button_style(palette, theme, status)),
        ]
        .spacing(4),
    );

    // The card is a button: clicking it activates its comment and moves the
    // cursor to the anchored element. Replies indent by their depth.
    let card_element: Element<'_, Message> = mouse_area(
        container(body)
            .padding(8)
            .width(Length::Fill)
            .style(move |_theme| comment_card_style(&palette, card.active, card.resolved)),
    )
    .on_press(Message::CommentCardPressed(thread, entry))
    .into();

    if card.depth == 0 {
        card_element
    } else {
        container(card_element)
            .padding(iced::Padding {
                left: 14.0 * card.depth as f32,
                ..iced::Padding::new(0.0)
            })
            .width(Length::Fill)
            .into()
    }
}

fn sidebar_style(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.dark_background)),
        border: Border {
            color: palette.muted,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

/// The collapsed sidebar's rail: a slim strip at the window's right edge
/// marking where the sidebar went.
fn sidebar_rail_style(palette: Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.dark_background)),
        border: Border {
            color: palette.muted,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn rail_button_style(palette: Palette, _theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(
                Palette::lightened(palette.darker_background, 0.15),
            )),
            _ => None,
        },
        ..Default::default()
    }
}

/// A comment card: a dark surface with the active comment glowing in the
/// accent color; resolved history dims to the background.
fn comment_card_style(palette: &Palette, active: bool, resolved: bool) -> container::Style {
    let (background, border) = if active {
        (
            Background::Color(palette.tint(palette.accent, 0.1)),
            palette.accent,
        )
    } else if resolved {
        (
            Background::Color(palette.dark_background),
            palette.dark_foreground,
        )
    } else {
        (Background::Color(palette.darker_background), palette.muted)
    };

    container::Style {
        background: Some(background),
        border: Border {
            color: border,
            width: if active { 1.5 } else { 1.0 },
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The quiet inline buttons of a comment card: resolve and delete.
fn card_button_style(palette: Palette, _theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(
                Palette::lightened(palette.darker_background, 0.25),
            )),
            _ => None,
        },
        border: Border {
            color: palette.muted,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The publish text field stands out with a lighter surface than the
/// comment cards and a clearly visible border.
fn publish_field_style(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.lighter_background)),
        border: Border {
            color: palette.light_foreground,
            width: 1.5,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The editor inside the publish field keeps its own light surface so the
/// two nested boxes read as one input control.
fn publish_editor_style(
    palette: &Palette,
    _theme: &Theme,
    _status: text_editor::Status,
) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Palette::lightened(palette.lighter_background, 0.18)),
        border: Border::default(),
        placeholder: palette.foreground,
        value: palette.foreground,
        selection: Palette::lightened(palette.selection, 0.15),
    }
}

/// The Add button sits beside Publish as the quieter, secondary action:
/// it files the draft as a global comment.
fn add_button_style(palette: &Palette, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(
                Palette::lightened(palette.darker_background, 0.3),
            )),
            _ => Some(Background::Color(palette.darker_background)),
        },
        border: Border {
            color: match status {
                button::Status::Hovered | button::Status::Pressed => palette.light_foreground,
                _ => palette.muted,
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The Publish button spans the sidebar width and reads as the primary
/// action of the panel.
fn publish_button_style(palette: &Palette, status: button::Status) -> button::Style {
    let base = Palette::darkened(palette.blue, 0.55);
    let hover = Palette::darkened(palette.blue, 0.4);

    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(hover)),
            _ => Some(Background::Color(base)),
        },
        border: Border {
            color: Palette::lightened(palette.blue, 0.2),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// A small badge naming the current mode, placed next to the open icon in
/// the bottom bar. Visual mode is highlighted with the selection blue so
/// the active selection state is obvious at a glance; the note mode with
/// the comment amber. Badge and keys read the same representation: the
/// keymap's mode.
fn mode_badge(editor: &Editor) -> Element<'_, Message> {
    let mode = editor.keymap.mode();
    let palette = editor.palette;

    let (label, color) = match mode {
        Mode::Visual => ("VISUAL", palette.blue),
        Mode::Note => ("NOTE", palette.yellow),
        Mode::Find => ("FIND", palette.orange),
        Mode::View => ("VIEW", palette.light_foreground),
        Mode::Write => ("WRITE", palette.light_foreground),
    };

    // A pending count shows beside the mode, like vim's cmdline — the `3`
    // of a `3j` waiting for its motion.
    let label = if editor.keymap.pending_count() > 0 {
        format!("{} {}", editor.keymap.pending_count(), label)
    } else {
        label.to_owned()
    };

    container(text(label).font(EDITOR_FONT).size(12).color(color))
        // The extra bottom padding pushes the label a few pixels up, level
        // with the icon glyphs beside it instead of below them.
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 4.0,
            left: 8.0,
        })
        .height(Length::Fixed(ICON_BUTTON_SIZE))
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

fn background_style(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette.background)),
        ..Default::default()
    }
}

/// The note popup: a translucent backdrop with a centered card holding a
/// text area and a button. Clicking the backdrop closes the popup; clicks
/// on the card are swallowed. Opened over the active comment it edits that
/// comment instead of writing a fresh one — its title says so, the previous
/// versions show as history, and Delete removes the comment.
fn note_popup(editor: &Editor) -> Element<'_, Message> {
    let palette = editor.palette;
    let editing = editor.editing_comment.is_some();

    let mut card_body = column![].spacing(12);

    card_body = card_body.push(if editing {
        text("Editing comment — Ctrl+S saves, Esc discards")
            .font(EDITOR_FONT)
            .size(12)
            .color(palette.yellow)
    } else {
        text("Note")
            .font(EDITOR_FONT)
            .size(12)
            .color(palette.light_foreground)
    });

    // The comment's history: the texts this comment replaced, newest
    // first, dimmed above the editor.
    if editing {
        for previous in editor.comments.active_history().iter().rev() {
            card_body = card_body.push(
                text(format!("— {previous}"))
                    .font(EDITOR_FONT)
                    .size(12)
                    .color(palette.dark_foreground),
            );
        }
    }

    card_body = card_body.push(
        text_editor(&editor.note_text)
            .id(Id::new(NOTE_EDITOR_ID))
            .on_action(Message::EditNote)
            .font(EDITOR_FONT)
            .size(20)
            .height(Length::Fixed(160.0))
            .padding(8)
            .style(move |theme, status| editor_style(&palette, theme, status)),
    );

    let mut buttons = row![button(
        text("Close")
            .font(EDITOR_FONT)
            .size(14)
            .color(palette.light_foreground),
    )
    .on_press(Message::CloseNotePopup)
    .padding([6, 12])
    .style(move |theme, status| popup_button_style(&palette, theme, status))]
    .width(Length::Fill);

    // Deleting from the edit popup removes the comment outright.
    if let Some((thread, entry)) = editor.editing_comment {
        buttons = buttons.push(
            button(text("Delete").font(EDITOR_FONT).size(14).color(palette.red))
                .on_press(Message::DeleteComment(thread, entry))
                .padding([6, 12])
                .style(move |theme, status| popup_button_style(&palette, theme, status)),
        );
    }

    buttons = buttons.push(Space::new().width(Length::Fill)).push(
        button(
            text("Save")
                .font(EDITOR_FONT)
                .size(14)
                .color(palette.foreground),
        )
        .on_press(Message::SaveNote)
        .padding([6, 12])
        .style(move |theme, status| popup_button_style(&palette, theme, status)),
    );

    card_body = card_body.push(buttons);

    let card = mouse_area(
        container(card_body)
            .width(Length::Fixed(440.0))
            .padding(16)
            .style(move |_theme| note_card_style(&palette)),
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

fn note_card_style(palette: &Palette) -> container::Style {
    container::Style {
        background: Some(Background::Color(Palette::lightened(
            palette.dark_background,
            0.08,
        ))),
        border: Border {
            color: palette.muted,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn popup_button_style(palette: &Palette, _theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(
                Palette::lightened(palette.darker_background, 0.25),
            )),
            _ => None,
        },
        border: Border {
            color: palette.muted,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn boot(args: &Args) -> (Editor, Task<Message>) {
    let contents =
        args.path.as_ref().map(
            |path| match document::io::read(std::path::Path::new(path)) {
                Ok(contents) => contents,
                Err(error) => {
                    eprintln!("agmawrite: {error}");
                    std::process::exit(1);
                }
            },
        );

    let mut editor = Editor {
        document: document::State::new(
            contents.as_deref().unwrap_or_default(),
            args.path.as_ref().map(std::path::PathBuf::from),
        ),
        markdown: markdown::Content::new(),
        keymap: Keymap::new(args.preview),
        caret: Caret::new(),
        preview_elements: ElementMap::default(),
        visual_anchor: None,
        note_text: text_editor::Content::new(),
        comments: Comments::new(),
        sidebar_override: None,
        find: find::Find::new(),
        help: help::Help::new(),
        editing_comment: None,
        palette: Palette::current(),
    };
    synchronize_source(&mut editor, SourceSynchronization::InitialLoad);

    let task = if args.preview {
        Task::none()
    } else {
        focus_next()
    };

    (editor, task)
}

fn theme(editor: &Editor) -> Theme {
    if editor.palette.light {
        Theme::Light
    } else {
        Theme::Dark
    }
}

/// Parses `args` and runs the agmawrite application.
pub fn run(args: impl IntoIterator<Item = String>) -> iced::Result {
    let args = match cli::parse_args(args.into_iter().collect()) {
        Ok(ParseOutcome::Run(args)) => args,
        Ok(ParseOutcome::Help) => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
        Err(error) => {
            eprintln!("agmawrite: {error}\n\n{}", cli::USAGE);
            std::process::exit(1);
        }
    };

    application(move || boot(&args), update, view)
        .title("agmawrite")
        .theme(theme)
        .subscription(subscription)
        .exit_on_close_request(false)
        .font(include_bytes!("../fonts/iAWriterMonoS-Regular.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Italic.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Bold.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-BoldItalic.ttf").as_slice())
        .run()
}

#[cfg(test)]
mod tests {
    use super::comments::{Comments, Mark};
    use super::find;
    use super::keymap::{Keymap, Mode, Transition};
    use super::preview::{Caret, CaretPosition, ElementMap};
    use super::theme::Palette;
    use super::{
        keyboard_guard_action, root_layer_order, synchronize_source, update, Editor,
        KeyboardGuardAction, Message, RootLayer, SourceSynchronization,
    };

    fn editor_at(contents: &str, position: CaretPosition) -> Editor {
        let mut keymap = Keymap::new(false);
        keymap.note(Transition::PreviewToggled);

        let mut caret = Caret::new();
        caret.place(position);

        Editor {
            document: super::document::State::new(contents, None),
            markdown: iced::widget::markdown::Content::parse(contents),
            keymap,
            caret,
            preview_elements: ElementMap::parse(contents),
            visual_anchor: None,
            note_text: iced::widget::text_editor::Content::new(),
            comments: Comments::new(),
            sidebar_override: None,
            find: find::Find::new(),
            help: super::help::Help::new(),
            editing_comment: None,
            palette: Palette::default(),
        }
    }

    fn assert_projection_matches(editor: &Editor, contents: &str) {
        let expected_elements = ElementMap::parse(contents);
        assert_eq!(
            editor.preview_elements.elements(),
            expected_elements.elements()
        );

        let expected_markdown = iced::widget::markdown::Content::parse(contents);
        assert_eq!(
            format!("{:?}", editor.markdown.items()),
            format!("{:?}", expected_markdown.items())
        );
    }

    #[test]
    fn initial_load_policy_builds_projection_and_clears_document_state() {
        let contents = "# Fresh\n\nbody";
        let mut editor = editor_at(
            contents,
            CaretPosition {
                element: 1,
                column: 2,
            },
        );
        editor.markdown = iced::widget::markdown::Content::parse("stale");
        editor.preview_elements = ElementMap::parse("stale");
        editor.visual_anchor = Some(CaretPosition {
            element: 0,
            column: 1,
        });
        editor.comments.save("old note", editor.caret.position());
        editor.note_text = iced::widget::text_editor::Content::with_text("draft");
        editor.editing_comment = Some((0, 0));

        let reveal = synchronize_source(&mut editor, SourceSynchronization::InitialLoad);

        assert!(!reveal);
        assert_projection_matches(&editor, contents);
        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 0,
                column: 0,
            }
        );
        assert!(editor.visual_anchor.is_none());
        assert!(editor.comments.is_empty());
        assert_eq!(editor.note_text.text(), "");
        assert!(editor.editing_comment.is_none());
    }

    #[test]
    fn file_load_policy_replaces_source_and_resets_document_bound_state() {
        let mut editor = editor_at(
            "old",
            CaretPosition {
                element: 0,
                column: 2,
            },
        );
        editor.visual_anchor = Some(CaretPosition {
            element: 0,
            column: 1,
        });
        editor.comments.save("old note", editor.caret.position());
        editor.note_text = iced::widget::text_editor::Content::with_text("draft");
        editor.editing_comment = Some((0, 0));
        let path = std::path::PathBuf::from("/tmp/task-5-loaded.md");
        let contents = "# Loaded\n\nnew body";

        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::OpenLoaded(Some((
                path.clone(),
                contents.to_owned(),
            )))),
        );

        assert_eq!(editor.document.path(), Some(path.as_path()));
        assert_eq!(editor.document.text(), contents);
        assert!(!editor.document.is_modified());
        assert_projection_matches(&editor, contents);
        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 0,
                column: 0,
            }
        );
        assert!(editor.visual_anchor.is_none());
        assert!(editor.comments.is_empty());
        assert_eq!(editor.note_text.text(), "");
        assert!(editor.editing_comment.is_none());
    }

    #[test]
    fn external_replacement_policy_preserves_source_cursor_and_live_context() {
        use iced::widget::text_editor::{Cursor, Position};

        let old_contents = "# Old\n\nold body";
        let mut editor = editor_at(
            old_contents,
            CaretPosition {
                element: 1,
                column: 3,
            },
        );
        let path = std::env::temp_dir().join(format!(
            "agmawrite-external-{}-{:?}.md",
            std::process::id(),
            iced::window::Id::unique()
        ));
        std::fs::write(&path, old_contents).unwrap();
        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::OpenLoaded(Some((
                path.clone(),
                old_contents.to_owned(),
            )))),
        );
        editor.caret.place(CaretPosition {
            element: 1,
            column: 3,
        });
        editor.document.move_to(Cursor {
            position: Position { line: 2, column: 3 },
            selection: Some(Position { line: 0, column: 2 }),
        });
        let source_cursor = editor.document.content().cursor();
        let visual_anchor = Some(CaretPosition {
            element: 0,
            column: 1,
        });
        editor.visual_anchor = visual_anchor;
        editor.comments.save("keep me", editor.caret.position());
        editor.note_text = iced::widget::text_editor::Content::with_text("draft");
        editor.editing_comment = Some((0, 0));
        let contents = "# New\n\nnew body";
        std::fs::write(&path, contents).unwrap();

        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::ExternalChange),
        );

        assert_eq!(editor.document.content().cursor(), source_cursor);
        assert_projection_matches(&editor, contents);
        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 0,
                column: 0,
            }
        );
        assert_eq!(editor.visual_anchor, visual_anchor);
        assert_eq!(editor.comments.len(), 1);
        assert_eq!(editor.note_text.text(), "draft");
        assert_eq!(editor.editing_comment, Some((0, 0)));

        editor.caret.place(CaretPosition {
            element: 1,
            column: 2,
        });
        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::ExternalChange),
        );
        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 1,
                column: 2,
            }
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn entering_preview_policy_projects_edits_and_places_caret_from_source() {
        use iced::widget::text_editor::{Cursor, Position};

        let mut editor = editor_at(
            "stale",
            CaretPosition {
                element: 0,
                column: 3,
            },
        );
        editor.keymap.note(Transition::PreviewToggled);
        let contents = "first\n\nsecond\n\nthird";
        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::OpenLoaded(Some((
                "/tmp/task-5-preview.md".into(),
                contents.to_owned(),
            )))),
        );
        editor.document.move_to(Cursor {
            position: Position { line: 4, column: 2 },
            selection: Some(Position { line: 2, column: 1 }),
        });
        let source_cursor = editor.document.content().cursor();
        editor.visual_anchor = Some(CaretPosition {
            element: 0,
            column: 1,
        });
        editor.comments.save("keep me", editor.caret.position());

        let reveal = synchronize_source(&mut editor, SourceSynchronization::EnterPreview);

        assert!(reveal);
        assert_eq!(editor.document.content().cursor(), source_cursor);
        assert_projection_matches(&editor, contents);
        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 2,
                column: 0,
            }
        );
        assert!(editor.visual_anchor.is_none());
        assert_eq!(editor.comments.len(), 1);
    }

    /// Help does not move the source cursor/selection or the preview
    /// caret/visual anchor when it opens and closes.
    #[test]
    fn help_preserves_underlying_editor_state() {
        use iced::widget::text_editor::{Cursor, Position};

        let mut source = Editor {
            document: super::document::State::new("first\nsecond", None),
            markdown: iced::widget::markdown::Content::parse("first\nsecond"),
            keymap: Keymap::new(false),
            caret: Caret::new(),
            preview_elements: ElementMap::parse("first\nsecond"),
            visual_anchor: None,
            note_text: iced::widget::text_editor::Content::new(),
            comments: Comments::new(),
            sidebar_override: None,
            find: find::Find::new(),
            help: super::help::Help::new(),
            editing_comment: None,
            palette: Palette::default(),
        };
        source.document.move_to(Cursor {
            position: Position { line: 1, column: 3 },
            selection: Some(Position { line: 0, column: 1 }),
        });
        let before = source.document.content().cursor();
        let _ = update(&mut source, Message::OpenHelp);
        let _ = update(&mut source, Message::Help(super::help::Message::Close));
        assert_eq!(source.document.content().cursor(), before);

        let mut preview = editor_at(
            "first\n\nsecond",
            CaretPosition {
                element: 1,
                column: 2,
            },
        );
        preview.visual_anchor = Some(CaretPosition {
            element: 0,
            column: 1,
        });
        let caret = preview.caret.position();
        let anchor = preview.visual_anchor;
        let _ = update(&mut preview, Message::OpenHelp);
        let _ = update(&mut preview, Message::Help(super::help::Message::Close));
        assert_eq!(preview.caret.position(), caret);
        assert_eq!(preview.visual_anchor, anchor);
    }

    /// The unsaved modal is composed from the real document and input
    /// state above the persistent editing surface and below Help.
    #[test]
    fn unsaved_modal_is_above_editing_surface_and_below_help() {
        use iced::widget::text_editor::{Action, Edit};

        let mut editor = editor_at(
            "draft",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );
        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::Edit(Action::Edit(Edit::Insert(
                '!',
            )))),
        );
        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::OpenRequested),
        );
        let _ = update(&mut editor, Message::OpenHelp);

        assert_eq!(
            root_layer_order(&editor),
            vec![
                RootLayer::EditingSurface,
                RootLayer::Unsaved,
                RootLayer::Help
            ]
        );
    }

    /// The root keyboard guard claims the `Ctrl + ?` chord before any
    /// focused input can edit, and while help is open it only claims the
    /// closers (Escape, the chord) and Tab — every other key press reaches
    /// the window's search field, and input-method events reach it too.
    #[test]
    fn help_guard_routes_keys_to_the_help_window() {
        use iced::keyboard::{key, Location, Modifiers};

        let pressed = |key: iced::keyboard::Key, modifiers| {
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
                location: Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            })
        };

        let write = Keymap::new(false);

        // A plain `?` (Shift + /) types into the editor like any character.
        let question = pressed(iced::keyboard::Key::Character("?".into()), Modifiers::SHIFT);
        assert_eq!(
            keyboard_guard_action(write, &question),
            KeyboardGuardAction::Pass
        );

        // The chord opens help — both the `?` and the `/` spelling.
        let chord = pressed(
            iced::keyboard::Key::Character("?".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
        );
        assert_eq!(
            keyboard_guard_action(write, &chord),
            KeyboardGuardAction::OpenHelp
        );
        let slash_chord = pressed(iced::keyboard::Key::Character("/".into()), Modifiers::CTRL);
        assert_eq!(
            keyboard_guard_action(write, &slash_chord),
            KeyboardGuardAction::OpenHelp
        );

        // While help is open, typing and input-method events reach the
        // window's search field; Tab stays captured so focus cannot wander
        // beneath the overlay; Escape and the chord close.
        let mut help = write;
        help.note(Transition::HelpOpened);

        let typing = pressed(
            iced::keyboard::Key::Character("x".into()),
            Modifiers::default(),
        );
        assert_eq!(
            keyboard_guard_action(help, &typing),
            KeyboardGuardAction::Pass
        );
        assert_eq!(
            keyboard_guard_action(help, &chord),
            KeyboardGuardAction::CloseHelp
        );
        assert_eq!(
            keyboard_guard_action(
                help,
                &iced::Event::InputMethod(iced::advanced::input_method::Event::Closed)
            ),
            KeyboardGuardAction::Pass
        );

        let tab = pressed(
            iced::keyboard::Key::Named(key::Named::Tab),
            Modifiers::default(),
        );
        assert_eq!(
            keyboard_guard_action(help, &tab),
            KeyboardGuardAction::Capture
        );

        let escape = pressed(
            iced::keyboard::Key::Named(key::Named::Escape),
            Modifiers::default(),
        );
        assert_eq!(
            keyboard_guard_action(help, &escape),
            KeyboardGuardAction::CloseHelp
        );

        // Escape with the find popup open still closes find.
        let mut find = write;
        find.note(Transition::FindOpened);
        assert_eq!(
            keyboard_guard_action(find, &escape),
            KeyboardGuardAction::CloseFind
        );
    }

    /// Saving the popup note stores a comment anchored at the caret and
    /// resets the popup; empty notes are discarded.
    #[test]
    fn saving_a_note_adds_a_comment() {
        let position = CaretPosition {
            element: 1,
            column: 2,
        };
        let mut editor = editor_at("# Title\n\nbody", position);
        editor.note_text = iced::widget::text_editor::Content::with_text("  fix this  \n");
        editor.keymap.note(Transition::NoteOpened);

        let _ = update(&mut editor, Message::SaveNote);

        assert!(!editor.keymap.note_open());
        assert_eq!(editor.note_text.text(), "");
        assert_eq!(editor.comments.len(), 1);
        assert_eq!(
            editor
                .comments
                .cards("# Title\n\nbody", editor.preview_elements.elements())[0]
                .text,
            "fix this"
        );
        // The freshly saved comment is the active one, anchored at the
        // caret position (element 1, column 2).
        assert_eq!(editor.comments.mark_for(1, 32), Mark::Active);
        assert_eq!(editor.comments.cycle(), Some(position));

        // An empty note only closes the popup; the first comment stays.
        editor.keymap.note(Transition::NoteOpened);
        editor.note_text = iced::widget::text_editor::Content::with_text("   ");
        let _ = update(&mut editor, Message::SaveNote);
        assert_eq!(editor.comments.len(), 1);
        assert!(!editor.keymap.note_open());
    }

    /// A note saved while the caret is inside a fenced code block anchors
    /// on the code element: it carries the comment mark and its card
    /// quotes the fenced source.
    #[test]
    fn comments_anchor_on_code_blocks() {
        let markdown = "text\n\n```\ncode line\n```\n\nafter";
        let mut editor = editor_at(
            markdown,
            CaretPosition {
                element: 1,
                column: 4,
            },
        );
        editor.note_text = iced::widget::text_editor::Content::with_text("about the code");
        editor.keymap.note(Transition::NoteOpened);

        let _ = update(&mut editor, Message::SaveNote);

        assert_eq!(editor.comments.mark_for(1, 32), Mark::Active);
        let cards = editor
            .comments
            .cards(markdown, editor.preview_elements.elements());
        assert!(cards[0].quote.contains("```"));
    }

    /// Clicking a comment card activates its comment and moves the cursor
    /// to the anchored element: the preview caret jumps there, and in
    /// write mode the source cursor lands on the element's source.
    #[test]
    fn clicking_a_comment_card_moves_the_cursor_to_its_anchor() {
        use iced::widget::text_editor::Position;

        let mut editor = editor_at(
            "# Title\n\nfirst\n\nsecond",
            CaretPosition {
                element: 2,
                column: 3,
            },
        );
        editor.note_text = iced::widget::text_editor::Content::with_text("note");
        editor.keymap.note(Transition::NoteOpened);
        let _ = update(&mut editor, Message::SaveNote);

        // The caret wanders off before the card is clicked.
        editor.caret.place(CaretPosition {
            element: 0,
            column: 0,
        });

        let _ = update(&mut editor, Message::CommentCardPressed(0, 0));

        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 2,
                column: 3,
            }
        );
        assert_eq!(editor.comments.mark_for(2, 32), Mark::Active);

        // In write mode the source cursor lands on the anchored element's
        // source instead.
        editor.keymap.note(Transition::PreviewToggled);
        let _ = update(&mut editor, Message::CommentCardPressed(0, 0));

        assert_eq!(
            editor.document.content().cursor().position,
            Position { line: 4, column: 0 }
        );
    }

    /// Escaping the note popup discards the draft: the next `c` opens a
    /// fresh popup instead of the half-written note.
    #[test]
    fn dismissing_the_note_popup_resets_its_text() {
        let mut editor = editor_at(
            "# Title\n\nbody",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );
        editor.keymap.note(Transition::NoteOpened);
        editor.note_text = iced::widget::text_editor::Content::with_text("half-written");

        let _ = update(&mut editor, Message::CloseNotePopup);

        assert!(!editor.keymap.note_open());
        assert_eq!(editor.note_text.text(), "");
        assert!(editor.comments.is_empty());
    }

    /// `Ctrl+S` with the popup closed saves no note.
    #[test]
    fn save_note_without_popup_is_a_no_op() {
        let mut editor = editor_at(
            "# Title\n\nbody",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );

        let _ = update(&mut editor, Message::SaveNote);

        assert!(editor.comments.is_empty());
        assert_eq!(editor.keymap.mode(), Mode::View);
    }

    /// Typing in the find popup selects the first match right away: as an
    /// editor selection in write mode, and as a caret jump in the preview.
    /// Enter steps through the matches and wraps around; Shift+Enter
    /// steps back.
    #[test]
    fn find_selects_and_steps_through_matches() {
        use iced::widget::text_editor::Position;

        let mut editor = editor_at(
            "# Title\n\nbody text\n\nmore text",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );
        editor.keymap.note(Transition::FindOpened);
        assert!(editor.keymap.find_open());

        // In the preview, the caret jumps to the match's element.
        let _ = update(&mut editor, Message::FindQueryChanged("more".to_owned()));
        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 2,
                column: 0
            }
        );

        // A single match: stepping wraps back onto itself.
        let _ = update(&mut editor, Message::FindNext);
        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 2,
                column: 0
            }
        );

        // In write mode, the match becomes the editor's selection and
        // Enter walks the matches.
        editor.keymap.note(Transition::PreviewToggled);
        let _ = update(&mut editor, Message::FindQueryChanged("text".to_owned()));
        let cursor = editor.document.content().cursor();
        assert_eq!(cursor.position, Position { line: 2, column: 5 });
        assert_eq!(cursor.selection, Some(Position { line: 2, column: 9 }));

        let _ = update(&mut editor, Message::FindNext);
        let cursor = editor.document.content().cursor();
        assert_eq!(cursor.position, Position { line: 4, column: 5 });
        assert_eq!(cursor.selection, Some(Position { line: 4, column: 9 }));

        // Wraps around to the first match.
        let _ = update(&mut editor, Message::FindNext);
        let cursor = editor.document.content().cursor();
        assert_eq!(cursor.position, Position { line: 2, column: 5 });

        // And Shift+Enter steps back.
        let _ = update(&mut editor, Message::FindPrevious);
        let cursor = editor.document.content().cursor();
        assert_eq!(cursor.position, Position { line: 4, column: 5 });
    }

    /// A note saved while visual mode holds a selection anchors to exactly
    /// the selected text: the covered elements carry the mark, the card
    /// quotes the joined source, and the active span reports its slices.
    #[test]
    fn notes_over_a_selection_comment_the_selected_text() {
        let mut editor = editor_at(
            "alpha\n\nbeta\n\ngamma",
            CaretPosition {
                element: 1,
                column: 1,
            },
        );

        // Visual mode anchors at the caret, which wanders to (2, 3).
        editor.visual_anchor = Some(CaretPosition {
            element: 1,
            column: 1,
        });
        editor.caret.place(CaretPosition {
            element: 2,
            column: 3,
        });

        editor.keymap.note(Transition::NoteOpened);
        editor.note_text = iced::widget::text_editor::Content::with_text("about the span");
        let _ = update(&mut editor, Message::SaveNote);

        assert_eq!(editor.comments.mark_for(1, 32), Mark::Active);
        assert_eq!(editor.comments.mark_for(2, 32), Mark::Active);
        assert_eq!(editor.comments.mark_for(0, 32), Mark::None);

        let cards = editor
            .comments
            .cards("alpha\n\nbeta\n\ngamma", editor.preview_elements.elements());
        assert_eq!(cards[0].quote, "eta gam");

        // The active selection anchor reports the covered slices.
        assert_eq!(editor.comments.anchor_selection_for(1, 4), Some(1..4));
        assert_eq!(editor.comments.anchor_selection_for(2, 5), Some(0..3));
    }

    /// Enter over the active comment opens the note popup for editing —
    /// the text preloaded, the popup titled so — and saving updates the
    /// comment while keeping the old text as history. Without an active
    /// comment, Enter does nothing at all.
    #[test]
    fn enter_edits_the_active_comment_end_to_end() {
        let mut editor = editor_at(
            "# Title\n\nbody",
            CaretPosition {
                element: 1,
                column: 0,
            },
        );
        editor.note_text = iced::widget::text_editor::Content::with_text("first");
        editor.keymap.note(Transition::NoteOpened);
        let _ = update(&mut editor, Message::SaveNote);

        // Without an active comment (deleted below), Enter is a no-op;
        // here the fresh comment is active, so Enter opens it for editing.
        let _ = update(&mut editor, Message::EditActiveComment);
        assert!(editor.keymap.note_open());
        assert_eq!(editor.editing_comment, Some((0, 0)));
        assert_eq!(editor.note_text.text(), "first");

        // Saving the edited text updates the comment and keeps history.
        editor.note_text = iced::widget::text_editor::Content::with_text("  second take  ");
        let _ = update(&mut editor, Message::SaveNote);

        assert!(!editor.keymap.note_open());
        assert_eq!(editor.comments.active_text(), Some("second take"));
        assert_eq!(editor.comments.active_history(), ["first"]);
        assert_eq!(editor.comments.len(), 1);

        // Dismissing the edit popup keeps the original comment.
        let _ = update(&mut editor, Message::EditActiveComment);
        editor.note_text = iced::widget::text_editor::Content::with_text("nope");
        let _ = update(&mut editor, Message::CloseNotePopup);
        assert_eq!(editor.comments.active_text(), Some("second take"));
        assert_eq!(editor.note_text.text(), "");
        assert!(editor.editing_comment.is_none());

        // With the comment deleted, Enter opens nothing.
        let _ = update(&mut editor, Message::DeleteComment(0, 0));
        let _ = update(&mut editor, Message::EditActiveComment);
        assert!(!editor.keymap.note_open());
    }

    /// Comments grow threads through the app too: a second note on the
    /// same element replies, replies render indented, and the thread's
    /// resolve/delete work from the sidebar's buttons.
    #[test]
    fn comment_threads_grow_and_resolve_through_the_sidebar() {
        let mut editor = editor_at(
            "one\n\ntwo",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );

        for text in ["root", "reply"] {
            editor.keymap.note(Transition::NoteOpened);
            editor.note_text = iced::widget::text_editor::Content::with_text(text);
            let _ = update(&mut editor, Message::SaveNote);
        }

        assert_eq!(editor.comments.len(), 2);
        let cards = editor
            .comments
            .cards("one\n\ntwo", editor.preview_elements.elements());
        assert_eq!((cards[1].thread, cards[1].entry), (0, 1));
        assert_eq!(cards[1].depth, 1);

        // Resolving moves the whole thread to history.
        let _ = update(&mut editor, Message::ResolveComment(0));
        assert_eq!(editor.comments.mark_for(0, 32), Mark::None);

        // Reopening brings the marks back.
        let _ = update(&mut editor, Message::ResolveComment(0));
        assert_eq!(editor.comments.mark_for(0, 32), Mark::Active);

        // Deleting the reply leaves the root; deleting the root removes
        // the thread.
        let _ = update(&mut editor, Message::DeleteComment(0, 1));
        assert_eq!(editor.comments.len(), 1);
        let _ = update(&mut editor, Message::DeleteComment(0, 0));
        assert!(editor.comments.is_empty());
    }

    /// Ctrl+Enter files the sidebar draft as a global comment, exactly
    /// like pressing the Add button.
    #[test]
    fn ctrl_enter_adds_the_draft_as_a_global_comment() {
        let mut editor = editor_at(
            "text",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );

        editor
            .comments
            .edit_draft(iced::widget::text_editor::Action::Edit(
                iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(
                    "  overall note  ".to_owned(),
                )),
            ));
        let _ = update(&mut editor, Message::AddGlobalComment);

        assert_eq!(editor.comments.len(), 1);
        let cards = editor.comments.cards("", &[]);
        assert_eq!(cards[0].label, Some("Global"));
        assert_eq!(cards[0].text, "overall note");
        assert_eq!(editor.comments.draft().text(), "");
    }
}
