//! Temporary keymap façade over the legal interaction-state hierarchy.
//!
//! [`Keymap::handle`] still resolves keys while callers migrate. All stored
//! UI state and preview sequences live in [`InteractionState`].

use iced::keyboard;

use super::command::{
    Command, CommentsCommand, DocumentCommand, FindCommand, HelpCommand, PreviewCommand,
};
use super::pending::Prefix;
use super::state::{InteractionState, ModeBadge, Overlay, Surface};
use crate::document::UnsavedAction;
use crate::help;
use crate::preview::{Jump, Motion, Page, Placement, WordMotion};

/// The input mode façade used while binding callers migrate.
///
/// `Clone` + `Hash` so the keyboard subscription can carry a snapshot and
/// re-listen when the mode changes; key handling reads the snapshot
/// ([`Keymap::handle`]) while transitions happen through messages
/// ([`Keymap::note`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keymap {
    interaction: InteractionState,
}

/// An input-local state transition observed by the keymap.
///
/// The application translates its messages to this small vocabulary. This
/// keeps mode and prefix bookkeeping independent of the root message enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Activity unrelated to input state clears pending prefixes and counts.
    Activity,
    DocumentLoaded,
    PreviewToggled,
    VisualToggled,
    PreviewCancelled,
    GArmed,
    ZArmed,
    CountPressed(u32),
    NoteOpened,
    NoteClosed,
    FindOpened,
    FindClosed,
    HelpOpened,
    HelpClosed,
    /// Activity within Help is transparent to underlying prefixes/counts.
    HelpActivity,
    UnsavedOpened(UnsavedAction),
    UnsavedClosed,
}

impl Keymap {
    /// A keymap starting in write mode — or in view mode when the app runs
    /// preview-only.
    pub fn new(preview_only: bool) -> Self {
        Self {
            interaction: if preview_only {
                InteractionState::preview_only()
            } else {
                InteractionState::editable()
            },
        }
    }

    /// The current mode, popups included — the badge reads this.
    pub fn mode(&self) -> ModeBadge {
        self.interaction.view().badge()
    }

    /// Whether the preview is showing — in every mode but write. The note
    /// popup only ever floats above the preview, so it counts as preview.
    pub fn preview(&self) -> bool {
        matches!(self.interaction.view().surface(), Surface::Preview)
    }

    /// Whether this session may switch between Write and Preview.
    pub fn can_toggle_preview(&self) -> bool {
        self.interaction.view().can_toggle_preview()
    }

    /// Whether the note popup is open.
    pub fn note_open(&self) -> bool {
        self.interaction.view().contains(Overlay::Note)
    }

    /// Whether the find popup is open.
    pub fn find_open(&self) -> bool {
        self.interaction.view().contains(Overlay::Find)
    }

    /// Whether the help overlay is open. The mode itself remains unchanged
    /// while help is visible.
    pub fn help_open(&self) -> bool {
        matches!(self.interaction.view().overlay(), Some(Overlay::Help))
    }

    /// Whether the unsaved-changes dialog is open. The mode itself remains
    /// unchanged beneath it.
    pub fn unsaved_open(&self) -> bool {
        self.interaction.view().has_unsaved()
    }

    /// The pending motion count, for the mode badge's `3×` hint. `0` when
    /// no count is pending.
    pub fn pending_count(&self) -> u32 {
        self.interaction
            .view()
            .pending_count()
            .map_or(0, std::num::NonZeroU32::get)
    }

    /// The count a motion repeats: the typed digits, or once.
    fn count(&self) -> usize {
        self.interaction.motion_count()
    }

    /// The count a jump carries: the typed digits, or `0` — none — so a
    /// plain `gg`/`G` keeps its first/last meaning.
    fn jump_count(&self) -> usize {
        self.interaction.jump_count()
    }

    /// Whether visual mode is active — motions extend the selection.
    pub fn visual(&self) -> bool {
        self.interaction.view().visual()
    }

    /// Turns a key event into the semantic command it means in the current
    /// mode, if any.
    ///
    /// This only reads the mode; the transitions themselves happen in
    /// [`Keymap::note`] once the produced message is dispatched, so key
    /// handling and message handling never see different states.
    pub fn handle(&self, event: keyboard::Event) -> Option<Command> {
        // Help owns its chord and modal keyboard decisions; the keymap
        // only supplies the visibility state and translates the decision
        // into an application message.
        match help::keyboard_action(self.help_open(), &event) {
            help::EventAction::Open => return Some(Command::Help(HelpCommand::Open)),
            help::EventAction::Close => return Some(Command::Help(HelpCommand::Close)),
            help::EventAction::Capture => return None,
            help::EventAction::Pass if self.help_open() => return None,
            help::EventAction::Pass => {}
        }

        let keyboard::Event::KeyPressed {
            modified_key,
            modifiers,
            repeat,
            ..
        } = event
        else {
            return None;
        };

        // The unsaved-changes dialog is a modal too: Escape cancels it, the
        // three buttons answer it, and every other key is swallowed so the
        // editing surface beneath stays untouched.
        if self.unsaved_open() {
            return match modified_key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Command::Document(DocumentCommand::CancelUnsaved))
                }
                _ => None,
            };
        }

        // The note popup swallows plain keys for its text area; only Escape
        // closes it and Ctrl+S saves the comment.
        if self.note_open() && !modifiers.alt() && !modifiers.logo() {
            return match modified_key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Command::Comments(CommentsCommand::CloseNote))
                }
                keyboard::Key::Character("s" | "S") if modifiers.control() && !repeat => {
                    Some(Command::Comments(CommentsCommand::SaveNote))
                }
                _ => None,
            };
        }

        // The find popup swallows plain keys for its query field; only
        // Escape closes it and Enter steps through the matches.
        // Ctrl combinations still reach the global shortcuts below.
        if self.find_open() && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            return match modified_key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Command::Find(FindCommand::Close))
                }
                // Enter steps to the next match, Shift+Enter back to the
                // previous one — welcome to repeat, like holding `n` in
                // vim.
                keyboard::Key::Named(keyboard::key::Named::Enter) => Some(if modifiers.shift() {
                    Command::Find(FindCommand::Previous)
                } else {
                    Command::Find(FindCommand::Next)
                }),
                _ => None,
            };
        }

        if self.preview() && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            // Auto-repeat is welcome for motions (holding `j`/`k`/`h`/`l`,
            // `w`, `b`, `e`, or `G` keeps moving), but one-shot actions
            // must not repeat.
            return match modified_key.as_ref() {
                // Digits accumulate a motion count like vim's `3j`; a lone
                // `0` is the element-start motion instead.
                keyboard::Key::Character(c)
                    if !repeat && c.chars().all(|character| character.is_ascii_digit()) =>
                {
                    if c == "0" && !self.interaction.has_count() {
                        Some(Command::Preview(PreviewCommand::Move(Motion::Start, 1)))
                    } else {
                        Some(Command::Preview(PreviewCommand::Count(
                            c.parse().unwrap_or(0),
                        )))
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowUp)
                | keyboard::Key::Character("k" | "K") => Some(Command::Preview(
                    PreviewCommand::Move(Motion::Up, self.count()),
                )),
                keyboard::Key::Named(keyboard::key::Named::ArrowDown)
                | keyboard::Key::Character("j" | "J") => Some(Command::Preview(
                    PreviewCommand::Move(Motion::Down, self.count()),
                )),
                keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
                | keyboard::Key::Character("h" | "H") => Some(Command::Preview(
                    PreviewCommand::Move(Motion::Left, self.count()),
                )),
                keyboard::Key::Named(keyboard::key::Named::ArrowRight)
                | keyboard::Key::Character("l" | "L") => Some(Command::Preview(
                    PreviewCommand::Move(Motion::Right, self.count()),
                )),
                // Full pages: PageUp/PageDown scroll the preview like
                // vim's `Ctrl+F`/`Ctrl+B`, with the count repeating pages.
                keyboard::Key::Named(keyboard::key::Named::PageUp) => Some(Command::Preview(
                    PreviewCommand::ScrollPage(Page::FullUp, self.count()),
                )),
                keyboard::Key::Named(keyboard::key::Named::PageDown) => Some(Command::Preview(
                    PreviewCommand::ScrollPage(Page::FullDown, self.count()),
                )),
                // `zz`/`zt`/`zb` — scroll the caret to the middle, top, or
                // bottom of the viewport. The second key is always fresh.
                keyboard::Key::Character("z")
                    if self.interaction.has_prefix(Prefix::Z) && !repeat =>
                {
                    Some(Command::Preview(PreviewCommand::ScrollCaret(
                        Placement::Center,
                    )))
                }
                keyboard::Key::Character("t")
                    if self.interaction.has_prefix(Prefix::Z) && !repeat =>
                {
                    Some(Command::Preview(PreviewCommand::ScrollCaret(
                        Placement::Top,
                    )))
                }
                keyboard::Key::Character("b")
                    if self.interaction.has_prefix(Prefix::Z) && !repeat =>
                {
                    Some(Command::Preview(PreviewCommand::ScrollCaret(
                        Placement::Bottom,
                    )))
                }
                // The first `z` of a `zz`/`zt`/`zb` sequence.
                keyboard::Key::Character("z") if !repeat => {
                    Some(Command::Preview(PreviewCommand::ArmZ))
                }
                // `gg` — the second `g` of the sequence is always a fresh
                // press.
                keyboard::Key::Character("g")
                    if self.interaction.has_prefix(Prefix::G) && !repeat =>
                {
                    Some(Command::Preview(PreviewCommand::Jump(
                        Jump::First,
                        self.jump_count(),
                    )))
                }
                // The first `g` of a `gg`/`ge` sequence.
                keyboard::Key::Character("g") if !repeat => {
                    Some(Command::Preview(PreviewCommand::ArmG))
                }
                // `ge` — the `e` of the sequence is always a fresh press.
                keyboard::Key::Character("e" | "E")
                    if self.interaction.has_prefix(Prefix::G) && !repeat =>
                {
                    Some(Command::Preview(PreviewCommand::MoveWord(
                        WordMotion::PreviousEnd,
                        self.count(),
                    )))
                }
                keyboard::Key::Character("e" | "E") => Some(Command::Preview(
                    PreviewCommand::MoveWord(WordMotion::NextEnd, self.count()),
                )),
                keyboard::Key::Character("w" | "W") => Some(Command::Preview(
                    PreviewCommand::MoveWord(WordMotion::NextStart, self.count()),
                )),
                keyboard::Key::Character("b" | "B") => Some(Command::Preview(
                    PreviewCommand::MoveWord(WordMotion::PreviousStart, self.count()),
                )),
                keyboard::Key::Character("G") => Some(Command::Preview(PreviewCommand::Jump(
                    Jump::Last,
                    self.jump_count(),
                ))),
                keyboard::Key::Character("v" | "V") if !repeat => {
                    Some(Command::Preview(PreviewCommand::ToggleVisual))
                }
                keyboard::Key::Character("c" | "C") if !repeat => {
                    Some(Command::Comments(CommentsCommand::OpenNote))
                }
                // Enter opens the active comment for editing, like the
                // sidebar's edit affordance.
                keyboard::Key::Named(keyboard::key::Named::Enter) if !repeat => {
                    Some(Command::Comments(CommentsCommand::EditActive))
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Command::Preview(PreviewCommand::Cancel))
                }
                _ => None,
            };
        }

        if modifiers.control() && !repeat {
            match modified_key.as_ref() {
                keyboard::Key::Character("o" | "O") => {
                    Some(Command::Document(DocumentCommand::Open))
                }
                keyboard::Key::Character("p" | "P") if self.can_toggle_preview() => {
                    Some(Command::Preview(PreviewCommand::Toggle))
                }
                // Show or hide the comments sidebar.
                keyboard::Key::Character("b" | "B") => {
                    Some(Command::Comments(CommentsCommand::ToggleSidebar))
                }
                // Save the document, asking for a path if it was never
                // saved. With the note popup open, Ctrl+S saves the note
                // instead — the popup guard above took that path already.
                keyboard::Key::Character("s" | "S") => {
                    Some(Command::Document(DocumentCommand::Save))
                }
                // Find in the document, in any mode.
                keyboard::Key::Character("f" | "F") => Some(Command::Find(FindCommand::Open)),
                // Step to the next match while the find popup is open.
                keyboard::Key::Character("g" | "G") if self.find_open() => {
                    Some(Command::Find(FindCommand::Next))
                }
                // Browse the saved comments in the preview.
                keyboard::Key::Character("n" | "N") if self.preview() => {
                    Some(Command::Comments(CommentsCommand::Next))
                }
                // Half pages: Ctrl+D/Ctrl+U scroll the preview like vim,
                // with the count repeating half pages.
                keyboard::Key::Character("d" | "D") if self.preview() => Some(Command::Preview(
                    PreviewCommand::ScrollPage(Page::HalfDown, self.count()),
                )),
                keyboard::Key::Character("u" | "U") if self.preview() => Some(Command::Preview(
                    PreviewCommand::ScrollPage(Page::HalfUp, self.count()),
                )),
                // The comments sidebar's buttons: Ctrl+Enter triggers Add,
                // Ctrl+Shift+Enter triggers Publish.
                keyboard::Key::Named(keyboard::key::Named::Enter) => Some(if modifiers.shift() {
                    Command::Comments(CommentsCommand::Publish)
                } else {
                    Command::Comments(CommentsCommand::AddGlobal)
                }),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Keeps the mode stack in sync with an input transition, so changes
    /// triggered without keys (button and backdrop clicks) land exactly
    /// like the key-driven ones.
    pub fn note(&mut self, transition: Transition) {
        match transition {
            Transition::Activity => self.interaction.activity(),
            Transition::DocumentLoaded => self.interaction.document_loaded(),
            Transition::PreviewToggled => self.interaction.toggle_preview(),
            Transition::VisualToggled => {
                let _ = self.interaction.toggle_visual();
            }
            Transition::PreviewCancelled => self.interaction.cancel_preview(),
            Transition::GArmed => self.interaction.arm_prefix(Prefix::G),
            Transition::ZArmed => self.interaction.arm_prefix(Prefix::Z),
            Transition::CountPressed(digit) => self.interaction.push_count_digit(digit),
            Transition::NoteOpened => {
                let _ = self.interaction.open_note();
            }
            Transition::NoteClosed => self.interaction.close_note(),
            Transition::FindOpened => self.interaction.open_find(),
            Transition::FindClosed => self.interaction.close_find(),
            Transition::HelpOpened => self.interaction.open_help(),
            Transition::HelpClosed => self.interaction.close_help(),
            Transition::HelpActivity => {}
            Transition::UnsavedOpened(action) => self.interaction.open_unsaved(action),
            Transition::UnsavedClosed => self.interaction.close_unsaved(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::command::{
        Command, CommentsCommand, DocumentCommand, FindCommand, HelpCommand, PreviewCommand,
    };
    use super::{Keymap, Transition};
    use crate::document::UnsavedAction;
    use crate::input::Mode;
    use crate::preview::{Jump, Motion, Page, Placement, WordMotion};
    use iced::keyboard::{self, key, Modifiers};

    fn key_press(character: &str, repeat: bool) -> keyboard::Event {
        let key = keyboard::Key::Character(character.into());

        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::default(),
            text: None,
            repeat,
        }
    }

    fn key_press_with(character: &str, modifiers: Modifiers, repeat: bool) -> keyboard::Event {
        let key = keyboard::Key::Character(character.into());

        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat,
        }
    }

    fn escape() -> keyboard::Event {
        let key = keyboard::Key::Named(keyboard::key::Named::Escape);

        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::default(),
            text: None,
            repeat: false,
        }
    }

    fn named(key: keyboard::key::Named, modifiers: Modifiers) -> keyboard::Event {
        let key = keyboard::Key::Named(key);

        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        }
    }

    /// A keymap in view mode, as if `Ctrl+P` had switched to the preview.
    fn viewing() -> Keymap {
        let mut keymap = Keymap::new(false);
        keymap.note(Transition::PreviewToggled);
        keymap
    }

    /// The badge and key handling read one representation: write when
    /// editing, view when previewing, visual while a selection is
    /// anchored, note while the popup is open.
    #[test]
    fn one_mode_for_badge_and_keys() {
        let mut keymap = Keymap::new(false);
        assert_eq!(keymap.mode(), Mode::Write);
        assert!(!keymap.preview());

        keymap.note(Transition::PreviewToggled);
        assert_eq!(keymap.mode(), Mode::View);
        assert!(keymap.preview());

        keymap.note(Transition::VisualToggled);
        assert_eq!(keymap.mode(), Mode::Visual);

        // The note popup floats above visual mode and closing it returns
        // there.
        keymap.note(Transition::NoteOpened);
        assert_eq!(keymap.mode(), Mode::Note);
        keymap.note(Transition::NoteClosed);
        assert_eq!(keymap.mode(), Mode::Visual);

        keymap.note(Transition::VisualToggled);
        assert_eq!(keymap.mode(), Mode::View);

        keymap.note(Transition::PreviewToggled);
        assert_eq!(keymap.mode(), Mode::Write);
    }

    #[test]
    fn closing_find_above_note_restores_note() {
        let mut keymap = viewing();
        keymap.note(Transition::NoteOpened);
        keymap.note(Transition::FindOpened);

        assert!(keymap.note_open());
        assert!(keymap.find_open());

        keymap.note(Transition::FindClosed);

        assert!(keymap.note_open());
        assert!(!keymap.find_open());
        assert_eq!(keymap.mode(), Mode::Note);
    }

    /// Find is rendered above Note, so its badge should have the same
    /// priority. This deliberately captures the priority correction that
    /// the legacy boolean ordering does not yet satisfy.
    #[test]
    fn find_above_note_uses_find_badge() {
        let mut keymap = viewing();
        keymap.note(Transition::NoteOpened);
        keymap.note(Transition::FindOpened);

        assert_eq!(keymap.mode(), Mode::Find);
    }

    #[test]
    fn preview_toggle_preserves_find_and_drops_note_beneath_it() {
        let mut keymap = viewing();
        keymap.note(Transition::NoteOpened);
        keymap.note(Transition::FindOpened);

        keymap.note(Transition::PreviewToggled);
        assert!(!keymap.preview());
        assert!(keymap.find_open());
        assert!(!keymap.note_open());
        assert_eq!(keymap.mode(), Mode::Find);

        keymap.note(Transition::PreviewToggled);
        assert!(keymap.preview());
        assert!(keymap.find_open());
        assert_eq!(keymap.mode(), Mode::Find);
    }

    /// Holding a motion key auto-repeats the motion, like holding `j` or
    /// `k` in vim; one-shot actions and the `g`-prefix never repeat.
    #[test]
    fn held_motion_keys_repeat() {
        let keymap = viewing();
        let pressed = |character: &str, repeat: bool| keymap.handle(key_press(character, repeat));

        // Plain presses and repeats both move.
        assert!(matches!(
            pressed("j", false),
            Some(Command::Preview(PreviewCommand::Move(Motion::Down, 1)))
        ));
        assert!(matches!(
            pressed("j", true),
            Some(Command::Preview(PreviewCommand::Move(Motion::Down, 1)))
        ));
        assert!(matches!(
            pressed("k", true),
            Some(Command::Preview(PreviewCommand::Move(Motion::Up, 1)))
        ));
        assert!(matches!(
            pressed("h", true),
            Some(Command::Preview(PreviewCommand::Move(Motion::Left, 1)))
        ));
        assert!(matches!(
            pressed("l", true),
            Some(Command::Preview(PreviewCommand::Move(Motion::Right, 1)))
        ));
        assert!(matches!(
            pressed("w", true),
            Some(Command::Preview(PreviewCommand::MoveWord(
                WordMotion::NextStart,
                1
            )))
        ));
        assert!(matches!(
            pressed("b", true),
            Some(Command::Preview(PreviewCommand::MoveWord(
                WordMotion::PreviousStart,
                1
            )))
        ));
        assert!(matches!(
            pressed("e", true),
            Some(Command::Preview(PreviewCommand::MoveWord(
                WordMotion::NextEnd,
                1
            )))
        ));

        // One-shot actions ignore repeats.
        assert!(matches!(
            pressed("c", false),
            Some(Command::Comments(CommentsCommand::OpenNote))
        ));
        assert!(pressed("c", true).is_none());

        // The `g` prefix never arms or fires on repeat — `gg` and `ge` need
        // two fresh presses.
        assert!(matches!(
            pressed("g", false),
            Some(Command::Preview(PreviewCommand::ArmG))
        ));
        assert!(pressed("g", true).is_none());

        // `v` toggles visual mode on fresh presses only.
        assert!(matches!(
            pressed("v", false),
            Some(Command::Preview(PreviewCommand::ToggleVisual))
        ));
        assert!(pressed("v", true).is_none());
    }

    /// `gg` jumps to the first element: the first `g` arms the prefix
    /// through its message, the second fresh `g` fires the jump, and any
    /// other key ends the sequence.
    #[test]
    fn gg_arms_then_fires() {
        let mut keymap = viewing();

        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::ArmG))
        ));
        keymap.note(Transition::GArmed);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::Jump(Jump::First, 0)))
        ));

        // The sequence fired; a third `g` starts over.
        keymap.note(Transition::Activity);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::ArmG))
        ));

        // Any other key ends the pending sequence.
        keymap.note(Transition::GArmed);
        assert!(matches!(
            keymap.handle(key_press("j", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Down, 1)))
        ));
        keymap.note(Transition::Activity);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::ArmG))
        ));

        // `ge` moves to the end of the previous word.
        keymap.note(Transition::GArmed);
        assert!(matches!(
            keymap.handle(key_press("e", false)),
            Some(Command::Preview(PreviewCommand::MoveWord(
                WordMotion::PreviousEnd,
                1
            )))
        ));
    }

    /// While the note popup is open, plain keys go to its text area — only
    /// Escape closes it and Ctrl+S saves the comment.
    #[test]
    fn note_popup_swallows_keys() {
        let mut keymap = viewing();
        keymap.note(Transition::NoteOpened);

        assert!(matches!(
            keymap.handle(escape()),
            Some(Command::Comments(CommentsCommand::CloseNote))
        ));

        // Motions, `c`, and typing characters produce nothing while the
        // popup is open.
        for key in ["j", "k", "h", "l", "w", "b", "e", "g", "c", "G", "x"] {
            assert!(
                keymap.handle(key_press(key, false)).is_none(),
                "'{key}' should be swallowed by the note popup"
            );
        }

        // Ctrl+S saves the note while the popup is open; other Ctrl combos
        // (even Ctrl+O) stay swallowed.
        assert!(matches!(
            keymap.handle(key_press_with("s", Modifiers::CTRL, false)),
            Some(Command::Comments(CommentsCommand::SaveNote))
        ));
        assert!(keymap
            .handle(key_press_with("o", Modifiers::CTRL, false))
            .is_none());
    }

    /// Ctrl+F opens the find popup in any mode; while it is open, plain
    /// keys type into its query field and only Escape closes it.
    #[test]
    fn find_popup_swallows_plain_keys_but_not_ctrl() {
        for mut keymap in [Keymap::new(false), viewing()] {
            assert!(matches!(
                keymap.handle(key_press_with("f", Modifiers::CTRL, false)),
                Some(Command::Find(FindCommand::Open))
            ));
            keymap.note(Transition::FindOpened);
            assert_eq!(keymap.mode(), Mode::Find);

            // Typing reaches the query field, motions included.
            for key in ["j", "k", "g", "c", "x"] {
                assert!(
                    keymap.handle(key_press(key, false)).is_none(),
                    "'{key}' should be swallowed by the find popup"
                );
            }

            // Global shortcuts still work.
            assert!(matches!(
                keymap.handle(key_press_with("b", Modifiers::CTRL, false)),
                Some(Command::Comments(CommentsCommand::ToggleSidebar))
            ));

            assert!(matches!(
                keymap.handle(escape()),
                Some(Command::Find(FindCommand::Close))
            ));
            keymap.note(Transition::FindClosed);
            assert!(keymap.mode() != Mode::Find);
        }
    }

    /// While the find popup is open, Enter and Ctrl+G step to the next
    /// match and Shift+Enter to the previous one; Ctrl+G does nothing
    /// once the popup is closed.
    #[test]
    fn find_navigation_keys_step_through_matches() {
        let mut keymap = viewing();
        keymap.note(Transition::FindOpened);

        assert!(matches!(
            keymap.handle(named(key::Named::Enter, Modifiers::default())),
            Some(Command::Find(FindCommand::Next))
        ));
        // Holding Enter keeps stepping, like holding `n` in vim.
        assert!(matches!(
            keymap.handle(named(key::Named::Enter, Modifiers::default())),
            Some(Command::Find(FindCommand::Next))
        ));
        assert!(matches!(
            keymap.handle(named(key::Named::Enter, Modifiers::SHIFT)),
            Some(Command::Find(FindCommand::Previous))
        ));
        assert!(matches!(
            keymap.handle(key_press_with("g", Modifiers::CTRL, false)),
            Some(Command::Find(FindCommand::Next))
        ));

        keymap.note(Transition::FindClosed);
        assert!(keymap
            .handle(key_press_with("g", Modifiers::CTRL, false))
            .is_none());
    }

    /// Help is a modal overlay: `Ctrl + ?` opens it from every mode
    /// (`Ctrl + /` is the same chord on US layouts, where `?` is shifted
    /// `/`), a plain `?` no longer does, Escape and the `Ctrl + ?` toggle
    /// close it, plain keys reach the window's search field through the
    /// widget tree instead of the keymap, and a pending preview prefix
    /// survives underneath.
    #[test]
    fn help_is_a_transparent_modal_overlay() {
        let mut write = Keymap::new(false);
        assert_eq!(write.mode(), Mode::Write);

        // A plain `?` no longer opens help — in write mode it types.
        assert!(write.handle(key_press("?", false)).is_none());

        assert!(matches!(
            write.handle(key_press_with("?", Modifiers::CTRL, false)),
            Some(Command::Help(HelpCommand::Open))
        ));
        assert!(matches!(
            write.handle(key_press_with("/", Modifiers::CTRL, false)),
            Some(Command::Help(HelpCommand::Open))
        ));
        write.note(Transition::HelpOpened);
        assert!(write.help_open());
        assert_eq!(write.mode(), Mode::Write);

        // Every key but the closers is left to the help window's search
        // field, which receives it through the widget tree.
        for key in ["j", "g", "?", "x"] {
            assert!(
                write.handle(key_press(key, false)).is_none(),
                "'{key}' should reach the help search field, not the keymap"
            );
        }

        // Escape closes; the chord toggles closed too.
        assert!(matches!(
            write.handle(escape()),
            Some(Command::Help(HelpCommand::Close))
        ));
        assert!(matches!(
            write.handle(key_press_with("?", Modifiers::CTRL, false)),
            Some(Command::Help(HelpCommand::Close))
        ));
        write.note(Transition::HelpClosed);
        assert!(!write.help_open());
        assert_eq!(write.mode(), Mode::Write);

        let mut view = viewing();
        assert!(view.handle(key_press("?", false)).is_none());
        view.note(Transition::CountPressed(3));
        view.note(Transition::GArmed);
        view.note(Transition::HelpOpened);
        assert_eq!(view.mode(), Mode::View);
        view.note(Transition::HelpActivity);
        view.note(Transition::HelpClosed);
        assert!(matches!(
            view.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::Jump(Jump::First, 3)))
        ));
    }

    #[test]
    fn help_preserves_pending_count_and_prefixes() {
        let mut count = viewing();
        count.note(Transition::CountPressed(4));
        count.note(Transition::HelpOpened);
        count.note(Transition::HelpActivity);
        count.note(Transition::HelpClosed);
        assert_eq!(count.pending_count(), 4);
        assert!(matches!(
            count.handle(key_press("j", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Down, 4)))
        ));

        let mut g = viewing();
        g.note(Transition::CountPressed(3));
        g.note(Transition::GArmed);
        g.note(Transition::HelpOpened);
        g.note(Transition::HelpActivity);
        g.note(Transition::HelpClosed);
        assert_eq!(g.pending_count(), 3);
        assert!(matches!(
            g.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::Jump(Jump::First, 3)))
        ));

        let mut z = viewing();
        z.note(Transition::ZArmed);
        z.note(Transition::HelpOpened);
        z.note(Transition::HelpActivity);
        z.note(Transition::HelpClosed);
        assert!(matches!(
            z.handle(key_press("t", false)),
            Some(Command::Preview(PreviewCommand::ScrollCaret(
                Placement::Top
            )))
        ));
    }

    /// Ctrl+S saves the document in write and view mode; with the note
    /// popup open it saves the note instead.
    #[test]
    fn ctrl_s_saves_the_file_unless_a_note_is_open() {
        for keymap in [Keymap::new(false), viewing()] {
            assert!(matches!(
                keymap.handle(key_press_with("s", Modifiers::CTRL, false)),
                Some(Command::Document(DocumentCommand::Save))
            ));
        }

        let mut keymap = viewing();
        keymap.note(Transition::NoteOpened);
        assert!(matches!(
            keymap.handle(key_press_with("s", Modifiers::CTRL, false)),
            Some(Command::Comments(CommentsCommand::SaveNote))
        ));
    }

    /// Ctrl+B toggles the comments sidebar in any mode.
    #[test]
    fn ctrl_b_toggles_the_sidebar() {
        for keymap in [Keymap::new(false), viewing()] {
            assert!(matches!(
                keymap.handle(key_press_with("b", Modifiers::CTRL, false)),
                Some(Command::Comments(CommentsCommand::ToggleSidebar))
            ));
            // Held down, the toggle does not flip back and forth.
            assert!(keymap
                .handle(key_press_with("b", Modifiers::CTRL, true))
                .is_none());
        }
    }

    /// Preview-only mode disables switching back to write mode, from keys
    /// and from clicks alike.
    #[test]
    fn preview_only_locks_the_mode() {
        let mut keymap = Keymap::new(true);
        assert_eq!(keymap.mode(), Mode::View);
        assert!(!keymap.can_toggle_preview());

        assert!(keymap
            .handle(key_press_with("p", Modifiers::CTRL, false))
            .is_none());

        keymap.note(Transition::PreviewToggled);
        assert_eq!(keymap.mode(), Mode::View);
    }

    /// Digits accumulate a count like vim and the next motion carries it:
    /// `3j` fires `Down` with count 3, `10k` with 10. The count dies with
    /// the motion, a lone `0` is the element-start motion, a `0` after
    /// digits extends the count, and held digits never repeat.
    #[test]
    fn counts_arm_and_repeat_motions() {
        let mut keymap = viewing();

        // 3j fires Down with count 3.
        assert!(matches!(
            keymap.handle(key_press("3", false)),
            Some(Command::Preview(PreviewCommand::Count(3)))
        ));
        keymap.note(Transition::CountPressed(3));
        assert_eq!(keymap.pending_count(), 3);

        assert!(matches!(
            keymap.handle(key_press("j", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Down, 3)))
        ));
        keymap.note(Transition::Activity);
        assert_eq!(keymap.pending_count(), 0);

        // 10k: a `0` extends a pending count rather than starting a motion.
        keymap.note(Transition::CountPressed(1));
        assert!(matches!(
            keymap.handle(key_press("0", false)),
            Some(Command::Preview(PreviewCommand::Count(0)))
        ));
        keymap.note(Transition::CountPressed(0));
        assert!(matches!(
            keymap.handle(key_press("k", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Up, 10)))
        ));
        keymap.note(Transition::Activity);

        // A lone `0` is the element-start motion; a count then repeats it.
        assert!(matches!(
            keymap.handle(key_press("0", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Start, 1)))
        ));

        keymap.note(Transition::Activity);

        // 2h and 3l carry their counts.
        keymap.note(Transition::CountPressed(2));
        assert!(matches!(
            keymap.handle(key_press("h", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Left, 2)))
        ));
        keymap.note(Transition::Activity);
        keymap.note(Transition::CountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("l", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Right, 3)))
        ));
        keymap.note(Transition::Activity);

        // 3w carries its count like the character motions.
        keymap.note(Transition::CountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("w", false)),
            Some(Command::Preview(PreviewCommand::MoveWord(
                WordMotion::NextStart,
                3
            )))
        ));

        // Held digits never extend the count.
        keymap.note(Transition::CountPressed(1));
        assert!(keymap.handle(key_press("1", true)).is_none());

        // Any other message clears the pending count.
        keymap.note(Transition::Activity);
        assert_eq!(keymap.pending_count(), 0);

        // Write mode never arms counts — digits type into the editor.
        let write = Keymap::new(false);
        assert!(write.handle(key_press("3", false)).is_none());
    }

    /// The count survives the `g` prefix: `3gg` jumps with count 3 and `5G`
    /// lands on the fifth element; a plain `gg`/`G` carries no count.
    #[test]
    fn counts_survive_the_g_prefix() {
        let mut keymap = viewing();

        keymap.note(Transition::CountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::ArmG))
        ));
        keymap.note(Transition::GArmed);
        assert_eq!(keymap.pending_count(), 3);

        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::Jump(Jump::First, 3)))
        ));
        keymap.note(Transition::Activity);

        // 5G carries the count; plain G carries none.
        keymap.note(Transition::CountPressed(5));
        assert!(matches!(
            keymap.handle(key_press("G", false)),
            Some(Command::Preview(PreviewCommand::Jump(Jump::Last, 5)))
        ));
        keymap.note(Transition::Activity);
        assert!(matches!(
            keymap.handle(key_press("G", false)),
            Some(Command::Preview(PreviewCommand::Jump(Jump::Last, 0)))
        ));
    }

    /// `zz`/`zt`/`zb` scroll the caret to the viewport's middle, top, and
    /// bottom: the first `z` arms the prefix, the second key fires, and any
    /// other key ends the sequence.
    #[test]
    fn zz_zt_zb_scroll_the_caret_into_place() {
        let mut keymap = viewing();

        // zz centers.
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Command::Preview(PreviewCommand::ArmZ))
        ));
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Command::Preview(PreviewCommand::ScrollCaret(
                Placement::Center
            )))
        ));
        keymap.note(Transition::Activity);

        // zt puts the caret at the top.
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("t", false)),
            Some(Command::Preview(PreviewCommand::ScrollCaret(
                Placement::Top
            )))
        ));
        keymap.note(Transition::Activity);

        // zb puts the caret at the bottom — `b` returns to its word motion
        // once the prefix ended.
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("b", false)),
            Some(Command::Preview(PreviewCommand::ScrollCaret(
                Placement::Bottom
            )))
        ));
        keymap.note(Transition::Activity);
        assert!(matches!(
            keymap.handle(key_press("b", false)),
            Some(Command::Preview(PreviewCommand::MoveWord(
                WordMotion::PreviousStart,
                1
            )))
        ));

        // Any other key ends a pending `z` sequence.
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("j", false)),
            Some(Command::Preview(PreviewCommand::Move(Motion::Down, 1)))
        ));
        keymap.note(Transition::Activity);
        assert!(keymap.handle(key_press("t", false)).is_none());

        // Held `z` never arms or fires.
        assert!(keymap.handle(key_press("z", true)).is_none());

        // In write mode `z` types into the editor.
        let write = Keymap::new(false);
        assert!(write.handle(key_press("z", false)).is_none());
    }

    /// Half and full pages scroll the preview: Ctrl+D/Ctrl+U scroll half a
    /// page down/up and PageDown/PageUp a full one, counts repeating
    /// pages. In write mode the editor keeps its own page keys.
    #[test]
    fn page_keys_scroll_the_preview() {
        let keymap = viewing();

        assert!(matches!(
            keymap.handle(named(key::Named::PageDown, Modifiers::default())),
            Some(Command::Preview(PreviewCommand::ScrollPage(
                Page::FullDown,
                1
            )))
        ));
        assert!(matches!(
            keymap.handle(named(key::Named::PageUp, Modifiers::default())),
            Some(Command::Preview(PreviewCommand::ScrollPage(
                Page::FullUp,
                1
            )))
        ));
        assert!(matches!(
            keymap.handle(key_press_with("d", Modifiers::CTRL, false)),
            Some(Command::Preview(PreviewCommand::ScrollPage(
                Page::HalfDown,
                1
            )))
        ));
        assert!(matches!(
            keymap.handle(key_press_with("u", Modifiers::CTRL, false)),
            Some(Command::Preview(PreviewCommand::ScrollPage(
                Page::HalfUp,
                1
            )))
        ));

        // 3Ctrl+D scrolls three half pages.
        let mut keymap = viewing();
        keymap.note(Transition::CountPressed(3));
        assert!(matches!(
            keymap.handle(key_press_with("d", Modifiers::CTRL, false)),
            Some(Command::Preview(PreviewCommand::ScrollPage(
                Page::HalfDown,
                3
            )))
        ));

        // Write mode keeps Ctrl+D and the page keys for its editor.
        let write = Keymap::new(false);
        assert!(write
            .handle(key_press_with("d", Modifiers::CTRL, false))
            .is_none());
        assert!(write
            .handle(named(key::Named::PageDown, Modifiers::default()))
            .is_none());
    }

    /// Enter in the preview opens the active comment for editing; without
    /// one it does nothing (update decides), and the popups keep their own
    /// Enter meanings.
    #[test]
    fn enter_edits_the_active_comment() {
        let keymap = viewing();
        assert!(matches!(
            keymap.handle(named(key::Named::Enter, Modifiers::default())),
            Some(Command::Comments(CommentsCommand::EditActive))
        ));

        // Enter repeats do not re-trigger the popup.
        let held_enter = |repeat: bool| keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Enter),
            modified_key: keyboard::Key::Named(key::Named::Enter),
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::default(),
            text: None,
            repeat,
        };
        assert!(keymap.handle(held_enter(true)).is_none());

        // While the note popup is open, Enter types a newline instead.
        let mut note = viewing();
        note.note(Transition::NoteOpened);
        assert!(note
            .handle(named(key::Named::Enter, Modifiers::default()))
            .is_none());

        // While find is open, Enter steps to the next match.
        let mut find = viewing();
        find.note(Transition::FindOpened);
        assert!(matches!(
            find.handle(named(key::Named::Enter, Modifiers::default())),
            Some(Command::Find(FindCommand::Next))
        ));
    }

    /// Ctrl+Enter triggers the sidebar's Add button and Ctrl+Shift+Enter
    /// its Publish button, in every mode — even with the sidebar's draft
    /// editor focused.
    #[test]
    fn ctrl_enter_triggers_add_and_publish() {
        for keymap in [Keymap::new(false), viewing()] {
            assert!(matches!(
                keymap.handle(key_press_named_enter(Modifiers::CTRL, false)),
                Some(Command::Comments(CommentsCommand::AddGlobal))
            ));
            assert!(matches!(
                keymap.handle(key_press_named_enter(
                    Modifiers::CTRL | Modifiers::SHIFT,
                    false
                )),
                Some(Command::Comments(CommentsCommand::Publish))
            ));
        }

        // With the note popup open, the popup keeps every Ctrl combo but
        // Ctrl+S.
        let mut note = viewing();
        note.note(Transition::NoteOpened);
        assert!(note
            .handle(key_press_named_enter(Modifiers::CTRL, false))
            .is_none());
    }

    /// Arming one `g`/`z` prefix disarms the other, and digits end a
    /// pending prefix: `gzg` never fires `gg`, `zgb` never fires `zb`, and
    /// `z3z` never fires `zz` — an unrelated key cancels the sequence.
    #[test]
    fn g_and_z_prefixes_are_mutually_exclusive() {
        let mut keymap = viewing();

        // gzg: the middle z ends the g sequence; the final g only re-arms.
        keymap.note(Transition::GArmed);
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Command::Preview(PreviewCommand::ArmZ))
        ));
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::ArmG))
        ));
        keymap.note(Transition::GArmed);
        // Now gg would fire — but the point is z's g did not. Arm z and
        // check the mirrored case.
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::Jump(Jump::First, 0)))
        ));
        keymap.note(Transition::Activity);

        // zgb: the middle g ends the z sequence, so b is just a word
        // motion — never zb's scroll.
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Command::Preview(PreviewCommand::ArmG))
        ));
        keymap.note(Transition::GArmed);
        assert!(matches!(
            keymap.handle(key_press("b", false)),
            Some(Command::Preview(PreviewCommand::MoveWord(
                WordMotion::PreviousStart,
                1
            )))
        ));
        keymap.note(Transition::Activity);

        // ztz fires normally after the fix: arm z fresh and complete it.
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("t", false)),
            Some(Command::Preview(PreviewCommand::ScrollCaret(
                Placement::Top
            )))
        ));

        // A digit ends a pending prefix: z3z is two fresh presses.
        keymap.note(Transition::ZArmed);
        keymap.note(Transition::CountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Command::Preview(PreviewCommand::ArmZ))
        ));
        keymap.note(Transition::ZArmed);
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Command::Preview(PreviewCommand::ScrollCaret(
                Placement::Center
            )))
        ));
    }

    #[test]
    fn note_find_and_unsaved_cancel_pending_input() {
        for (name, open, close) in [
            ("note", Transition::NoteOpened, Transition::NoteClosed),
            ("find", Transition::FindOpened, Transition::FindClosed),
            (
                "unsaved",
                Transition::UnsavedOpened(UnsavedAction::OpenFile),
                Transition::UnsavedClosed,
            ),
        ] {
            let mut g = viewing();
            g.note(Transition::CountPressed(7));
            g.note(Transition::GArmed);
            g.note(open);
            assert_eq!(
                g.pending_count(),
                0,
                "opening {name} should cancel the pending count"
            );
            g.note(close);
            assert!(
                matches!(
                    g.handle(key_press("g", false)),
                    Some(Command::Preview(PreviewCommand::ArmG))
                ),
                "opening {name} should cancel the pending g prefix"
            );

            let mut z = viewing();
            z.note(Transition::ZArmed);
            z.note(open);
            z.note(close);
            assert!(
                z.handle(key_press("t", false)).is_none(),
                "opening {name} should cancel the pending z prefix"
            );
        }
    }

    /// The unsaved-changes dialog swallows every key but Escape, and its
    /// messages open and close it; the mode beneath stays untouched.
    #[test]
    fn unsaved_dialog_is_a_modal() {
        let mut keymap = viewing();
        keymap.note(Transition::UnsavedOpened(UnsavedAction::OpenFile));
        assert!(keymap.unsaved_open());
        assert_eq!(keymap.mode(), Mode::View);

        // Motions and shortcuts are swallowed; Escape cancels.
        for key in ["j", "k", "g", "c", "0", "3"] {
            assert!(
                keymap.handle(key_press(key, false)).is_none(),
                "'{key}' should be swallowed by the unsaved dialog"
            );
        }
        assert!(keymap
            .handle(key_press_with("s", Modifiers::CTRL, false))
            .is_none());
        assert!(matches!(
            keymap.handle(escape()),
            Some(Command::Document(DocumentCommand::CancelUnsaved))
        ));

        // Answering closes it too.
        keymap.note(Transition::UnsavedClosed);
        assert!(!keymap.unsaved_open());
        keymap.note(Transition::UnsavedOpened(UnsavedAction::OpenFile));
        keymap.note(Transition::UnsavedClosed);
        assert!(!keymap.unsaved_open());
        assert_eq!(keymap.mode(), Mode::View);
    }

    fn key_press_named_enter(modifiers: Modifiers, _repeat: bool) -> keyboard::Event {
        named(key::Named::Enter, modifiers)
    }
}
