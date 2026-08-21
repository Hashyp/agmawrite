//! Pure keyboard and input-method binding resolution.
//!
//! This module only interprets an interaction snapshot and an Iced event. It
//! does not mutate parser or feature state; callers apply the returned input
//! message at the application boundary.

use iced::keyboard;

use super::command::{
    Command, CommentsCommand, DocumentCommand, FindCommand, HelpCommand, PreviewCommand,
};
use super::pending::{Pending, Prefix};
use super::state::{
    EditableWorkspace, HelpResume, InteractionState, PreviewState, RootState, Workspace, WriteState,
};
use crate::help;
use crate::preview::{Jump, Motion, Page, Placement, WordMotion};

/// The root event policy selected by the current interaction variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Let the focused child widget handle the event.
    Pass,
    /// Stop the event without publishing a message.
    Capture,
    /// Publish an input message and stop the event.
    Dispatch(InputMessage),
}

/// An input consequence, separating semantic intent from parser changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputMessage {
    Execute(Command),
    ArmPrefix(Prefix),
    PushCountDigit(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalContext {
    EditableWrite,
    EditableWriteFind,
    EditablePreview(Pending),
    EditablePreviewFind,
    PreviewOnly(Pending),
    PreviewOnlyFind,
}

/// Routes one root event through the legal interaction hierarchy.
///
/// Root and workspace matching is deliberately exhaustive: modal ownership
/// follows the enum nesting instead of being reconstructed from boolean
/// projections.
pub(crate) fn route(state: &InteractionState, event: &iced::Event) -> Decision {
    match state.root {
        RootState::Help {
            resume: HelpResume::Active(_),
        }
        | RootState::Help {
            resume: HelpResume::Unsaved { .. },
        } => route_help(true, event),
        RootState::Unsaved { .. } => route_unsaved(event),
        RootState::Active(workspace) => route_workspace(workspace, event),
    }
}

fn route_workspace(workspace: Workspace, event: &iced::Event) -> Decision {
    // Help may be opened above every active workspace interaction.
    let help = route_help(false, event);
    if help != Decision::Pass {
        return help;
    }

    match workspace {
        Workspace::Editable(EditableWorkspace::Write(WriteState::Editor)) => {
            route_global(GlobalContext::EditableWrite, event)
        }
        Workspace::Editable(EditableWorkspace::Write(WriteState::Find)) => {
            fallback(route_find(event), || {
                route_global(GlobalContext::EditableWriteFind, event)
            })
        }
        Workspace::Editable(EditableWorkspace::Preview(PreviewState::Canvas {
            pending, ..
        })) => fallback(route_preview(pending, event), || {
            route_global(GlobalContext::EditablePreview(pending), event)
        }),
        Workspace::Editable(EditableWorkspace::Preview(PreviewState::Note { .. })) => {
            route_note(event)
        }
        Workspace::Editable(EditableWorkspace::Preview(PreviewState::Find { .. })) => {
            fallback(route_find(event), || {
                route_global(GlobalContext::EditablePreviewFind, event)
            })
        }
        Workspace::PreviewOnly(PreviewState::Canvas { pending, .. }) => {
            fallback(route_preview(pending, event), || {
                route_global(GlobalContext::PreviewOnly(pending), event)
            })
        }
        Workspace::PreviewOnly(PreviewState::Note { .. }) => route_note(event),
        Workspace::PreviewOnly(PreviewState::Find { .. }) => fallback(route_find(event), || {
            route_global(GlobalContext::PreviewOnlyFind, event)
        }),
    }
}

fn fallback(primary: Decision, secondary: impl FnOnce() -> Decision) -> Decision {
    match primary {
        Decision::Pass => secondary(),
        decision @ (Decision::Capture | Decision::Dispatch(_)) => decision,
    }
}

fn execute(command: Command) -> Decision {
    Decision::Dispatch(InputMessage::Execute(command))
}

fn one_shot(repeat: bool, command: Command) -> Decision {
    if repeat {
        Decision::Capture
    } else {
        execute(command)
    }
}

/// Routes Help's open/close chord and its focused-field containment policy.
fn route_help(open: bool, event: &iced::Event) -> Decision {
    match help::event_action(open, event) {
        help::EventAction::Pass => Decision::Pass,
        help::EventAction::Capture => Decision::Capture,
        help::EventAction::Open => execute(Command::Help(HelpCommand::Open)),
        help::EventAction::Close => execute(Command::Help(HelpCommand::Close)),
    }
}

/// Routes the Unsaved modal, which owns all key presses and input methods.
fn route_unsaved(event: &iced::Event) -> Decision {
    let help = route_help(false, event);
    if help != Decision::Pass {
        return help;
    }

    if matches!(event, iced::Event::InputMethod(_)) {
        return Decision::Capture;
    }

    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        repeat,
        ..
    }) = event
    else {
        return Decision::Pass;
    };

    if matches!(
        modified_key.as_ref(),
        keyboard::Key::Named(keyboard::key::Named::Escape)
    ) {
        one_shot(*repeat, Command::Document(DocumentCommand::CancelUnsaved))
    } else {
        Decision::Capture
    }
}

/// Routes shortcuts owned by the Note composer and passes text editing on.
fn route_note(event: &iced::Event) -> Decision {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        modifiers,
        repeat,
        ..
    }) = event
    else {
        return Decision::Pass;
    };

    if modifiers.alt() || modifiers.logo() {
        return Decision::Pass;
    }

    match modified_key.as_ref() {
        keyboard::Key::Named(keyboard::key::Named::Escape) => {
            one_shot(*repeat, Command::Comments(CommentsCommand::CloseNote))
        }
        keyboard::Key::Character("s" | "S") if modifiers.control() => {
            one_shot(*repeat, Command::Comments(CommentsCommand::SaveNote))
        }
        _ => Decision::Pass,
    }
}

/// Routes navigation owned by a focused Find field and passes query editing on.
fn route_find(event: &iced::Event) -> Decision {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        modifiers,
        repeat,
        ..
    }) = event
    else {
        return Decision::Pass;
    };

    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return Decision::Pass;
    }

    match modified_key.as_ref() {
        keyboard::Key::Named(keyboard::key::Named::Escape) => {
            one_shot(*repeat, Command::Find(FindCommand::Close))
        }
        keyboard::Key::Named(keyboard::key::Named::Enter) => {
            execute(Command::Find(if modifiers.shift() {
                FindCommand::Previous
            } else {
                FindCommand::Next
            }))
        }
        _ => Decision::Pass,
    }
}

/// Routes Vim-like preview motions and sequence parser input.
fn route_preview(pending: Pending, event: &iced::Event) -> Decision {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        modifiers,
        repeat,
        ..
    }) = event
    else {
        return Decision::Pass;
    };

    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return Decision::Pass;
    }

    if let Some(digit) = ascii_digit(&modified_key.as_ref()) {
        if *repeat {
            return Decision::Capture;
        }

        return if digit == 0 && matches!(pending, Pending::Idle) {
            execute(Command::Preview(PreviewCommand::Move(Motion::Start, 1)))
        } else {
            Decision::Dispatch(InputMessage::PushCountDigit(digit))
        };
    }

    let motion_count = pending.motion_count();
    let jump_count = pending.jump_count();

    match modified_key.as_ref() {
        keyboard::Key::Named(keyboard::key::Named::ArrowUp)
        | keyboard::Key::Character("k" | "K") => execute(Command::Preview(PreviewCommand::Move(
            Motion::Up,
            motion_count,
        ))),
        keyboard::Key::Named(keyboard::key::Named::ArrowDown)
        | keyboard::Key::Character("j" | "J") => execute(Command::Preview(PreviewCommand::Move(
            Motion::Down,
            motion_count,
        ))),
        keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
        | keyboard::Key::Character("h" | "H") => execute(Command::Preview(PreviewCommand::Move(
            Motion::Left,
            motion_count,
        ))),
        keyboard::Key::Named(keyboard::key::Named::ArrowRight)
        | keyboard::Key::Character("l" | "L") => execute(Command::Preview(PreviewCommand::Move(
            Motion::Right,
            motion_count,
        ))),
        keyboard::Key::Named(keyboard::key::Named::PageUp) => execute(Command::Preview(
            PreviewCommand::ScrollPage(Page::FullUp, motion_count),
        )),
        keyboard::Key::Named(keyboard::key::Named::PageDown) => execute(Command::Preview(
            PreviewCommand::ScrollPage(Page::FullDown, motion_count),
        )),
        keyboard::Key::Character("z") if pending.has_prefix(Prefix::Z) => one_shot(
            *repeat,
            Command::Preview(PreviewCommand::ScrollCaret(Placement::Center)),
        ),
        keyboard::Key::Character("t") if pending.has_prefix(Prefix::Z) => one_shot(
            *repeat,
            Command::Preview(PreviewCommand::ScrollCaret(Placement::Top)),
        ),
        keyboard::Key::Character("b") if pending.has_prefix(Prefix::Z) => one_shot(
            *repeat,
            Command::Preview(PreviewCommand::ScrollCaret(Placement::Bottom)),
        ),
        keyboard::Key::Character("z") => {
            if *repeat {
                Decision::Capture
            } else {
                Decision::Dispatch(InputMessage::ArmPrefix(Prefix::Z))
            }
        }
        keyboard::Key::Character("g") if pending.has_prefix(Prefix::G) => one_shot(
            *repeat,
            Command::Preview(PreviewCommand::Jump(Jump::First, jump_count)),
        ),
        keyboard::Key::Character("g") => {
            if *repeat {
                Decision::Capture
            } else {
                Decision::Dispatch(InputMessage::ArmPrefix(Prefix::G))
            }
        }
        keyboard::Key::Character("e" | "E") if pending.has_prefix(Prefix::G) => one_shot(
            *repeat,
            Command::Preview(PreviewCommand::MoveWord(
                WordMotion::PreviousEnd,
                motion_count,
            )),
        ),
        keyboard::Key::Character("e" | "E") => execute(Command::Preview(PreviewCommand::MoveWord(
            WordMotion::NextEnd,
            motion_count,
        ))),
        keyboard::Key::Character("w" | "W") => execute(Command::Preview(PreviewCommand::MoveWord(
            WordMotion::NextStart,
            motion_count,
        ))),
        keyboard::Key::Character("b" | "B") => execute(Command::Preview(PreviewCommand::MoveWord(
            WordMotion::PreviousStart,
            motion_count,
        ))),
        keyboard::Key::Character("G") => execute(Command::Preview(PreviewCommand::Jump(
            Jump::Last,
            jump_count,
        ))),
        keyboard::Key::Character("v" | "V") => {
            one_shot(*repeat, Command::Preview(PreviewCommand::ToggleVisual))
        }
        keyboard::Key::Character("c" | "C") => {
            one_shot(*repeat, Command::Comments(CommentsCommand::OpenNote))
        }
        keyboard::Key::Named(keyboard::key::Named::Enter) => {
            one_shot(*repeat, Command::Comments(CommentsCommand::EditActive))
        }
        keyboard::Key::Named(keyboard::key::Named::Escape) => {
            one_shot(*repeat, Command::Preview(PreviewCommand::Cancel))
        }
        _ => Decision::Pass,
    }
}

fn ascii_digit(key: &keyboard::Key<&str>) -> Option<u8> {
    let keyboard::Key::Character(character) = key else {
        return None;
    };
    let bytes = character.as_bytes();
    (bytes.len() == 1 && bytes[0].is_ascii_digit()).then(|| bytes[0] - b'0')
}

/// Routes application-wide shortcuts after the active interaction has had
/// first refusal.
fn route_global(context: GlobalContext, event: &iced::Event) -> Decision {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        modifiers,
        repeat,
        ..
    }) = event
    else {
        return Decision::Pass;
    };

    if !modifiers.control() || modifiers.alt() || modifiers.logo() {
        return Decision::Pass;
    }

    let command = match modified_key.as_ref() {
        keyboard::Key::Character("o" | "O") => Command::Document(DocumentCommand::Open),
        keyboard::Key::Character("p" | "P")
            if matches!(
                context,
                GlobalContext::EditableWrite
                    | GlobalContext::EditableWriteFind
                    | GlobalContext::EditablePreview(_)
                    | GlobalContext::EditablePreviewFind
            ) =>
        {
            Command::Preview(PreviewCommand::Toggle)
        }
        keyboard::Key::Character("b" | "B") => Command::Comments(CommentsCommand::ToggleSidebar),
        keyboard::Key::Character("s" | "S") => Command::Document(DocumentCommand::Save),
        keyboard::Key::Character("f" | "F") => Command::Find(FindCommand::Open),
        keyboard::Key::Character("g" | "G")
            if matches!(
                context,
                GlobalContext::EditableWriteFind
                    | GlobalContext::EditablePreviewFind
                    | GlobalContext::PreviewOnlyFind
            ) =>
        {
            Command::Find(FindCommand::Next)
        }
        keyboard::Key::Character("n" | "N")
            if matches!(
                context,
                GlobalContext::EditablePreview(_)
                    | GlobalContext::EditablePreviewFind
                    | GlobalContext::PreviewOnly(_)
                    | GlobalContext::PreviewOnlyFind
            ) =>
        {
            Command::Comments(CommentsCommand::Next)
        }
        keyboard::Key::Character("d" | "D")
            if matches!(
                context,
                GlobalContext::EditablePreview(_)
                    | GlobalContext::EditablePreviewFind
                    | GlobalContext::PreviewOnly(_)
                    | GlobalContext::PreviewOnlyFind
            ) =>
        {
            Command::Preview(PreviewCommand::ScrollPage(
                Page::HalfDown,
                global_motion_count(context),
            ))
        }
        keyboard::Key::Character("u" | "U")
            if matches!(
                context,
                GlobalContext::EditablePreview(_)
                    | GlobalContext::EditablePreviewFind
                    | GlobalContext::PreviewOnly(_)
                    | GlobalContext::PreviewOnlyFind
            ) =>
        {
            Command::Preview(PreviewCommand::ScrollPage(
                Page::HalfUp,
                global_motion_count(context),
            ))
        }
        keyboard::Key::Named(keyboard::key::Named::Enter) => {
            Command::Comments(if modifiers.shift() {
                CommentsCommand::Publish
            } else {
                CommentsCommand::AddGlobal
            })
        }
        _ => return Decision::Pass,
    };

    one_shot(*repeat, command)
}

fn global_motion_count(context: GlobalContext) -> usize {
    match context {
        GlobalContext::EditablePreview(pending) | GlobalContext::PreviewOnly(pending) => {
            pending.motion_count()
        }
        GlobalContext::EditablePreviewFind | GlobalContext::PreviewOnlyFind => 1,
        GlobalContext::EditableWrite | GlobalContext::EditableWriteFind => {
            unreachable!("write contexts do not route preview scrolling")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::UnsavedAction;
    use iced::keyboard::{key, Location, Modifiers};

    fn pressed(key: keyboard::Key, modifiers: Modifiers, repeat: bool) -> iced::Event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat,
        })
    }

    fn character(value: &str) -> iced::Event {
        character_with(value, Modifiers::default(), false)
    }

    fn character_with(value: &str, modifiers: Modifiers, repeat: bool) -> iced::Event {
        pressed(keyboard::Key::Character(value.into()), modifiers, repeat)
    }

    fn named(value: key::Named, modifiers: Modifiers, repeat: bool) -> iced::Event {
        pressed(keyboard::Key::Named(value), modifiers, repeat)
    }

    fn escape() -> iced::Event {
        named(key::Named::Escape, Modifiers::default(), false)
    }

    fn editable_preview(visual: bool) -> InteractionState {
        let mut state = InteractionState::editable();
        assert!(state.toggle_preview());
        if visual {
            state.toggle_visual().unwrap();
        }
        state
    }

    fn preview_only(visual: bool) -> InteractionState {
        let mut state = InteractionState::preview_only();
        if visual {
            state.toggle_visual().unwrap();
        }
        state
    }

    fn note(mut state: InteractionState) -> InteractionState {
        state.open_note().unwrap();
        state
    }

    fn find(mut state: InteractionState) -> InteractionState {
        state.open_find();
        state
    }

    fn dispatch(command: Command) -> Decision {
        Decision::Dispatch(InputMessage::Execute(command))
    }

    #[test]
    fn exhaustive_workspace_variants_route_to_their_focused_owner() {
        let mut cases = vec![
            (
                "editable write editor".to_owned(),
                InteractionState::editable(),
                character("x"),
                Decision::Pass,
            ),
            (
                "editable write find".to_owned(),
                find(InteractionState::editable()),
                named(key::Named::Enter, Modifiers::default(), false),
                dispatch(Command::Find(FindCommand::Next)),
            ),
        ];

        for (preview_only_session, label) in [(false, "editable"), (true, "preview-only")] {
            for visual in [false, true] {
                let mode = if visual { "visual" } else { "view" };
                let canvas = if preview_only_session {
                    preview_only(visual)
                } else {
                    editable_preview(visual)
                };
                cases.push((
                    format!("{label} canvas {mode}"),
                    canvas,
                    character("j"),
                    dispatch(Command::Preview(PreviewCommand::Move(Motion::Down, 1))),
                ));

                let base = if preview_only_session {
                    preview_only(visual)
                } else {
                    editable_preview(visual)
                };
                cases.push((
                    format!("{label} note {mode}"),
                    note(base),
                    character("x"),
                    Decision::Pass,
                ));

                let base = if preview_only_session {
                    preview_only(visual)
                } else {
                    editable_preview(visual)
                };
                cases.push((
                    format!("{label} find over canvas {mode}"),
                    find(base),
                    named(key::Named::Enter, Modifiers::default(), false),
                    dispatch(Command::Find(FindCommand::Next)),
                ));

                let base = if preview_only_session {
                    preview_only(visual)
                } else {
                    editable_preview(visual)
                };
                cases.push((
                    format!("{label} find over note {mode}"),
                    find(note(base)),
                    named(key::Named::Enter, Modifiers::SHIFT, false),
                    dispatch(Command::Find(FindCommand::Previous)),
                ));
            }
        }

        for (name, state, event, expected) in cases {
            assert_eq!(route(&state, &event), expected, "{name}");
        }
    }

    #[test]
    fn modal_root_variants_enforce_top_down_ownership() {
        let mut state = find(note(editable_preview(false)));
        state.open_unsaved(UnsavedAction::OpenFile);
        state.open_help();

        assert_eq!(
            route(&state, &escape()),
            dispatch(Command::Help(HelpCommand::Close))
        );
        assert_eq!(route(&state, &character("x")), Decision::Pass);

        let _ = state.close_help();
        assert_eq!(
            route(&state, &escape()),
            dispatch(Command::Document(DocumentCommand::CancelUnsaved))
        );
        assert_eq!(route(&state, &character("x")), Decision::Capture);
        assert_eq!(
            route(
                &state,
                &iced::Event::InputMethod(iced::advanced::input_method::Event::Closed)
            ),
            Decision::Capture
        );

        let help_chord = character_with("?", Modifiers::CTRL, false);
        assert_eq!(
            route(&state, &help_chord),
            dispatch(Command::Help(HelpCommand::Open))
        );

        let _ = state.resolve_unsaved();
        assert_eq!(
            route(&state, &escape()),
            dispatch(Command::Find(FindCommand::Close))
        );
        let _ = state.close_find();
        assert_eq!(
            route(&state, &escape()),
            dispatch(Command::Comments(CommentsCommand::CloseNote))
        );
    }

    #[test]
    fn help_routes_open_close_capture_and_text_pass_through() {
        let chord = character_with("/", Modifiers::CTRL, false);
        let write = InteractionState::editable();
        assert_eq!(
            route(&write, &chord),
            dispatch(Command::Help(HelpCommand::Open))
        );

        let mut help = write;
        help.open_help();
        let cases = [
            ("typing", character("x"), Decision::Pass),
            (
                "tab containment",
                named(key::Named::Tab, Modifiers::default(), false),
                Decision::Capture,
            ),
            (
                "escape",
                escape(),
                dispatch(Command::Help(HelpCommand::Close)),
            ),
            (
                "toggle chord",
                chord,
                dispatch(Command::Help(HelpCommand::Close)),
            ),
        ];
        for (name, event, expected) in cases {
            assert_eq!(route(&help, &event), expected, "{name}");
        }
        assert_eq!(
            route(
                &help,
                &iced::Event::InputMethod(iced::advanced::input_method::Event::Closed)
            ),
            Decision::Pass
        );
    }

    #[test]
    fn note_find_help_and_write_fields_receive_unbound_editing_input() {
        let write = InteractionState::editable();
        let note = note(editable_preview(false));
        let find = find(editable_preview(false));
        let mut help = InteractionState::editable();
        help.open_help();

        let cases = [
            ("write typing", write, character("x")),
            (
                "write select all",
                write,
                character_with("a", Modifiers::CTRL, false),
            ),
            ("note typing", note, character("x")),
            (
                "note newline",
                note,
                named(key::Named::Enter, Modifiers::default(), false),
            ),
            (
                "note select all",
                note,
                character_with("a", Modifiers::CTRL, false),
            ),
            ("find typing", find, character("j")),
            (
                "find select all",
                find,
                character_with("a", Modifiers::CTRL, false),
            ),
            ("help typing", help, character("x")),
        ];

        for (name, state, event) in cases {
            assert_eq!(route(&state, &event), Decision::Pass, "{name}");
        }

        assert_eq!(
            route(&note, &character_with("s", Modifiers::CTRL, false)),
            dispatch(Command::Comments(CommentsCommand::SaveNote))
        );
    }

    #[test]
    fn preview_sequences_emit_internal_parser_messages_only() {
        let idle = editable_preview(false);
        let mut counted = idle;
        counted.push_count_digit(1);
        let mut g = idle;
        g.arm_prefix(Prefix::G);
        let mut counted_g = counted;
        counted_g.arm_prefix(Prefix::G);
        let mut z = idle;
        z.arm_prefix(Prefix::Z);

        let cases = [
            (
                "lone zero is semantic",
                idle,
                character("0"),
                dispatch(Command::Preview(PreviewCommand::Move(Motion::Start, 1))),
            ),
            (
                "digit starts count",
                idle,
                character("3"),
                Decision::Dispatch(InputMessage::PushCountDigit(3)),
            ),
            (
                "zero extends count",
                counted,
                character("0"),
                Decision::Dispatch(InputMessage::PushCountDigit(0)),
            ),
            (
                "g arms internally",
                idle,
                character("g"),
                Decision::Dispatch(InputMessage::ArmPrefix(Prefix::G)),
            ),
            (
                "gg completes semantically",
                g,
                character("g"),
                dispatch(Command::Preview(PreviewCommand::Jump(Jump::First, 0))),
            ),
            (
                "count survives g",
                counted_g,
                character("g"),
                dispatch(Command::Preview(PreviewCommand::Jump(Jump::First, 1))),
            ),
            (
                "z arms internally",
                idle,
                character("z"),
                Decision::Dispatch(InputMessage::ArmPrefix(Prefix::Z)),
            ),
            (
                "zt completes semantically",
                z,
                character("t"),
                dispatch(Command::Preview(PreviewCommand::ScrollCaret(
                    Placement::Top,
                ))),
            ),
            (
                "digit after prefix is parser input",
                z,
                character("0"),
                Decision::Dispatch(InputMessage::PushCountDigit(0)),
            ),
        ];

        for (name, state, event, expected) in cases {
            let before = state;
            assert_eq!(route(&state, &event), expected, "{name}");
            assert_eq!(state, before, "{name}: routing must be pure");
        }
    }

    #[test]
    fn repeat_policy_distinguishes_motions_navigation_and_one_shots() {
        let preview = editable_preview(false);
        let find = find(preview);
        let cases = [
            (
                "held motion",
                preview,
                character_with("j", Modifiers::default(), true),
                dispatch(Command::Preview(PreviewCommand::Move(Motion::Down, 1))),
            ),
            (
                "held full page",
                preview,
                named(key::Named::PageDown, Modifiers::default(), true),
                dispatch(Command::Preview(PreviewCommand::ScrollPage(
                    Page::FullDown,
                    1,
                ))),
            ),
            (
                "held note open",
                preview,
                character_with("c", Modifiers::default(), true),
                Decision::Capture,
            ),
            (
                "held prefix",
                preview,
                character_with("g", Modifiers::default(), true),
                Decision::Capture,
            ),
            (
                "held digit",
                preview,
                character_with("3", Modifiers::default(), true),
                Decision::Capture,
            ),
            (
                "held preview enter",
                preview,
                named(key::Named::Enter, Modifiers::default(), true),
                Decision::Capture,
            ),
            (
                "held find enter",
                find,
                named(key::Named::Enter, Modifiers::default(), true),
                dispatch(Command::Find(FindCommand::Next)),
            ),
            (
                "held global toggle",
                preview,
                character_with("b", Modifiers::CTRL, true),
                Decision::Capture,
            ),
        ];

        for (name, state, event, expected) in cases {
            assert_eq!(route(&state, &event), expected, "{name}");
        }
    }

    #[test]
    fn global_shortcuts_respect_structural_workspace_capabilities() {
        let write = InteractionState::editable();
        let preview = editable_preview(false);
        let preview_only = InteractionState::preview_only();
        let find = find(preview);
        let mut counted = preview;
        counted.push_count_digit(3);

        let cases = [
            (
                "open from write",
                write,
                character_with("o", Modifiers::CTRL, false),
                dispatch(Command::Document(DocumentCommand::Open)),
            ),
            (
                "save from write",
                write,
                character_with("s", Modifiers::CTRL, false),
                dispatch(Command::Document(DocumentCommand::Save)),
            ),
            (
                "toggle sidebar from preview",
                preview,
                character_with("b", Modifiers::CTRL, false),
                dispatch(Command::Comments(CommentsCommand::ToggleSidebar)),
            ),
            (
                "open find from write",
                write,
                character_with("f", Modifiers::CTRL, false),
                dispatch(Command::Find(FindCommand::Open)),
            ),
            (
                "toggle from write",
                write,
                character_with("p", Modifiers::CTRL, false),
                dispatch(Command::Preview(PreviewCommand::Toggle)),
            ),
            (
                "toggle from preview",
                preview,
                character_with("p", Modifiers::CTRL, false),
                dispatch(Command::Preview(PreviewCommand::Toggle)),
            ),
            (
                "preview-only cannot toggle",
                preview_only,
                character_with("p", Modifiers::CTRL, false),
                Decision::Pass,
            ),
            (
                "next comment in preview",
                preview,
                character_with("n", Modifiers::CTRL, false),
                dispatch(Command::Comments(CommentsCommand::Next)),
            ),
            (
                "no next comment in write",
                write,
                character_with("n", Modifiers::CTRL, false),
                Decision::Pass,
            ),
            (
                "counted half page down",
                counted,
                character_with("d", Modifiers::CTRL, false),
                dispatch(Command::Preview(PreviewCommand::ScrollPage(
                    Page::HalfDown,
                    3,
                ))),
            ),
            (
                "counted half page up",
                counted,
                character_with("u", Modifiers::CTRL, false),
                dispatch(Command::Preview(PreviewCommand::ScrollPage(
                    Page::HalfUp,
                    3,
                ))),
            ),
            (
                "next find match only while find owns input",
                find,
                character_with("g", Modifiers::CTRL, false),
                dispatch(Command::Find(FindCommand::Next)),
            ),
            (
                "ctrl enter adds global",
                write,
                named(key::Named::Enter, Modifiers::CTRL, false),
                dispatch(Command::Comments(CommentsCommand::AddGlobal)),
            ),
            (
                "ctrl shift enter publishes",
                preview,
                named(key::Named::Enter, Modifiers::CTRL | Modifiers::SHIFT, false),
                dispatch(Command::Comments(CommentsCommand::Publish)),
            ),
        ];

        for (name, state, event, expected) in cases {
            assert_eq!(route(&state, &event), expected, "{name}");
        }
    }
}
