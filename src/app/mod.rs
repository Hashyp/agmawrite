//! Application composition and cross-feature coordination.
mod coordination;
mod shell;
mod status;

use crate::cli::Args;
use crate::input::{
    Command, CommentsCommand, DocumentCommand, FindCommand, HelpCommand, InputMessage,
    InteractionState, Overlay, PreviewCommand, Surface,
};
use crate::theme::Palette;
use crate::{comments, document, find, help, highlight, input, preview, theme, ui};
use coordination::{
    focus_target, handle_comments_event, handle_document_event, handle_find_event,
    handle_preview_event,
};

use iced::widget::{operation::focus_next, text_editor, Id};
use iced::{Background, Border, Element, Font, Length, Subscription, Task, Theme};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
const SOURCE_EDITOR_ID: &str = "source-editor";
pub(crate) struct App {
    document: document::State,
    preview: preview::State,
    interaction: InteractionState,
    comments: comments::State,
    find: find::State,
    help: help::Help,
    palette: Palette,
    status_metadata: ui::status_bar::metadata::Metadata,
    /// The transient yank report, like Neovim's `N characters yanked`
    /// cmdline message: shown by the status bar until the next interaction.
    report: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Document(document::Message),
    Preview(preview::Message),
    Comments(comments::Message),
    Find(find::Message),
    Help(help::Message),
    Input(InputMessage),
    Toolbar(ui::toolbar::Message),
    StatusBar(ui::status_bar::Message),
    Theme(theme::Event),
    StatusMetadata(ui::status_bar::metadata::Metadata),
}

fn message_for_command(command: Command) -> Message {
    match command {
        Command::Document(command) => Message::Document(match command {
            DocumentCommand::Open => document::Message::OpenRequested,
            DocumentCommand::Save => document::Message::SaveRequested,
            DocumentCommand::CancelUnsaved => document::Message::UnsavedCancel,
            DocumentCommand::ConfirmUnsaved => document::Message::UnsavedSave,
            DocumentCommand::UnsavedNext => document::Message::UnsavedFocusNext,
            DocumentCommand::UnsavedPrevious => document::Message::UnsavedFocusPrevious,
        }),
        Command::Preview(command) => Message::Preview(match command {
            PreviewCommand::Toggle => preview::Message::Toggle,
            PreviewCommand::Move(motion, count) => preview::Message::Move(motion, count),
            PreviewCommand::MoveWord(motion, count) => preview::Message::MoveWord(motion, count),
            PreviewCommand::Jump(jump, count) => preview::Message::Jump(jump, count),
            PreviewCommand::Cancel => preview::Message::Cancel,
            PreviewCommand::ToggleVisual => preview::Message::ToggleVisual,
            PreviewCommand::Yank => preview::Message::Yank,
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

pub(crate) fn subscription(editor: &App) -> Subscription<Message> {
    let close = iced::window::close_requests()
        .map(document::Message::CloseRequested)
        .map(Message::Document);

    // The yanked span flashes for its moment, then one tick ends it.
    let flash = if editor.preview.flash_active() {
        preview::flash_subscription().map(Message::Preview)
    } else {
        Subscription::none()
    };

    let theme = theme::subscription().map(Message::Theme);
    let metadata =
        ui::status_bar::metadata::subscription(editor.document.path().map(ToOwned::to_owned))
            .map(Message::StatusMetadata);
    let theme = Subscription::batch([theme, metadata]);

    match editor.document.path() {
        Some(path) => Subscription::batch([
            close,
            document::subscription(path).map(Message::Document),
            theme,
            flash,
        ]),
        None => Subscription::batch([close, theme, flash]),
    }
}

pub(crate) fn update(editor: &mut App, message: Message) -> Task<Message> {
    // The yank report is a cmdline message: it survives background events
    // and the flash ticker, and the next interaction replaces it — the
    // yank itself writes the new one.
    if !matches!(
        message,
        Message::StatusMetadata(_)
            | Message::Theme(_)
            | Message::Preview(preview::Message::Yank)
            | Message::Preview(preview::Message::ClearFlash)
            | Message::Preview(preview::Message::ScrollBy(_))
    ) {
        editor.report = None;
    }

    let message = match message {
        Message::Input(InputMessage::Execute(command)) => message_for_command(command),
        Message::Input(InputMessage::ArmPrefix(prefix)) => {
            editor.interaction.arm_prefix(prefix);
            return Task::none();
        }
        Message::Input(InputMessage::PushCountDigit(digit)) => {
            editor.interaction.push_count_digit(digit);
            return Task::none();
        }
        Message::Toolbar(message)
        | Message::StatusBar(ui::status_bar::Message::Toolbar(message)) => {
            message_for_toolbar(message)
        }
        Message::StatusBar(ui::status_bar::Message::ToggleComments) => {
            Message::Comments(comments::Message::ToggleSidebar)
        }
        message => message,
    };

    match message {
        Message::Document(document::Message::UnsavedCancel) => {
            let Some(resolution) = editor.interaction.resolve_unsaved() else {
                return Task::none();
            };
            focus_target(resolution.focus())
        }
        Message::Document(
            prompt @ (document::Message::UnsavedSave | document::Message::UnsavedDiscard),
        ) => {
            let Some(resolution) = editor.interaction.resolve_unsaved() else {
                return Task::none();
            };
            let continuation = match prompt {
                document::Message::UnsavedSave => document::Message::SaveThen(resolution.action()),
                document::Message::UnsavedDiscard => {
                    document::Message::RunUnsavedAction(resolution.action())
                }
                _ => unreachable!(),
            };
            let document::Update { task, event } =
                document::update(&mut editor.document, continuation);
            let event_task =
                event.map_or_else(Task::none, |event| handle_document_event(editor, event));
            Task::batch([
                task.map(Message::Document),
                event_task,
                focus_target(resolution.focus()),
            ])
        }
        Message::Document(message) => {
            let document::Update { task, event } = document::update(&mut editor.document, message);
            let event_task =
                event.map_or_else(Task::none, |event| handle_document_event(editor, event));
            editor.interaction.activity();
            Task::batch([task.map(Message::Document), event_task])
        }
        Message::StatusMetadata(metadata) => {
            editor.status_metadata = metadata;
            Task::none()
        }
        Message::Theme(theme::Event::Changed) => {
            editor.palette = Palette::current();
            editor.interaction.activity();
            Task::none()
        }
        Message::Preview(message @ preview::Message::ToggleVisual) => {
            let mut accepted = editor.interaction;
            if accepted.toggle_visual().is_err() {
                return Task::none();
            }
            let context = preview::Context {
                visual_active: accepted.view().visual(),
            };
            let preview::Update { task, event } =
                preview::update(&mut editor.preview, message, context);
            debug_assert!(event.is_none());
            editor.interaction = accepted;
            task.map(Message::Preview)
        }
        Message::Preview(message @ preview::Message::Cancel) => {
            let mut accepted = editor.interaction;
            if accepted.cancel_preview().is_err() {
                return Task::none();
            }
            let context = preview::Context {
                visual_active: accepted.view().visual(),
            };
            let preview::Update { task, event } =
                preview::update(&mut editor.preview, message, context);
            debug_assert!(event.is_none());
            editor.interaction = accepted;
            task.map(Message::Preview)
        }
        Message::Preview(message @ preview::Message::Yank) => {
            // `y` is a visual-mode verb, like vim: it yanks the selection
            // and immediately leaves visual mode, spending any pending
            // count like any completed command.
            if !editor.interaction.view().visual() {
                return Task::none();
            }

            let mut accepted = editor.interaction;
            if accepted.toggle_visual().is_err() {
                return Task::none();
            }
            accepted.activity();

            let context = preview::Context {
                visual_active: accepted.view().visual(),
            };
            let preview::Update { task, event } =
                preview::update(&mut editor.preview, message, context);
            editor.interaction = accepted;
            let event_task =
                event.map_or_else(Task::none, |event| handle_preview_event(editor, event));
            Task::batch([task.map(Message::Preview), event_task])
        }
        Message::Preview(message) => {
            let toggles_surface = matches!(message, preview::Message::Toggle);
            let context = preview::Context {
                visual_active: editor.interaction.view().visual(),
            };
            let preview::Update { task, event } =
                preview::update(&mut editor.preview, message, context);
            let event_task =
                event.map_or_else(Task::none, |event| handle_preview_event(editor, event));
            if !toggles_surface {
                editor.interaction.activity();
            }
            Task::batch([task.map(Message::Preview), event_task])
        }
        Message::Comments(message) => {
            if matches!(
                &message,
                comments::Message::OpenComposer | comments::Message::EditActive
            ) {
                let mut legal = editor.interaction;
                if legal.open_note().is_err() {
                    return Task::none();
                }
            }

            let interaction = editor.interaction.view();
            let context = comments::Context {
                caret: editor.preview.caret(),
                selection: editor.preview.visual_selection(),
                composer_open: interaction.contains(Overlay::Note),
            };
            let comments::Update { event } =
                comments::update(&mut editor.comments, message, context);
            event.map_or_else(Task::none, |event| handle_comments_event(editor, event))
        }
        Message::Find(message) => {
            let source;
            let surface = match editor.interaction.view().surface() {
                Surface::Preview => find::Surface::Preview(editor.preview.elements()),
                Surface::Write => {
                    source = editor.document.text();
                    find::Surface::Source(&source)
                }
            };
            let find::Update { task, event } = find::update(&mut editor.find, message, surface);
            let event_task =
                event.map_or_else(Task::none, |event| handle_find_event(editor, event));
            Task::batch([task.map(Message::Find), event_task])
        }
        Message::Help(help::Message::Open) => {
            editor.help.update(help::Message::Open);
            editor.interaction.open_help();
            help::focus_input()
        }
        Message::Help(help::Message::Close) => {
            if !matches!(editor.interaction.view().overlay(), Some(Overlay::Help)) {
                return Task::none();
            }
            editor.help.update(help::Message::Close);
            let target = editor.interaction.close_help();
            focus_target(target)
        }
        Message::Help(message) => {
            editor.help.update(message);
            Task::none()
        }
        Message::Input(_) | Message::Toolbar(_) | Message::StatusBar(_) => {
            unreachable!("boundary messages are translated before delegation")
        }
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

pub(crate) fn view(editor: &App) -> Element<'_, Message> {
    let palette = editor.palette;
    let interaction = editor.interaction.view();

    let source = editor.document.text();
    let find_surface = match interaction.surface() {
        Surface::Preview => find::Surface::Preview(editor.preview.elements()),
        Surface::Write => find::Surface::Source(&source),
    };

    let base_area: Element<'_, Message> = match interaction.surface() {
        Surface::Preview => {
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
        }
        Surface::Write => text_editor(editor.document.content())
            .id(Id::new(SOURCE_EDITOR_ID))
            .on_action(|action| Message::Document(document::Message::Edit(action)))
            .font(EDITOR_FONT)
            .size(crate::typography::TEXT_SIZE)
            .height(Length::Fill)
            .padding(0)
            .line_height(crate::typography::LINE_HEIGHT)
            .highlight_with::<highlight::MarkdownMarkers>(
                highlight::Settings {
                    query: editor.find.query().to_owned(),
                    palette,
                },
                highlight::format,
            )
            .style(move |theme, status| editor_style(&palette, theme, status))
            .into(),
    };

    let note = interaction
        .contains(Overlay::Note)
        .then(|| comments::composer::view(&editor.comments, palette).map(Message::Comments));
    let editing_area = shell::stack_layers(base_area, note);

    // The status bar spans every surface; the old bottom-toolbar slot
    // keeps only its 1/8 spacing, like it already did on the preview.
    let toolbar = iced::widget::Space::new().into();
    let sidebar = comments::sidebar::view(
        &editor.comments,
        comments::sidebar::ViewContext {
            source: &source,
            preview_elements: editor.preview.elements(),
            palette,
            font: EDITOR_FONT,
            // The comments rail belongs to the preview surface — comments
            // are made there. Write mode keeps its right edge clean; Ctrl + B
            // still toggles the sidebar from either surface.
            collapsed_rail: matches!(interaction.surface(), Surface::Preview),
        },
    )
    .map(Message::Comments);
    let base = shell::layout(editing_area, toolbar, sidebar, palette.background);
    // The document controls float at the window's top-left corner, over
    // the body's empty top margin — but only in an editable session: a
    // preview-only one (`--preview`) shows no corner controls at all.
    let base = if interaction.can_toggle_preview() {
        let controls = ui::toolbar::view(ui::toolbar::Model::new(
            editor.document.is_modified(),
            palette,
        ))
        .map(Message::Toolbar);
        shell::with_corner_controls(base, controls)
    } else {
        base
    };
    let base = shell::with_status_bar(
        base,
        ui::status_bar::view(status::model(editor, &source)).map(Message::StatusBar),
    );

    let find = interaction
        .contains(Overlay::Find)
        .then(|| find::view(&editor.find, find_surface, palette).map(Message::Find));
    let unsaved = interaction.unsaved_action().map(|action| {
        document::unsaved_view::view(action, editor.document.unsaved_focus(), palette)
            .map(Message::Document)
    });
    let help = interaction
        .contains(Overlay::Help)
        .then(|| help::view(&editor.help, palette).map(Message::Help));
    let layers = shell::stack_layers(base, find.into_iter().chain(unsaved).chain(help));

    input::guard(layers, editor.interaction, Message::Input)
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
        interaction: if args.preview {
            InteractionState::preview_only()
        } else {
            InteractionState::editable()
        },
        comments: comments::State::new(),
        find: find::State::new(),
        help: help::Help::new(),
        palette: Palette::current(),
        status_metadata: ui::status_bar::metadata::Metadata::default(),
        report: None,
    };

    let task = if args.preview {
        Task::none()
    } else {
        focus_next()
    };

    (editor, task)
}

pub(crate) fn theme(editor: &App) -> Theme {
    editor.palette.runtime_theme()
}

#[cfg(test)]
mod tests;
