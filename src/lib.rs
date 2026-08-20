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
use keymap::{Keymap, Mode, Transition};
use theme::Palette;

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{layout, renderer, Clipboard, Layout, Shell, Widget};
use iced::widget::{
    button, canvas, column, container, operation::focus, operation::focus_next, row, stack, text,
    text_editor, tooltip, Id, Space,
};
use iced::{
    alignment, application, keyboard, mouse, Background, Border, Color, Element, Font, Length,
    Point, Rectangle, Renderer, Size, Subscription, Task, Theme, Vector,
};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
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
struct Editor {
    document: document::State,
    /// Parsed Markdown, numbered elements, caret, and visual selection.
    preview: preview::State,
    /// The input mode stack — write, view, visual, note — owning key
    /// handling and the mode badge's state.
    keymap: Keymap,
    /// Comment store and workflow state, including the note composer and
    /// sidebar visibility policy.
    comments: comments::State,
    /// The find popup's query and current match.
    find: find::State,
    /// The shortcuts Help window's query state.
    help: help::Help,
    /// The omarchy color scheme the interface paints with.
    palette: Palette,
}

#[derive(Debug, Clone)]
enum Message {
    Document(document::Message),
    Preview(preview::Message),
    Comments(comments::Message),
    Find(find::Message),
    OpenHelp,
    Help(help::Message),
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
        Command::Preview(command) => Message::Preview(match command {
            PreviewCommand::Toggle => preview::Message::Toggle,
            PreviewCommand::Move(motion, count) => preview::Message::Move(motion, count),
            PreviewCommand::MoveWord(motion, count) => preview::Message::MoveWord(motion, count),
            PreviewCommand::Jump(jump, count) => preview::Message::Jump(jump, count),
            PreviewCommand::ArmG => preview::Message::AcknowledgeG,
            PreviewCommand::ArmZ => preview::Message::AcknowledgeZ,
            PreviewCommand::Count(digit) => preview::Message::AcknowledgeCount(digit),
            PreviewCommand::Cancel => preview::Message::Cancel,
            PreviewCommand::ToggleVisual => preview::Message::ToggleVisual,
            PreviewCommand::ScrollPage(page, count) => preview::Message::ScrollPage(page, count),
            PreviewCommand::ScrollCaret(placement) => preview::Message::ScrollCaret(placement),
        }),
        Command::Comments(command) => Message::Comments(match command {
            CommentsCommand::OpenNote => comments::Message::OpenComposer,
            CommentsCommand::CloseNote => comments::Message::CloseComposer,
            CommentsCommand::SaveNote => comments::Message::SaveComposer,
            CommentsCommand::EditActive => comments::Message::EditActive,
            CommentsCommand::Next => comments::Message::Cycle,
            CommentsCommand::ToggleSidebar => comments::Message::ToggleSidebar,
            CommentsCommand::AddGlobal => comments::Message::AddDraftAsGlobal,
            CommentsCommand::Publish => comments::Message::PublishDraft,
        }),
        Command::Find(command) => Message::Find(match command {
            FindCommand::Open => find::Message::Open,
            FindCommand::Close => find::Message::Close,
            FindCommand::Next => find::Message::Next,
            FindCommand::Previous => find::Message::Previous,
        }),
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
        Message::Preview(preview::Message::Toggle) => Transition::PreviewToggled,
        Message::Preview(preview::Message::ToggleVisual) => Transition::VisualToggled,
        Message::Preview(preview::Message::Cancel) => Transition::PreviewCancelled,
        Message::Preview(preview::Message::AcknowledgeG) => Transition::GArmed,
        Message::Preview(preview::Message::AcknowledgeZ) => Transition::ZArmed,
        Message::Preview(preview::Message::AcknowledgeCount(digit)) => {
            Transition::CountPressed(*digit)
        }
        Message::Comments(comments::Message::OpenComposer) => Transition::NoteOpened,
        Message::Comments(comments::Message::CloseComposer | comments::Message::SaveComposer) => {
            Transition::NoteClosed
        }
        Message::Find(find::Message::Open) => Transition::FindOpened,
        Message::Find(find::Message::Close) => Transition::FindClosed,
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

fn handle_document_event(editor: &mut Editor, event: document::Event) -> Task<Message> {
    match event {
        document::Event::SourceReplaced { reason } => {
            let source = editor.document.text();

            match reason {
                document::SourceReplacement::Loaded => {
                    editor.preview.load_source(&source);
                    editor.comments = comments::State::new();
                    Task::none()
                }
                document::SourceReplacement::External => {
                    editor.preview.replace_source(&source);

                    if editor.keymap.preview() {
                        preview::reveal_caret().map(Message::Preview)
                    } else {
                        Task::none()
                    }
                }
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

fn handle_preview_event(editor: &mut Editor, event: preview::Event) -> Task<Message> {
    match event {
        preview::Event::ToggleRequested => {
            if editor.keymap.preview_only() {
                return Task::none();
            }

            if editor.keymap.preview() {
                let source = editor.document.text();
                editor
                    .preview
                    .refresh_from_source(&source, editor.document.content());
                preview::reveal_caret().map(Message::Preview)
            } else {
                editor.preview.clear_visual_selection();
                focus(Id::new(SOURCE_EDITOR_ID))
            }
        }
        preview::Event::OpenLink(_uri) => {
            // TODO: open links in the default browser
            Task::none()
        }
    }
}

fn handle_comments_event(editor: &mut Editor, event: comments::Event) -> Task<Message> {
    match event {
        comments::Event::NavigateTo(anchor) => {
            if editor.keymap.preview() {
                editor.preview.clear_visual_selection();
                editor.preview.place_caret(anchor);
                preview::reveal_caret().map(Message::Preview)
            } else {
                let source = editor.document.text();
                let element = editor.preview.elements().get(anchor.element);

                if let Some(element) = element {
                    editor.document.move_to(text_editor::Cursor {
                        position: editing::position_at(&source, element.source().start),
                        selection: None,
                    });
                    focus(Id::new(SOURCE_EDITOR_ID))
                } else {
                    Task::none()
                }
            }
        }
        comments::Event::FocusComposer => {
            editor.keymap.note(Transition::NoteOpened);
            comments::focus_composer().map(Message::Comments)
        }
        comments::Event::PublishRequested => {
            // TODO: publish the comments
            Task::none()
        }
    }
}

fn handle_find_event(editor: &mut Editor, event: find::Event) -> Task<Message> {
    match event {
        find::Event::SelectSource(matched) => {
            editor.document.apply_find_selection(matched);
            Task::none()
        }
        find::Event::SelectPreview { element, range } => {
            editor.preview.apply_find_selection(element, range);
            preview::reveal_caret().map(Message::Preview)
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
        Message::Preview(message) => {
            let context = preview::Context {
                visual_active: editor.keymap.visual(),
            };
            let preview::Update { task, event } =
                preview::update(&mut editor.preview, message, context);
            let event_task =
                event.map_or_else(Task::none, |event| handle_preview_event(editor, event));

            return Task::batch([task.map(Message::Preview), event_task]);
        }
        Message::Comments(message) => {
            let closes_composer = matches!(
                &message,
                comments::Message::DeleteComment(thread, entry)
                    if editor.comments.editing_target() == Some((*thread, *entry))
            );
            let context = comments::Context {
                caret: editor.preview.caret(),
                selection: editor.preview.visual_selection(),
                composer_open: note_was_open,
            };
            let comments::Update { event } =
                comments::update(&mut editor.comments, message, context);

            if closes_composer {
                editor.keymap.note(Transition::NoteClosed);
            }

            return event.map_or_else(Task::none, |event| handle_comments_event(editor, event));
        }
        Message::Find(message) => {
            let closes = matches!(message, find::Message::Close);
            let source;
            let surface = if editor.keymap.preview() {
                find::Surface::Preview(editor.preview.elements())
            } else {
                source = editor.document.text();
                find::Surface::Source(&source)
            };
            let find::Update { task, event } = find::update(&mut editor.find, message, surface);
            let event_task =
                event.map_or_else(Task::none, |event| handle_find_event(editor, event));
            let restore_task = if closes && !editor.keymap.preview() {
                // Give the source editor its focus back so its caret resumes.
                focus(Id::new(SOURCE_EDITOR_ID))
            } else {
                Task::none()
            };

            return Task::batch([task.map(Message::Find), event_task, restore_task]);
        }
        // Help owns its query and internal events. The root only composes
        // the window and restores whichever field was underneath it.
        Message::OpenHelp => return help::focus_input(),
        Message::Help(message) => {
            let closed = matches!(message, help::Message::Close);
            editor.help.update(message);

            if closed {
                if editor.keymap.find_open() {
                    return find::focus_input().map(Message::Find);
                }
                if editor.keymap.note_open() {
                    return comments::focus_composer().map(Message::Comments);
                }
                if !editor.keymap.preview() {
                    return focus(Id::new(SOURCE_EDITOR_ID));
                }
            }
        }
    }

    Task::none()
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
                KeyboardGuardAction::CloseFind => {
                    shell.publish(Message::Find(find::Message::Close))
                }
                KeyboardGuardAction::CloseNote => {
                    shell.publish(Message::Comments(comments::Message::CloseComposer))
                }
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

    let source = editor.document.text();
    let find_surface = if editor.keymap.preview() {
        find::Surface::Preview(editor.preview.elements())
    } else {
        find::Surface::Source(&source)
    };

    let base_area: Element<'_, Message> = if editor.keymap.preview() {
        // The preview receives only the find feature's narrow read-only
        // query and current-match projections.
        let current_match = editor.find.current_preview_match(editor.preview.elements());

        preview::view(
            &editor.preview,
            preview::ViewContext::new(
                &editor.comments,
                editor.find.query(),
                current_match,
                palette,
            ),
        )
        .map(Message::Preview)
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
        editing_stack = editing_stack
            .push(comments::composer::view(&editor.comments, palette).map(Message::Comments));
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
            .on_press(Message::Preview(preview::Message::Toggle))
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

    let sidebar_area = comments::sidebar::view(
        &editor.comments,
        comments::sidebar::ViewContext {
            source: &source,
            preview_elements: editor.preview.elements(),
            palette,
            font: EDITOR_FONT,
        },
    )
    .map(Message::Comments);

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
            RootLayer::Find => {
                layers.push(find::view(&editor.find, find_surface, palette).map(Message::Find))
            }
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

fn modal_backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
        ..Default::default()
    }
}

fn modal_card_style(palette: &Palette) -> container::Style {
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

fn modal_button_style(palette: &Palette, _theme: &Theme, status: button::Status) -> button::Style {
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

    let editor = Editor {
        document: document::State::new(
            contents.as_deref().unwrap_or_default(),
            args.path.as_ref().map(std::path::PathBuf::from),
        ),
        preview: preview::State::new(contents.as_deref().unwrap_or_default()),
        keymap: Keymap::new(args.preview),
        comments: comments::State::new(),
        find: find::State::new(),
        help: help::Help::new(),
        palette: Palette::current(),
    };

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
    use super::comments::{self, Mark};
    use super::find;
    use super::keymap::{Keymap, Transition};
    use super::preview::{self, CaretPosition};
    use super::theme::Palette;
    use super::{
        keyboard_guard_action, root_layer_order, update, Editor, KeyboardGuardAction, Message,
        RootLayer,
    };

    fn editor_at(contents: &str, position: CaretPosition) -> Editor {
        let mut keymap = Keymap::new(false);
        keymap.note(Transition::PreviewToggled);

        let mut preview = preview::State::new(contents);
        preview.place_caret(position);

        Editor {
            document: super::document::State::new(contents, None),
            preview,
            keymap,
            comments: comments::State::new(),
            find: find::State::new(),
            help: super::help::Help::new(),
            palette: Palette::default(),
        }
    }

    fn set_composer_text(editor: &mut Editor, text: &str) {
        let _ = update(
            editor,
            Message::Comments(comments::Message::EditComposer(
                iced::widget::text_editor::Action::SelectAll,
            )),
        );
        let _ = update(
            editor,
            Message::Comments(comments::Message::EditComposer(
                iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
                    std::sync::Arc::new(text.to_owned()),
                )),
            )),
        );
    }

    fn save_comment(editor: &mut Editor, text: &str) {
        editor.keymap.note(Transition::NoteOpened);
        set_composer_text(editor, text);
        let _ = update(editor, Message::Comments(comments::Message::SaveComposer));
    }

    fn prepare_comment_edit(editor: &mut Editor, saved: &str, draft: &str) {
        save_comment(editor, saved);
        let _ = update(editor, Message::Comments(comments::Message::EditActive));
        set_composer_text(editor, draft);
    }

    fn assert_projection_matches(editor: &Editor, contents: &str) {
        let expected = preview::State::new(contents);
        assert_eq!(editor.preview.elements(), expected.elements());

        let expected_markdown = iced::widget::markdown::Content::parse(contents);
        assert_eq!(
            format!("{:?}", editor.preview.markdown().items()),
            format!("{:?}", expected_markdown.items())
        );
    }

    fn enter_visual(editor: &mut Editor) {
        let _ = update(editor, Message::Preview(preview::Message::ToggleVisual));
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
        enter_visual(&mut editor);
        prepare_comment_edit(&mut editor, "old note", "draft");
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
            editor.preview.caret(),
            CaretPosition {
                element: 0,
                column: 0,
            }
        );
        assert!(editor.preview.visual_selection().is_none());
        assert!(editor.comments.is_empty());
        assert_eq!(editor.comments.composer().text(), "");
        assert!(editor.comments.editing_target().is_none());
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
        editor.preview.place_caret(CaretPosition {
            element: 0,
            column: 1,
        });
        enter_visual(&mut editor);
        editor.preview.place_caret(CaretPosition {
            element: 1,
            column: 3,
        });
        editor.document.move_to(Cursor {
            position: Position { line: 2, column: 3 },
            selection: Some(Position { line: 0, column: 2 }),
        });
        let source_cursor = editor.document.content().cursor();
        let visual_anchor = CaretPosition {
            element: 0,
            column: 1,
        };
        prepare_comment_edit(&mut editor, "keep me", "draft");
        let contents = "# New\n\nnew body";
        std::fs::write(&path, contents).unwrap();

        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::ExternalChange),
        );

        assert_eq!(editor.document.content().cursor(), source_cursor);
        assert_projection_matches(&editor, contents);
        assert_eq!(
            editor.preview.caret(),
            CaretPosition {
                element: 0,
                column: 0,
            }
        );
        assert_eq!(
            editor.preview.visual_selection().map(|(anchor, _)| anchor),
            Some(visual_anchor)
        );
        assert_eq!(editor.comments.len(), 1);
        assert_eq!(editor.comments.composer().text(), "draft");
        assert_eq!(editor.comments.editing_target(), Some((0, 0)));

        editor.preview.place_caret(CaretPosition {
            element: 1,
            column: 2,
        });
        let _ = update(
            &mut editor,
            Message::Document(super::document::Message::ExternalChange),
        );
        assert_eq!(
            editor.preview.caret(),
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
        save_comment(&mut editor, "keep me");

        let _ = update(&mut editor, Message::Preview(preview::Message::Toggle));
        assert_eq!(editor.document.content().cursor(), source_cursor);
        assert_projection_matches(&editor, contents);
        assert_eq!(
            editor.preview.caret(),
            CaretPosition {
                element: 2,
                column: 0,
            }
        );
        assert!(editor.preview.visual_selection().is_none());
        assert_eq!(editor.comments.len(), 1);
    }

    /// Help does not move the source cursor/selection or the preview
    /// caret/visual anchor when it opens and closes.
    #[test]
    fn help_preserves_underlying_editor_state() {
        use iced::widget::text_editor::{Cursor, Position};

        let mut source = Editor {
            document: super::document::State::new("first\nsecond", None),
            preview: preview::State::new("first\nsecond"),
            keymap: Keymap::new(false),
            comments: comments::State::new(),
            find: find::State::new(),
            help: super::help::Help::new(),
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
        preview.preview.place_caret(CaretPosition {
            element: 0,
            column: 1,
        });
        enter_visual(&mut preview);
        preview.preview.place_caret(CaretPosition {
            element: 1,
            column: 2,
        });
        let caret = preview.preview.caret();
        let selection = preview.preview.visual_selection();
        let _ = update(&mut preview, Message::OpenHelp);
        let _ = update(&mut preview, Message::Help(super::help::Message::Close));
        assert_eq!(preview.preview.caret(), caret);
        assert_eq!(preview.preview.visual_selection(), selection);
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
        save_comment(&mut editor, "note");

        // The caret wanders off before the card is clicked.
        editor.preview.place_caret(CaretPosition {
            element: 0,
            column: 0,
        });

        let _ = update(
            &mut editor,
            Message::Comments(comments::Message::ActivateCard(0, 0)),
        );

        assert_eq!(
            editor.preview.caret(),
            CaretPosition {
                element: 2,
                column: 3,
            }
        );
        assert_eq!(editor.comments.mark_for(2, 32), Mark::Active);

        // In write mode the source cursor lands on the anchored element's
        // source instead.
        editor.keymap.note(Transition::PreviewToggled);
        let _ = update(
            &mut editor,
            Message::Comments(comments::Message::ActivateCard(0, 0)),
        );

        assert_eq!(
            editor.document.content().cursor().position,
            Position { line: 4, column: 0 }
        );
    }

    /// The app applies find navigation to only the active editing surface:
    /// preview events place its caret, while source events select source text.
    #[test]
    fn find_navigation_routes_to_source_and_preview_features() {
        use iced::widget::text_editor::Position;

        let mut editor = editor_at(
            "# Title\n\nbody text\n\nmore text",
            CaretPosition {
                element: 0,
                column: 0,
            },
        );
        editor.keymap.note(Transition::FindOpened);

        let _ = update(
            &mut editor,
            Message::Find(find::Message::QueryChanged("more".to_owned())),
        );
        assert_eq!(
            editor.preview.caret(),
            CaretPosition {
                element: 2,
                column: 0,
            }
        );
        assert_eq!(
            editor.document.content().cursor().position,
            Position { line: 0, column: 0 }
        );

        editor.keymap.note(Transition::PreviewToggled);
        let preview_caret = editor.preview.caret();
        let _ = update(
            &mut editor,
            Message::Find(find::Message::QueryChanged("text".to_owned())),
        );
        let cursor = editor.document.content().cursor();
        assert_eq!(cursor.position, Position { line: 2, column: 5 });
        assert_eq!(cursor.selection, Some(Position { line: 2, column: 9 }));
        assert_eq!(editor.preview.caret(), preview_caret);
    }
}
