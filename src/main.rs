mod comments;
mod editing;
mod highlight;
mod interactive_text;
mod keymap;
mod preview;

use comments::{Comments, Mark};
use keymap::{Keymap, Mode};
use preview::{Caret, CaretPosition, ElementMap, Jump, Motion, WordMotion};

use iced::advanced::widget::operation::{Outcome, Scrollable};
use iced::advanced::widget::Operation;
use iced::widget::{
    button, canvas, column, container, markdown, mouse_area, operation::focus,
    operation::focus_next, operation::scroll_by, operation::AbsoluteOffset, row, scrollable, stack,
    text, text_editor, text_input, tooltip, Id, Space,
};
use iced::{
    alignment, application, keyboard, mouse, Background, Border, Color, Element, Font, Length,
    Point, Rectangle, Renderer, Subscription, Task, Theme, Vector,
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
    content: text_editor::Content,
    markdown: markdown::Content,
    /// The file the contents came from and save back to, once known.
    path: Option<std::path::PathBuf>,
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
    /// The find popup's query (`Ctrl+F`).
    find_query: String,
}

#[derive(Debug, Clone)]
enum Message {
    Edit(text_editor::Action),
    OpenFile,
    FileLoaded(Option<(std::path::PathBuf, String)>),
    SaveFile,
    SavePathChosen(Option<std::path::PathBuf>),
    FileSaved(Result<std::path::PathBuf, String>),
    FileChangedExternally,
    TogglePreview,
    LinkClicked(markdown::Uri),
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
    CommentCardPressed(usize),
    EditPublish(text_editor::Action),
    PublishPressed,
    AddGlobalComment,
    NextComment,
    ToggleSidebar,
    OpenFind,
    CloseFind,
    FindQueryChanged(String),
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

async fn open_file() -> Option<(std::path::PathBuf, String)> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Open Markdown file")
        .pick_file()
        .await?;

    let contents = String::from_utf8_lossy(&file.read().await).into_owned();

    Some((file.path().to_path_buf(), contents))
}

/// Writes the editor contents to `path`, returning the path on success and
/// the error message on failure (kept as a string so the message stays
/// `Clone`).
async fn save_file(
    path: std::path::PathBuf,
    contents: String,
) -> Result<std::path::PathBuf, String> {
    // The files this editor opens are small; a blocking write inside the
    // task is fine.
    std::fs::write(&path, contents)
        .map_err(|error| format!("cannot save '{}': {error}", path.display()))?;

    Ok(path)
}

async fn pick_save_path() -> Option<std::path::PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Save Markdown file")
        .set_file_name("untitled.md")
        .save_file()
        .await
        .map(|file| file.path().to_path_buf())
}

fn subscription(editor: &Editor) -> Subscription<Message> {
    let keys = keyboard::listen()
        .with(editor.keymap)
        .filter_map(|(keymap, event)| keymap.handle(event));

    match &editor.path {
        Some(path) => Subscription::batch([keys, watch_file(path)]),
        None => keys,
    }
}

/// Watches the opened file's directory and reloads it when it changes on
/// disk — in write mode and preview alike.
fn watch_file(path: &std::path::Path) -> Subscription<Message> {
    Subscription::run_with(path.to_path_buf(), |path| {
        let path = path.clone();

        iced::stream::channel(1, move |sender| async move {
            spawn_watcher(path, sender);
            // The events arrive on the watcher thread; this runner only
            // keeps the stream alive.
            std::future::pending::<()>().await;
        })
    })
}

/// Spawns the filesystem watcher thread for `path`, forwarding one
/// [`Message::FileChangedExternally`] per burst of events touching the
/// file. Editors that save by rename-over are caught by watching the
/// directory rather than the file itself.
fn spawn_watcher(
    path: std::path::PathBuf,
    mut sender: iced::futures::channel::mpsc::Sender<Message>,
) {
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};

        let Some(directory) = path.parent().map(std::path::Path::to_path_buf) else {
            return;
        };
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(watcher) => watcher,
            Err(error) => {
                eprintln!("agmawrite: cannot watch '{}': {error}", path.display());
                return;
            }
        };

        if let Err(error) = watcher.watch(&directory, RecursiveMode::NonRecursive) {
            eprintln!("agmawrite: cannot watch '{}': {error}", directory.display());
            return;
        }

        while let Ok(event) = rx.recv() {
            let Ok(event) = event else { continue };

            // Reading the file below produces Access events on Linux. If
            // those events trigger another read, the watcher and editor
            // form a feedback loop that consumes an entire CPU core.
            if !may_change_file(&event.kind) {
                continue;
            }

            let touches = event.paths.iter().any(|event_path| {
                event_path == &path
                    || event_path
                        .canonicalize()
                        .is_ok_and(|canonicalized| canonicalized == canonical)
            });

            if !touches {
                continue;
            }

            // A save often produces a burst of events; collapse it into a
            // single reload.
            while rx.try_recv().is_ok() {}

            if !forward_file_change(&mut sender) {
                break;
            }
        }
    });
}

fn may_change_file(kind: &notify::EventKind) -> bool {
    !kind.is_access()
}

/// Queues a reload, treating a full one-item channel as an already queued
/// reload rather than as a disconnected watcher.
fn forward_file_change(sender: &mut iced::futures::channel::mpsc::Sender<Message>) -> bool {
    match sender.try_send(Message::FileChangedExternally) {
        Ok(()) => true,
        Err(error) => error.is_full(),
    }
}

/// Replaces source text while keeping its cursor (and selection) at the
/// nearest valid position in the externally changed document.
fn replace_source_text(editor: &mut Editor, contents: &str) {
    let cursor = editor.content.cursor();
    let mut content = text_editor::Content::with_text(contents);
    content.move_to(text_editor::Cursor {
        position: clamp_editor_position(&content, cursor.position),
        selection: cursor
            .selection
            .map(|position| clamp_editor_position(&content, position)),
    });
    editor.content = content;
}

fn clamp_editor_position(
    content: &text_editor::Content,
    position: text_editor::Position,
) -> text_editor::Position {
    let line = position.line.min(content.line_count().saturating_sub(1));
    let text = content.line(line).map(|line| line.text).unwrap_or_default();
    let mut column = position.column.min(text.len());

    // iced's editor columns are UTF-8 byte offsets. An external edit can
    // put the old offset in the middle of a new multi-byte character.
    while !text.is_char_boundary(column) {
        column -= 1;
    }

    text_editor::Position { line, column }
}

fn update(editor: &mut Editor, message: Message) -> Task<Message> {
    // The note popup's text area owns the draft until the popup closes; a
    // save started from the open popup still lands after the keymap has
    // marked the popup closed.
    let note_was_open = editor.keymap.note_open();
    editor.keymap.note(&message);

    match message {
        Message::Edit(action) => return edit_source(editor, action),
        Message::OpenFile => return Task::perform(open_file(), Message::FileLoaded),
        Message::FileLoaded(Some((path, contents))) => {
            editor.path = Some(path);
            editor.preview_elements = ElementMap::parse(&contents);
            editor.caret = Caret::new();
            editor.visual_anchor = None;
            editor.note_text = text_editor::Content::new();
            editor.comments = Comments::new();
            editor.content = text_editor::Content::with_text(&contents);
            editor.markdown = markdown::Content::parse(&contents);
        }
        Message::FileLoaded(None) => {}
        Message::SaveFile => {
            return match &editor.path {
                // A known path saves straight to disk; an unsaved document
                // asks where first.
                Some(path) => Task::perform(
                    save_file(path.clone(), editor.content.text()),
                    Message::FileSaved,
                ),
                None => Task::perform(pick_save_path(), Message::SavePathChosen),
            };
        }
        Message::SavePathChosen(Some(path)) => {
            editor.path = Some(path.clone());

            return Task::perform(save_file(path, editor.content.text()), Message::FileSaved);
        }
        Message::SavePathChosen(None) => {}
        Message::FileSaved(Ok(_path)) => {}
        Message::FileSaved(Err(error)) => {
            eprintln!("agmawrite: {error}");
        }
        Message::FileChangedExternally => {
            let Some(path) = &editor.path else {
                return Task::none();
            };

            // Our own saves fire the watcher too; reload only when the
            // contents actually differ.
            let Ok(contents) = std::fs::read_to_string(path) else {
                return Task::none();
            };

            if contents != editor.content.text() {
                replace_source_text(editor, &contents);
                editor.markdown = markdown::Content::parse(&contents);
                editor.preview_elements = ElementMap::parse(&contents);
                editor.caret = Caret::new();

                if editor.keymap.preview() {
                    return reveal_preview_caret();
                }
            }
        }
        Message::TogglePreview => {
            if !editor.keymap.preview_only() {
                editor.visual_anchor = None;

                if editor.keymap.preview() {
                    let contents = editor.content.text();
                    editor.preview_elements = ElementMap::parse(&contents);
                    editor
                        .caret
                        .move_to_source_cursor(&editor.content, editor.preview_elements.elements());
                    editor.markdown = markdown::Content::parse(&contents);

                    return reveal_preview_caret();
                }

                // Switching back to write mode: the editor content kept its
                // cursor, it only needs focus for the caret to show again.
                return focus(Id::new(SOURCE_EDITOR_ID));
            }
        }
        Message::LinkClicked(_uri) => {
            // TODO: open links in the default browser
        }
        Message::MovePreviewCursor(motion) => {
            // The caret always advances; the page only scrolls as much as
            // needed to keep the caret visible.
            if editor
                .caret
                .move_by(editor.preview_elements.elements(), motion)
            {
                return reveal_preview_caret();
            }
        }
        Message::MovePreviewWord(motion) => {
            if editor
                .caret
                .move_word(editor.preview_elements.elements(), motion)
            {
                return reveal_preview_caret();
            }
        }
        Message::MovePreviewJump(jump) => {
            if editor.caret.jump(editor.preview_elements.elements(), jump) {
                return reveal_preview_caret();
            }
        }
        Message::PreviewGPressed => {}
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
        Message::OpenNotePopup => {
            return focus(Id::new(NOTE_EDITOR_ID));
        }
        Message::CloseNotePopup => {
            // Dismissing the popup discards the draft, so the next note
            // starts fresh instead of inheriting the escaped one.
            editor.note_text = text_editor::Content::new();
        }
        Message::EditNote(action) => editor.note_text.perform(action),
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
            editor.find_query = query;

            return find_select_first_match(editor);
        }
        // Clicks on the card itself are swallowed so they neither close the
        // popup nor reach the preview beneath.
        Message::NoteCardPressed => {}
        Message::CommentCardPressed(index) => {
            // Clicking a card makes its comment the active one and moves the
            // cursor to the anchored element: the preview caret jumps there
            // and the page reveals it; in write mode the source cursor lands
            // on the element's source instead.
            if let Some(anchor) = editor.comments.activate(index) {
                if editor.keymap.preview() {
                    editor.visual_anchor = None;
                    editor.caret.place(anchor);

                    return reveal_preview_caret();
                }

                let source = editor.content.text();
                let element = editor.preview_elements.elements().get(anchor.element);

                if let Some(element) = element {
                    editor.content.move_to(text_editor::Cursor {
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

/// Selects the first match of the find query, live as it is typed: in
/// write mode the match becomes the editor's selection, in the preview the
/// caret jumps to the match's element and the matches paint as highlights.
fn find_select_first_match(editor: &mut Editor) -> Task<Message> {
    let query = editor.find_query.clone();

    if query.is_empty() {
        return Task::none();
    }

    if editor.keymap.preview() {
        let position = editor
            .preview_elements
            .elements()
            .iter()
            .enumerate()
            .find_map(|(index, element)| {
                editing::matches_in(element.text(), &query)
                    .first()
                    .map(|range| CaretPosition {
                        element: index,
                        column: range.start,
                    })
            });

        if let Some(position) = position {
            editor.caret.place(position);

            return reveal_preview_caret();
        }

        return Task::none();
    }

    let source = editor.content.text();

    if let Some((line, start, end)) = editing::first_match(&source, &query) {
        editor.content.move_to(text_editor::Cursor {
            position: text_editor::Position {
                line,
                column: start,
            },
            selection: Some(text_editor::Position { line, column: end }),
        });
    }

    Task::none()
}

/// The find popup: a small card in the top right corner with the query
/// field and a live match count.
fn find_popup(editor: &Editor) -> Element<'_, Message> {
    let match_count = (!editor.find_query.is_empty()).then(|| {
        if editor.keymap.preview() {
            editor
                .preview_elements
                .elements()
                .iter()
                .map(|element| editing::matches_in(element.text(), &editor.find_query).len())
                .sum()
        } else {
            editing::matches_in(&editor.content.text(), &editor.find_query).len()
        }
    });

    let mut card_row = row![text_input("Search…", &editor.find_query)
        .id(Id::new(FIND_INPUT_ID))
        .on_input(Message::FindQueryChanged)
        .font(EDITOR_FONT)
        .size(14)
        .padding(6)
        .width(Length::Fixed(220.0))]
    .spacing(8)
    .align_y(alignment::Vertical::Center);

    if let Some(count) = match_count {
        card_row = card_row.push(
            text(if count == 0 {
                "no match".to_owned()
            } else {
                count.to_string()
            })
            .font(EDITOR_FONT)
            .size(12)
            .color(if count == 0 {
                Color::from_rgb(0.9, 0.45, 0.4)
            } else {
                Color::from_rgb(0.5, 0.5, 0.5)
            }),
        );
    }

    container(container(card_row).padding(8).style(note_card_style))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(alignment::Horizontal::Right)
        .align_y(alignment::Vertical::Top)
        .padding(16)
        .into()
}

/// Whether the comments sidebar is showing: the manual `Ctrl+B` override
/// wins, otherwise it appears once there are comments.
fn sidebar_shown(editor: &Editor) -> bool {
    editor
        .sidebar_override
        .unwrap_or(!editor.comments.is_empty())
}

/// Applies a source edit action. Pressing Enter on a list line continues
/// the list — the marker, indented like the current item, starts the new
/// line — and pressing it on an empty item removes the marker and ends
/// the list.
fn edit_source(editor: &mut Editor, action: text_editor::Action) -> Task<Message> {
    use text_editor::{Action, Edit};

    if !matches!(action, Action::Edit(Edit::Enter)) {
        editor.content.perform(action);
        return Task::none();
    }

    let cursor = editor.content.cursor().position;
    let before = editor.content.line(cursor.line).map(|line| {
        line.text
            .char_indices()
            .nth(cursor.column)
            .map_or(line.text.as_ref(), |(index, _)| &line.text[..index])
            .to_owned()
    });

    match before.as_deref().map(editing::continuation) {
        Some(editing::Continuation::Continue(prefix)) => {
            editor.content.perform(action);
            editor
                .content
                .perform(Action::Edit(Edit::Paste(std::sync::Arc::new(prefix))));
        }
        Some(editing::Continuation::Outdent(count)) => {
            for _ in 0..count {
                editor.content.perform(Action::Edit(Edit::Backspace));
            }

            editor.content.perform(action);
        }
        _ => editor.content.perform(action),
    }

    Task::none()
}

/// Saves the note popup text as a comment anchored at the preview caret,
/// then closes the popup with a fresh note. Empty notes are discarded.
fn save_note(editor: &mut Editor) {
    editor
        .comments
        .save(&editor.note_text.text(), editor.caret.position());

    editor.note_text = text_editor::Content::new();
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
                Vec::new(),
            );
        };

        let focused = self.focused_element == element;
        let selection = self.visual.and_then(|(anchor, caret)| {
            preview::element_selection(anchor, caret, element, preview_element.len())
        });
        let mark = self.comments.mark_for(element);
        let commented = matches!(mark, Mark::Commented | Mark::Active);
        let active_comment = mark == Mark::Active;
        let search_matches = if self.find_query.is_empty() {
            Vec::new()
        } else {
            editing::matches_in(preview_element.text(), self.find_query)
        };

        interactive_text::paragraph(
            settings,
            text,
            selection,
            focused.then_some(self.caret_column),
            focused.then(|| Id::new(PREVIEW_CARET_ID)),
            commented,
            active_comment,
            search_matches,
        )
    }
}

fn view(editor: &Editor) -> Element<'_, Message> {
    let position = editor.caret.position();
    let visual = editor.visual_anchor.map(|anchor| (anchor, position));

    let base_area: Element<'_, Message> = if editor.keymap.preview() {
        scrollable(
            container(markdown::view_with(
                editor.markdown.items(),
                markdown::Settings::with_text_size(20.0, markdown_style()),
                &PreviewViewer {
                    claims: editor.preview_elements.claims(),
                    focused_element: position.element,
                    caret_column: position.column,
                    visual,
                    comments: &editor.comments,
                    find_query: &editor.find_query,
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
            .id(Id::new(SOURCE_EDITOR_ID))
            .on_action(Message::Edit)
            .font(EDITOR_FONT)
            .size(20)
            .height(Length::Fill)
            .padding(0)
            .line_height(1.8)
            .highlight_with::<highlight::MarkdownMarkers>((), highlight::format)
            .style(editor_style)
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
        .on_press(Message::OpenFile)
        .width(Length::Fixed(ICON_BUTTON_SIZE))
        .height(Length::Fixed(ICON_BUTTON_SIZE))
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
                .width(Length::Fixed(ICON_SIZE))
                .height(Length::Fixed(ICON_SIZE)),
        )
        .on_press(Message::TogglePreview)
        .width(Length::Fixed(ICON_BUTTON_SIZE))
        .height(Length::Fixed(ICON_BUTTON_SIZE))
        .padding(0)
        .style(icon_button_style),
        container(text("Ctrl + p, Preview").font(EDITOR_FONT).size(12))
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
        .on_press(Message::SaveFile)
        .width(Length::Fixed(ICON_BUTTON_SIZE))
        .height(Length::Fixed(ICON_BUTTON_SIZE))
        .padding(0)
        .style(icon_button_style),
        container(text("Ctrl + s, Save").font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    // The main column: top margin, the writing area, and the bottom
    // controls. It fills the space between the window's 5% side margins.
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
                controls.push(preview_button.into());
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

    // With comments saved, the comments sidebar sits beside the main
    // column and spans the whole window height.
    let mut inner = row![container(main)
        .width(Length::FillPortion(9))
        .height(Length::Fill)]
    .width(Length::Fill)
    .height(Length::Fill);

    if sidebar_shown(editor) {
        inner = inner.push(
            container(comments_sidebar(editor))
                .width(Length::FillPortion(2))
                .height(Length::Fill),
        );
    }

    // The side margins frame the whole window — 5% left and right whether
    // or not the sidebar is showing.
    let content: Element<'_, Message> = container(
        row![
            Space::new()
                .width(Length::FillPortion(5))
                .height(Length::Fill),
            container(inner)
                .width(Length::FillPortion(90))
                .height(Length::Fill),
            Space::new()
                .width(Length::FillPortion(5))
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(background_style)
    .into();

    // The find popup floats in the top right corner, in any mode — on a
    // persistent stack too, for the same reason as the note popup.
    let mut layers = stack![content];

    if editor.keymap.find_open() {
        layers = layers.push(find_popup(editor));
    }

    layers.into()
}

/// The comments sidebar: a full-height panel with a scrollable list of
/// saved comments (each quoting the Markdown source it was written for,
/// collapsed to one line and trimmed) and a free text field with a Publish
/// button at the bottom.
fn comments_sidebar<'a>(editor: &'a Editor) -> Element<'a, Message> {
    let source = editor.content.text();

    let cards: Vec<Element<'_, Message>> = editor
        .comments
        .cards(&source, editor.preview_elements.elements())
        .into_iter()
        .map(|card| {
            let index = card.index;
            let active = card.active;
            // Anchored comments quote their element's source; global
            // comments show their label instead.
            let caption = card.label.map(str::to_owned).unwrap_or(card.quote);

            // The card is a button: clicking it activates its comment and
            // moves the cursor to the anchored element.
            mouse_area(
                container(
                    column![
                        text(caption).font(EDITOR_FONT).size(11).color(if active {
                            Color::from_rgb(0.4, 0.85, 0.78)
                        } else {
                            Color::from_rgb(0.45, 0.45, 0.45)
                        }),
                        text(card.text)
                            .font(EDITOR_FONT)
                            .size(13)
                            .color(Color::WHITE),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                )
                .padding(8)
                .width(Length::Fill)
                .style(move |_theme| comment_card_style(active)),
            )
            .on_press(Message::CommentCardPressed(index))
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
                    text_editor(editor.comments.draft())
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
            row![
                button(text("Add").font(EDITOR_FONT).size(13).color(Color::WHITE),)
                    .on_press(Message::AddGlobalComment)
                    .width(Length::Fill)
                    .padding([6, 12])
                    .style(add_button_style),
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

fn comment_card_style(active: bool) -> container::Style {
    container::Style {
        background: Some(if active {
            Background::Color(Color::from_rgba(0.3, 0.9, 0.8, 0.1))
        } else {
            Background::Color(Color::from_rgb(0.08, 0.08, 0.08))
        }),
        border: Border {
            color: if active {
                Color::from_rgba(0.3, 0.9, 0.8, 0.9)
            } else {
                Color::from_rgb(0.25, 0.25, 0.25)
            },
            width: if active { 1.5 } else { 1.0 },
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

/// The Add button sits beside Publish as the quieter, secondary action:
/// it files the draft as a global comment.
fn add_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(Color::from_rgb(0.22, 0.22, 0.22)))
            }
            _ => Some(Background::Color(Color::from_rgb(0.12, 0.12, 0.12))),
        },
        border: Border {
            color: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb(0.55, 0.55, 0.55)
                }
                _ => Color::from_rgb(0.35, 0.35, 0.35),
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
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

/// A small badge naming the current mode, placed next to the open icon in
/// the bottom bar. Visual mode is highlighted with the selection blue so
/// the active selection state is obvious at a glance; the note mode with
/// the comment amber. Badge and keys read the same representation: the
/// keymap's mode.
fn mode_badge(editor: &Editor) -> Element<'_, Message> {
    let mode = editor.keymap.mode();

    let (label, color) = match mode {
        Mode::Visual => ("VISUAL", Color::from_rgb(0.4, 0.65, 1.0)),
        Mode::Note => ("NOTE", Color::from_rgb(0.9, 0.7, 0.35)),
        Mode::Find => ("FIND", Color::from_rgb(0.95, 0.6, 0.35)),
        Mode::View => ("VIEW", Color::from_rgb(0.6, 0.6, 0.6)),
        Mode::Write => ("WRITE", Color::from_rgb(0.6, 0.6, 0.6)),
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
        .map(ElementMap::parse)
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
        path: args.path.as_ref().map(std::path::PathBuf::from),
        keymap: Keymap::new(args.preview),
        caret: Caret::new(),
        preview_elements,
        visual_anchor: None,
        note_text: text_editor::Content::new(),
        comments: Comments::new(),
        sidebar_override: None,
        find_query: String::new(),
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
    use super::comments::{Comments, Mark};
    use super::keymap::{Keymap, Mode};
    use super::preview::{Caret, CaretPosition, ElementMap};
    use super::{
        forward_file_change, may_change_file, replace_source_text, update, Editor, Message,
    };

    fn editor_at(contents: &str, position: CaretPosition) -> Editor {
        let mut keymap = Keymap::new(false);
        keymap.note(&Message::TogglePreview);

        let mut caret = Caret::new();
        caret.place(position);

        Editor {
            content: iced::widget::text_editor::Content::with_text(contents),
            markdown: iced::widget::markdown::Content::parse(contents),
            path: None,
            keymap,
            caret,
            preview_elements: ElementMap::parse(contents),
            visual_anchor: None,
            note_text: iced::widget::text_editor::Content::new(),
            comments: Comments::new(),
            sidebar_override: None,
            find_query: String::new(),
        }
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
        editor.keymap.note(&Message::OpenNotePopup);

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
        assert_eq!(editor.comments.mark_for(1), Mark::Active);
        assert_eq!(editor.comments.cycle(), Some(position));

        // An empty note only closes the popup; the first comment stays.
        editor.keymap.note(&Message::OpenNotePopup);
        editor.note_text = iced::widget::text_editor::Content::with_text("   ");
        let _ = update(&mut editor, Message::SaveNote);
        assert_eq!(editor.comments.len(), 1);
        assert!(!editor.keymap.note_open());
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
        editor.keymap.note(&Message::OpenNotePopup);
        let _ = update(&mut editor, Message::SaveNote);

        // The caret wanders off before the card is clicked.
        editor.caret.place(CaretPosition {
            element: 0,
            column: 0,
        });

        let _ = update(&mut editor, Message::CommentCardPressed(0));

        assert_eq!(
            editor.caret.position(),
            CaretPosition {
                element: 2,
                column: 3,
            }
        );
        assert_eq!(editor.comments.mark_for(2), Mark::Active);

        // In write mode the source cursor lands on the anchored element's
        // source instead.
        editor.keymap.note(&Message::TogglePreview);
        let _ = update(&mut editor, Message::CommentCardPressed(0));

        assert_eq!(editor.content.cursor().position, Position { line: 4, column: 0 });
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
        editor.keymap.note(&Message::OpenNotePopup);
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
    #[test]
    fn find_selects_the_first_match_as_it_is_typed() {
        use iced::widget::text_editor::Position;

        let mut editor = editor_at(
            "# Title\n\nbody text\n\nmore text",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );
        editor.keymap.note(&Message::OpenFind);
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

        // In write mode, the match becomes the editor's selection.
        editor.keymap.note(&Message::TogglePreview);
        let _ = update(&mut editor, Message::FindQueryChanged("text".to_owned()));
        let cursor = editor.content.cursor();
        assert_eq!(cursor.position, Position { line: 2, column: 5 });
        assert_eq!(cursor.selection, Some(Position { line: 2, column: 9 }));
    }

    /// Opening the watched file to reload it must not trigger another
    /// reload; mutations still do.
    #[test]
    fn file_watcher_ignores_access_events() {
        use notify::event::{AccessKind, AccessMode, DataChange, ModifyKind};
        use notify::EventKind;

        assert!(!may_change_file(&EventKind::Access(AccessKind::Open(
            AccessMode::Read,
        ))));
        assert!(may_change_file(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content,
        ))));
    }

    /// A queued file-change message already represents the latest disk
    /// state. A full channel must therefore coalesce, not kill the watcher.
    #[test]
    fn a_full_file_watcher_channel_stays_connected() {
        let (mut sender, receiver) = iced::futures::channel::mpsc::channel(0);

        assert!(forward_file_change(&mut sender));
        assert!(forward_file_change(&mut sender));

        drop(receiver);
        assert!(!forward_file_change(&mut sender));
    }

    /// Reloading external text keeps the write cursor instead of moving it
    /// to the beginning, clamping positions when the new text is shorter.
    #[test]
    fn external_text_replacement_preserves_the_source_cursor() {
        use iced::widget::text_editor::{Cursor, Position};

        let mut editor = editor_at(
            "first line\nsecond line\nthird line",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );
        editor.content.move_to(Cursor {
            position: Position { line: 1, column: 6 },
            selection: Some(Position { line: 2, column: 5 }),
        });

        replace_source_text(&mut editor, "changed\nstill here\nlast");
        let cursor = editor.content.cursor();
        assert_eq!(cursor.position, Position { line: 1, column: 6 });
        assert_eq!(cursor.selection, Some(Position { line: 2, column: 4 }));

        replace_source_text(&mut editor, "short");
        let cursor = editor.content.cursor();
        assert_eq!(cursor.position, Position { line: 0, column: 5 });
        assert_eq!(cursor.selection, Some(Position { line: 0, column: 4 }));
    }

    /// Enter on a list line continues the list; Enter on an empty item
    /// removes the marker and ends it.
    #[test]
    fn enter_continues_lists() {
        use iced::widget::text_editor::{Action, Cursor, Edit, Position};

        let mut editor = Editor {
            content: iced::widget::text_editor::Content::with_text("- item"),
            markdown: iced::widget::markdown::Content::parse("- item"),
            path: None,
            keymap: Keymap::new(false),
            caret: Caret::new(),
            preview_elements: ElementMap::parse("- item"),
            visual_anchor: None,
            note_text: iced::widget::text_editor::Content::new(),
            comments: Comments::new(),
            sidebar_override: None,
            find_query: String::new(),
        };
        editor.content.move_to(Cursor {
            position: Position { line: 0, column: 6 },
            selection: None,
        });

        let _ = update(&mut editor, Message::Edit(Action::Edit(Edit::Enter)));

        assert_eq!(editor.content.text(), "- item\n- ");
        let cursor = editor.content.cursor();
        assert_eq!(cursor.position, Position { line: 1, column: 2 });

        // The new item is empty; Enter removes the marker and ends the list.
        let _ = update(&mut editor, Message::Edit(Action::Edit(Edit::Enter)));

        assert_eq!(editor.content.text(), "- item\n\n");
        assert_eq!(
            editor.content.cursor().position,
            Position { line: 2, column: 0 }
        );
    }
}
