//! Application composition and cross-feature coordination.
mod shell;

use crate::cli::Args;
use crate::input::{
    Command, CommentsCommand, DocumentCommand, FindCommand, GuardAction, HelpCommand, Keymap,
    PreviewCommand, Transition,
};
use crate::theme::Palette;
use crate::{comments, document, editing, find, help, highlight, input, preview, theme, ui};

use iced::widget::{operation::focus, operation::focus_next, text_editor, Id};
use iced::{keyboard, Background, Border, Element, Font, Length, Subscription, Task, Theme};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
const SOURCE_EDITOR_ID: &str = "source-editor";
pub(crate) struct App {
    document: document::State,
    preview: preview::State,
    keymap: Keymap,
    comments: comments::State,
    find: find::State,
    help: help::Help,
    palette: Palette,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Document(document::Message),
    Preview(preview::Message),
    Comments(comments::Message),
    Find(find::Message),
    Help(help::Message),
    Input(Command),
    Toolbar(ui::toolbar::Message),
    Theme(theme::Event),
}

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
        Command::Help(command) => Message::Help(match command {
            HelpCommand::Open => help::Message::Open,
            HelpCommand::Close => help::Message::Close,
        }),
    }
}

fn message_for_toolbar(message: ui::toolbar::Message) -> Message {
    match message {
        ui::toolbar::Message::Open => Message::Document(document::Message::OpenRequested),
        ui::toolbar::Message::Save => Message::Document(document::Message::SaveRequested),
        ui::toolbar::Message::TogglePreview => Message::Preview(preview::Message::Toggle),
    }
}

fn message_for_guard_action(action: GuardAction) -> Message {
    match action {
        GuardAction::OpenHelp => Message::Help(help::Message::Open),
        GuardAction::CloseHelp => Message::Help(help::Message::Close),
        GuardAction::CloseFind => Message::Find(find::Message::Close),
        GuardAction::CloseNote => Message::Comments(comments::Message::CloseComposer),
        GuardAction::CancelUnsaved => Message::Document(document::Message::UnsavedCancel),
        GuardAction::Pass | GuardAction::Capture => {
            unreachable!("non-dispatch guard actions are handled inside input::guard")
        }
    }
}

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
        Message::Help(help::Message::Open) => Transition::HelpOpened,
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

pub(crate) fn subscription(editor: &App) -> Subscription<Message> {
    let keys = keyboard::listen()
        .with(editor.keymap)
        .filter_map(|(keymap, event)| keymap.handle(event))
        .map(Message::Input);

    let close = iced::window::close_requests()
        .map(document::Message::CloseRequested)
        .map(Message::Document);

    let theme = theme::subscription().map(Message::Theme);

    match editor.document.path() {
        Some(path) => Subscription::batch([
            keys,
            close,
            document::subscription(path).map(Message::Document),
            theme,
        ]),
        None => Subscription::batch([keys, close, theme]),
    }
}

fn handle_document_event(editor: &mut App, event: document::Event) -> Task<Message> {
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

fn handle_preview_event(editor: &mut App, event: preview::Event) -> Task<Message> {
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
        preview::Event::OpenLink(_uri) => Task::none(),
    }
}

fn handle_comments_event(editor: &mut App, event: comments::Event) -> Task<Message> {
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
        comments::Event::PublishRequested => Task::none(),
    }
}

fn handle_find_event(editor: &mut App, event: find::Event) -> Task<Message> {
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

pub(crate) fn update(editor: &mut App, message: Message) -> Task<Message> {
    let message = match message {
        Message::Input(command) => message_for_command(command),
        Message::Toolbar(message) => message_for_toolbar(message),
        message => message,
    };

    let note_was_open = editor.keymap.note_open();
    editor.keymap.note(input_transition(&message));

    match message {
        Message::Document(message) => {
            let document::Update { task, event } = document::update(&mut editor.document, message);
            let event_task =
                event.map_or_else(Task::none, |event| handle_document_event(editor, event));

            return Task::batch([task.map(Message::Document), event_task]);
        }
        Message::Theme(theme::Event::Changed) => editor.palette = Palette::current(),
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
                focus(Id::new(SOURCE_EDITOR_ID))
            } else {
                Task::none()
            };

            return Task::batch([task.map(Message::Find), event_task, restore_task]);
        }
        Message::Help(message) => {
            let opened = matches!(message, help::Message::Open);
            let closed = matches!(message, help::Message::Close);
            editor.help.update(message);

            if opened {
                return help::focus_input();
            }

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
        Message::Input(_) | Message::Toolbar(_) => {
            unreachable!("boundary messages are translated before delegation")
        }
    }

    Task::none()
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

pub(crate) fn view(editor: &App) -> Element<'_, Message> {
    let palette = editor.palette;

    let source = editor.document.text();
    let find_surface = if editor.keymap.preview() {
        find::Surface::Preview(editor.preview.elements())
    } else {
        find::Surface::Source(&source)
    };

    let base_area: Element<'_, Message> = if editor.keymap.preview() {
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

    let note = editor
        .keymap
        .note_open()
        .then(|| comments::composer::view(&editor.comments, palette).map(Message::Comments));
    let editing_area = shell::stack_layers(base_area, note);

    let toolbar = ui::toolbar::view(ui::toolbar::Model::new(
        editor.keymap.preview(),
        editor.keymap.preview_only(),
        editor.keymap.mode(),
        editor.keymap.pending_count(),
        palette,
    ))
    .map(Message::Toolbar);
    let sidebar = comments::sidebar::view(
        &editor.comments,
        comments::sidebar::ViewContext {
            source: &source,
            preview_elements: editor.preview.elements(),
            palette,
            font: EDITOR_FONT,
        },
    )
    .map(Message::Comments);
    let base = shell::layout(editing_area, toolbar, sidebar, palette.background);

    let find = editor
        .keymap
        .find_open()
        .then(|| find::view(&editor.find, find_surface, palette).map(Message::Find));
    let unsaved = editor.document.pending_action().and_then(|action| {
        editor
            .keymap
            .unsaved_open()
            .then(|| document::unsaved_view::view(action, palette).map(Message::Document))
    });
    let help = editor
        .keymap
        .help_open()
        .then(|| help::view(&editor.help, palette).map(Message::Help));
    let layers = shell::stack_layers(base, find.into_iter().chain(unsaved).chain(help));

    input::guard(layers, editor.keymap, message_for_guard_action)
}

pub(crate) fn boot(args: &Args) -> (App, Task<Message>) {
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

    let editor = App {
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

pub(crate) fn theme(editor: &App) -> Theme {
    if editor.palette.light {
        Theme::Light
    } else {
        Theme::Dark
    }
}

#[cfg(test)]
mod tests;
