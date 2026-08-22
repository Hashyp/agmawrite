use super::comments::{self, Mark};
use super::find;
use super::input::{InputMessage, InteractionState, Overlay, Surface};
use super::preview::{self, CaretPosition};
use super::theme::Palette;
use super::ui::toolbar;
use super::{message_for_toolbar, update, App, Message};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootStackEntry {
    Base,
    Note,
    Find,
    Unsaved,
    Help,
    InputGuard,
}

fn root_stack_order(app: &App) -> Vec<RootStackEntry> {
    let interaction = app.interaction.view();
    let mut stack = vec![RootStackEntry::Base];

    if interaction.contains(Overlay::Note) {
        stack.push(RootStackEntry::Note);
    }
    if interaction.contains(Overlay::Find) {
        stack.push(RootStackEntry::Find);
    }
    if interaction.unsaved_action().is_some() {
        stack.push(RootStackEntry::Unsaved);
    }
    if interaction.contains(Overlay::Help) {
        stack.push(RootStackEntry::Help);
    }

    stack.push(RootStackEntry::InputGuard);
    stack
}

fn is_preview(app: &App) -> bool {
    matches!(app.interaction.view().surface(), Surface::Preview)
}

fn pending_count(app: &App) -> u32 {
    app.interaction
        .view()
        .toolbar()
        .pending_count()
        .map_or(0, std::num::NonZeroU32::get)
}

fn app_at(contents: &str, position: CaretPosition) -> App {
    let mut interaction = InteractionState::editable();
    assert!(interaction.toggle_preview());

    let mut preview = preview::State::new(contents);
    preview.place_caret(position);

    App {
        document: super::document::State::new(contents, None),
        preview,
        interaction,
        comments: comments::State::new(),
        find: find::State::new(),
        help: super::help::Help::new(),
        palette: Palette::default(),
    }
}

fn set_composer_text(editor: &mut App, text: &str) {
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

fn save_comment(editor: &mut App, text: &str) {
    editor.interaction.open_note().unwrap();
    set_composer_text(editor, text);
    let _ = update(editor, Message::Comments(comments::Message::SaveComposer));
}

fn prepare_comment_edit(editor: &mut App, saved: &str, draft: &str) {
    save_comment(editor, saved);
    let _ = update(editor, Message::Comments(comments::Message::EditActive));
    set_composer_text(editor, draft);
}

fn assert_projection_matches(editor: &App, contents: &str) {
    let expected = preview::State::new(contents);
    assert_eq!(editor.preview.elements(), expected.elements());

    let expected_markdown = iced::widget::markdown::Content::parse(contents);
    assert_eq!(
        format!("{:?}", editor.preview.markdown().items()),
        format!("{:?}", expected_markdown.items())
    );
}

fn enter_visual(editor: &mut App) {
    let _ = update(editor, Message::Preview(preview::Message::ToggleVisual));
}

#[test]
fn toolbar_actions_map_to_document_and_preview_interactions() {
    assert!(matches!(
        message_for_toolbar(toolbar::Message::Open),
        Message::Document(super::document::Message::OpenRequested)
    ));
    assert!(matches!(
        message_for_toolbar(toolbar::Message::Save),
        Message::Document(super::document::Message::SaveRequested)
    ));
    assert!(matches!(
        message_for_toolbar(toolbar::Message::TogglePreview),
        Message::Preview(preview::Message::Toggle)
    ));

    let mut editor = app_at(
        "body",
        CaretPosition {
            element: 0,
            column: 0,
        },
    );
    assert!(is_preview(&editor));

    let _ = update(
        &mut editor,
        Message::Toolbar(toolbar::Message::TogglePreview),
    );
    assert!(!is_preview(&editor));
}

#[test]
fn rejected_feature_operations_do_not_transition_interaction_state() {
    let mut editor = App {
        document: super::document::State::new("body", None),
        preview: preview::State::new("body"),
        interaction: InteractionState::editable(),
        comments: comments::State::new(),
        find: find::State::new(),
        help: super::help::Help::new(),
        palette: Palette::default(),
    };

    let _ = update(
        &mut editor,
        Message::Comments(comments::Message::OpenComposer),
    );
    assert!(!editor.interaction.view().contains(Overlay::Note));

    let _ = update(
        &mut editor,
        Message::Comments(comments::Message::EditActive),
    );
    assert!(!editor.interaction.view().contains(Overlay::Note));

    let _ = update(
        &mut editor,
        Message::Preview(preview::Message::ToggleVisual),
    );
    assert_eq!(editor.interaction.view().surface(), Surface::Write);
    assert!(!editor.interaction.view().visual());

    assert!(editor.interaction.toggle_preview());
    save_comment(&mut editor, "saved note");
    assert!(editor.interaction.toggle_preview());
    let _ = update(
        &mut editor,
        Message::Comments(comments::Message::EditActive),
    );
    assert!(editor.comments.editing_target().is_none());
    assert!(!editor.interaction.view().contains(Overlay::Note));
}

#[test]
fn input_parser_messages_preserve_multi_digit_counts_without_entering_preview() {
    let mut editor = app_at(
        "one\n\ntwo",
        CaretPosition {
            element: 0,
            column: 0,
        },
    );
    let caret = editor.preview.caret();

    let _ = update(&mut editor, Message::Input(InputMessage::PushCountDigit(1)));
    let _ = update(&mut editor, Message::Input(InputMessage::PushCountDigit(0)));

    assert_eq!(pending_count(&editor), 10);
    assert_eq!(editor.preview.caret(), caret);
}

#[test]
fn file_load_policy_replaces_source_and_resets_document_bound_state() {
    let mut editor = app_at(
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
    let mut editor = app_at(
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

    let mut editor = app_at(
        "stale",
        CaretPosition {
            element: 0,
            column: 3,
        },
    );
    assert!(editor.interaction.toggle_preview());
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
    assert!(editor.interaction.toggle_preview());
    save_comment(&mut editor, "keep me");
    assert!(editor.interaction.toggle_preview());

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

    let mut source = App {
        document: super::document::State::new("first\nsecond", None),
        preview: preview::State::new("first\nsecond"),
        interaction: InteractionState::editable(),
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
    let _ = update(&mut source, Message::Help(super::help::Message::Open));
    let _ = update(&mut source, Message::Help(super::help::Message::Close));
    assert_eq!(source.document.content().cursor(), before);

    let mut preview = app_at(
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
    let _ = update(&mut preview, Message::Help(super::help::Message::Open));
    let _ = update(&mut preview, Message::Help(super::help::Message::Close));
    assert_eq!(preview.preview.caret(), caret);
    assert_eq!(preview.preview.visual_selection(), selection);
}

/// The unsaved modal is composed from the real document and input
/// state above the persistent editing surface and below Help.
#[test]
fn unsaved_modal_is_above_editing_surface_and_below_help() {
    use iced::widget::text_editor::{Action, Edit};

    let mut editor = app_at(
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
    let _ = update(&mut editor, Message::Help(super::help::Message::Open));

    assert_eq!(
        root_stack_order(&editor),
        vec![
            RootStackEntry::Base,
            RootStackEntry::Unsaved,
            RootStackEntry::Help,
            RootStackEntry::InputGuard,
        ]
    );
    assert_eq!(
        editor.interaction.view().unsaved_action(),
        Some(super::document::UnsavedAction::OpenFile)
    );

    let _ = update(&mut editor, Message::Help(super::help::Message::Close));
    assert_eq!(
        editor.interaction.view().unsaved_action(),
        Some(super::document::UnsavedAction::OpenFile)
    );
    let _ = update(
        &mut editor,
        Message::Document(super::document::Message::UnsavedCancel),
    );
    assert!(editor.interaction.view().unsaved_action().is_none());
}

#[test]
fn unsaved_save_reopens_the_same_action_after_a_stale_async_completion() {
    use iced::widget::text_editor::{Action, Edit};

    let mut editor = app_at(
        "draft",
        CaretPosition {
            element: 0,
            column: 0,
        },
    );
    let _ = update(
        &mut editor,
        Message::Document(super::document::Message::OpenLoaded(Some((
            "/tmp/phase-3-stale-save.md".into(),
            "draft".to_owned(),
        )))),
    );
    let _ = update(
        &mut editor,
        Message::Document(super::document::Message::Edit(Action::Edit(Edit::Insert(
            '!',
        )))),
    );
    let id = iced::window::Id::unique();
    let _ = update(
        &mut editor,
        Message::Document(super::document::Message::CloseRequested(id)),
    );
    assert_eq!(
        editor.interaction.view().unsaved_action(),
        Some(super::document::UnsavedAction::CloseWindow(id))
    );

    let snapshot = editor.document.text();
    let _ = update(
        &mut editor,
        Message::Document(super::document::Message::UnsavedSave),
    );
    assert!(editor.interaction.view().unsaved_action().is_none());

    let _ = update(
        &mut editor,
        Message::Document(super::document::Message::Edit(Action::Edit(Edit::Insert(
            '?',
        )))),
    );
    let _ = update(
        &mut editor,
        Message::Document(super::document::Message::Saved(
            Ok("/tmp/phase-3-stale-save.md".into()),
            snapshot,
        )),
    );

    assert_eq!(
        editor.interaction.view().unsaved_action(),
        Some(super::document::UnsavedAction::CloseWindow(id))
    );
}

#[test]
fn root_features_keep_base_note_find_unsaved_help_guard_order() {
    use iced::widget::text_editor::{Action, Edit};

    let mut editor = app_at(
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
    editor.interaction.open_note().unwrap();
    editor.interaction.open_find();
    editor.interaction.open_help();

    assert_eq!(
        root_stack_order(&editor),
        vec![
            RootStackEntry::Base,
            RootStackEntry::Note,
            RootStackEntry::Find,
            RootStackEntry::Unsaved,
            RootStackEntry::Help,
            RootStackEntry::InputGuard,
        ]
    );
}

#[test]
fn closing_find_above_note_restores_the_note_layer() {
    let mut editor = app_at(
        "draft",
        CaretPosition {
            element: 0,
            column: 0,
        },
    );
    let _ = update(
        &mut editor,
        Message::Comments(comments::Message::OpenComposer),
    );
    let _ = update(&mut editor, Message::Find(find::Message::Open));

    assert_eq!(
        root_stack_order(&editor),
        vec![
            RootStackEntry::Base,
            RootStackEntry::Note,
            RootStackEntry::Find,
            RootStackEntry::InputGuard,
        ]
    );

    let _ = update(&mut editor, Message::Find(find::Message::Close));

    assert_eq!(
        root_stack_order(&editor),
        vec![
            RootStackEntry::Base,
            RootStackEntry::Note,
            RootStackEntry::InputGuard,
        ]
    );
}

#[test]
fn preview_toggle_preserves_an_open_find_across_surfaces() {
    let mut editor = app_at(
        "draft",
        CaretPosition {
            element: 0,
            column: 0,
        },
    );
    let _ = update(&mut editor, Message::Find(find::Message::Open));

    let _ = update(&mut editor, Message::Preview(preview::Message::Toggle));
    assert!(!is_preview(&editor));
    assert!(editor.interaction.view().contains(Overlay::Find));

    let _ = update(&mut editor, Message::Preview(preview::Message::Toggle));
    assert!(is_preview(&editor));
    assert!(editor.interaction.view().contains(Overlay::Find));
}

/// Clicking a comment card activates its comment without moving the
/// preview caret onto it — the framed anchor scrolls into view instead —
/// while in write mode the source cursor lands on the element's source.
#[test]
fn clicking_a_comment_card_activates_without_placing_the_caret() {
    use iced::widget::text_editor::Position;

    let mut editor = app_at(
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

    // The caret stays where the reader left it; the comment itself is
    // active, framing its element in the preview.
    assert_eq!(
        editor.preview.caret(),
        CaretPosition {
            element: 0,
            column: 0,
        }
    );
    assert_eq!(editor.comments.mark_for(2, 32), Mark::Active);
    assert!(editor.comments.anchors_element(2, 32));

    // In write mode the source cursor lands on the anchored element's
    // source instead.
    assert!(editor.interaction.toggle_preview());
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

    let mut editor = app_at(
        "# Title\n\nbody text\n\nmore text",
        CaretPosition {
            element: 0,
            column: 0,
        },
    );
    editor.interaction.open_find();

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

    assert!(editor.interaction.toggle_preview());
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
