//! Successful cross-feature boundaries and payload-driven focus restoration.

use super::{App, Message, SOURCE_EDITOR_ID};
use crate::input::{FocusTarget, Surface};
use crate::{comments, document, editing, find, help, preview};
use iced::widget::{operation::focus, text_editor, Id};
use iced::Task;

pub(super) fn focus_target(target: Option<FocusTarget>) -> Task<Message> {
    match target {
        Some(FocusTarget::SourceEditor) => focus(Id::new(SOURCE_EDITOR_ID)),
        Some(FocusTarget::NoteComposer) => comments::focus_composer().map(Message::Comments),
        Some(FocusTarget::FindInput) => find::focus_input().map(Message::Find),
        Some(FocusTarget::HelpInput) => help::focus_input(),
        None => Task::none(),
    }
}

pub(super) fn handle_document_event(editor: &mut App, event: document::Event) -> Task<Message> {
    match event {
        document::Event::SourceReplaced { reason } => {
            let source = editor.document.text();

            match reason {
                document::SourceReplacement::Loaded => {
                    editor.preview.load_source(&source);
                    editor.comments = comments::State::new();
                    editor.interaction.document_loaded();
                    Task::none()
                }
                document::SourceReplacement::External => {
                    editor.preview.replace_source(&source);

                    if matches!(editor.interaction.view().surface(), Surface::Preview) {
                        preview::reveal_caret().map(Message::Preview)
                    } else {
                        Task::none()
                    }
                }
            }
        }
        document::Event::CloseWindow(id) => iced::window::close(id),
        document::Event::UnsavedConfirmationRequested(action) => {
            editor.interaction.open_unsaved(action);
            Task::none()
        }
    }
}

pub(super) fn handle_preview_event(editor: &mut App, event: preview::Event) -> Task<Message> {
    match event {
        preview::Event::ToggleRequested => {
            let interaction = editor.interaction.view();
            if !interaction.can_toggle_preview() {
                return Task::none();
            }

            let surface_task = match interaction.surface() {
                Surface::Write => {
                    let source = editor.document.text();
                    editor
                        .preview
                        .refresh_from_source(&source, editor.document.content());
                    preview::reveal_caret().map(Message::Preview)
                }
                Surface::Preview => {
                    // The caret mirrors into the write cursor: leaving the
                    // preview lands the source cursor exactly where the
                    // caret read.
                    editor.preview.clear_visual_selection();
                    let source = editor.document.text();
                    if let Some(offset) = editor.preview.caret_source_offset(&source) {
                        editor.document.move_to(text_editor::Cursor {
                            position: editing::position_at(&source, offset),
                            selection: None,
                        });
                    }
                    Task::none()
                }
            };

            let toggled = editor.interaction.toggle_preview();
            debug_assert!(toggled);
            Task::batch([
                surface_task,
                focus_target(editor.interaction.focus_target()),
            ])
        }
        preview::Event::OpenLink(_uri) => Task::none(),
        preview::Event::Yanked { text } => {
            // Like `clipboard=unnamedplus` with Neovim's yank report: the
            // text lands on the system clipboard and the status bar reports
            // the count until the next key press replaces it.
            editor.report = Some(super::status::yank_report(&text));
            iced::clipboard::write(text)
        }
    }
}

pub(super) fn handle_comments_event(editor: &mut App, event: comments::Event) -> Task<Message> {
    match event {
        comments::Event::NavigateTo(anchor) => {
            if matches!(editor.interaction.view().surface(), Surface::Preview) {
                // Activation frames the commented text with its rectangle and
                // scrolls it into view — no caret is dropped onto the
                // selection's start.
                editor.preview.clear_visual_selection();
                preview::reveal_anchor().map(Message::Preview)
            } else {
                let source = editor.document.text();
                // Column 0 of the element aligns onto its first rendered
                // character, so the cursor lands on the text, not on the
                // markup before it — the same place the caret mirrors to.
                let caret = preview::CaretPosition {
                    element: anchor.element,
                    column: 0,
                };

                match preview::caret_source_offset(&source, editor.preview.elements(), caret) {
                    Some(offset) => {
                        editor.document.move_to(text_editor::Cursor {
                            position: editing::position_at(&source, offset),
                            selection: None,
                        });
                        focus(Id::new(SOURCE_EDITOR_ID))
                    }
                    None => Task::none(),
                }
            }
        }
        comments::Event::FocusComposer => {
            if editor.interaction.open_note().is_err() {
                return Task::none();
            }
            focus_target(editor.interaction.focus_target())
        }
        comments::Event::ComposerClosed => {
            let target = editor.interaction.close_note();
            focus_target(target)
        }
        comments::Event::PublishRequested => Task::none(),
    }
}

pub(super) fn handle_find_event(editor: &mut App, event: find::Event) -> Task<Message> {
    match event {
        find::Event::Opened => {
            editor.interaction.open_find();
            Task::none()
        }
        find::Event::Closed => {
            let target = editor.interaction.close_find();
            focus_target(target)
        }
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
