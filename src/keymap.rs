//! The keymap: one representation of input mode, read by both key handling
//! and the mode badge.
//!
//! The mode is a small stack: write or view at the base, visual mode as a
//! layer over view, and the note popup floating on top of either. [`Keymap`]
//! owns the stack plus the pending `g` of a `gg`/`ge` sequence, behind two
//! entry points: [`Keymap::handle`] turns a key event into an optional app
//! message, and [`Keymap::note`] keeps the stack in sync with
//! message-driven transitions (button clicks, backdrop clicks) so the two
//! never disagree.

use iced::keyboard;

use crate::preview::{Jump, Motion, Page, Placement, WordMotion};
use crate::Message;

/// The highest a pending count may grow: `99999j` and `9999999999j` both
/// scroll to the document's end instead of overflowing.
const MAX_COUNT: u32 = 99_999;

/// The mode the editor is in, as shown by the bottom-bar badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Editing the Markdown source.
    Write,
    /// Previewing, caret moves without selecting.
    View,
    /// Previewing, motions extend the selection.
    Visual,
    /// The note popup is open over the preview.
    Note,
    /// The find popup is open.
    Find,
}

/// The mode stack without the note popup: write, or preview with an
/// optional visual layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Layer {
    Write,
    View,
    Visual,
}

/// The input mode stack and the pending `g` sequence state.
///
/// `Clone` + `Hash` so the keyboard subscription can carry a snapshot and
/// re-listen when the mode changes; key handling reads the snapshot
/// ([`Keymap::handle`]) while transitions happen through messages
/// ([`Keymap::note`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keymap {
    layer: Layer,
    note_open: bool,
    find_open: bool,
    /// Whether the help overlay is open. This is deliberately kept out of
    /// [`Keymap::mode`] so opening help does not change the underlying mode.
    help_open: bool,
    /// Whether the unsaved-changes dialog is open. Like help, it floats
    /// above the underlying mode and swallows every key but Escape.
    unsaved_open: bool,
    preview_only: bool,
    /// Whether a lone `g` is awaiting its second key of a `gg`/`ge`
    /// sequence.
    pending_g: bool,
    /// Whether a lone `z` is awaiting its second key of a `zz`/`zt`/`zb`
    /// sequence.
    pending_z: bool,
    /// The digits of a pending motion count, like vim's `3` of `3j`.
    pending_count: Option<u32>,
}

impl Keymap {
    /// A keymap starting in write mode — or in view mode when the app runs
    /// preview-only.
    pub fn new(preview_only: bool) -> Self {
        Self {
            layer: if preview_only {
                Layer::View
            } else {
                Layer::Write
            },
            note_open: false,
            find_open: false,
            help_open: false,
            unsaved_open: false,
            preview_only,
            pending_g: false,
            pending_z: false,
            pending_count: None,
        }
    }

    /// The current mode, popups included — the badge reads this.
    pub fn mode(&self) -> Mode {
        if self.note_open {
            Mode::Note
        } else if self.find_open {
            Mode::Find
        } else {
            match self.layer {
                Layer::Write => Mode::Write,
                Layer::View => Mode::View,
                Layer::Visual => Mode::Visual,
            }
        }
    }

    /// Whether the preview is showing — in every mode but write. The note
    /// popup only ever floats above the preview, so it counts as preview.
    pub fn preview(&self) -> bool {
        !matches!(self.layer, Layer::Write)
    }

    /// Whether the app runs preview-only (`--preview`): editing and
    /// switching to write mode are disabled.
    pub fn preview_only(&self) -> bool {
        self.preview_only
    }

    /// Whether the note popup is open.
    pub fn note_open(&self) -> bool {
        self.note_open
    }

    /// Whether the find popup is open.
    pub fn find_open(&self) -> bool {
        self.find_open
    }

    /// Whether the help overlay is open. The mode itself remains unchanged
    /// while help is visible.
    pub fn help_open(&self) -> bool {
        self.help_open
    }

    /// Whether the unsaved-changes dialog is open. The mode itself remains
    /// unchanged beneath it.
    pub fn unsaved_open(&self) -> bool {
        self.unsaved_open
    }

    /// The pending motion count, for the mode badge's `3×` hint. `0` when
    /// no count is pending.
    pub fn pending_count(&self) -> u32 {
        self.pending_count.unwrap_or(0)
    }

    /// The count a motion repeats: the typed digits, or once.
    fn count(&self) -> usize {
        self.pending_count.unwrap_or(1).min(MAX_COUNT) as usize
    }

    /// The count a jump carries: the typed digits, or `0` — none — so a
    /// plain `gg`/`G` keeps its first/last meaning.
    fn jump_count(&self) -> usize {
        self.pending_count.map_or(0, |count| count.min(MAX_COUNT) as usize)
    }

    /// Whether visual mode is active — motions extend the selection.
    pub fn visual(&self) -> bool {
        matches!(self.layer, Layer::Visual)
    }

    /// Turns a key event into the app message it means in the current
    /// mode, if any.
    ///
    /// This only reads the mode; the transitions themselves happen in
    /// [`Keymap::note`] once the produced message is dispatched, so key
    /// handling and message handling never see different states.
    pub fn handle(&self, event: keyboard::Event) -> Option<Message> {
        let keyboard::Event::KeyPressed {
            modified_key,
            modifiers,
            repeat,
            ..
        } = event
        else {
            return None;
        };

        // Help is a read-only modal overlay. It takes the question-mark
        // shortcut before any other popup or mode can interpret it, and
        // swallows every key except Escape. No underlying editor state is
        // touched while it is open.
        if self.help_open {
            return match modified_key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Message::CloseHelp)
                }
                _ => None,
            };
        }

        // The unsaved-changes dialog is a modal too: Escape cancels it, the
        // three buttons answer it, and every other key is swallowed so the
        // editing surface beneath stays untouched.
        if self.unsaved_open {
            return match modified_key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Message::UnsavedCancel)
                }
                _ => None,
            };
        }

        // `?` opens help from every mode, including while another popup is
        // open. The existing popup and editor state remains underneath it.
        if !modifiers.control()
            && !modifiers.alt()
            && !modifiers.logo()
            && !repeat
            && matches!(modified_key.as_ref(), keyboard::Key::Character("?"))
        {
            return Some(Message::OpenHelp);
        }

        // The note popup swallows plain keys for its text area; only Escape
        // closes it and Ctrl+S saves the comment.
        if self.note_open && !modifiers.alt() && !modifiers.logo() {
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

        // The find popup swallows plain keys for its query field; only
        // Escape closes it and Enter steps through the matches.
        // Ctrl combinations still reach the global shortcuts below.
        if self.find_open && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            return match modified_key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Message::CloseFind)
                }
                // Enter steps to the next match, Shift+Enter back to the
                // previous one — welcome to repeat, like holding `n` in
                // vim.
                keyboard::Key::Named(keyboard::key::Named::Enter) => Some(if modifiers.shift() {
                    Message::FindPrevious
                } else {
                    Message::FindNext
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
                    if !repeat
                        && c.chars().all(|character| character.is_ascii_digit()) =>
                {
                    if c == "0" && self.pending_count.is_none() {
                        Some(Message::MovePreviewCursor(Motion::Start, 1))
                    } else {
                        Some(Message::PreviewCountPressed(c.parse().unwrap_or(0)))
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowUp)
                | keyboard::Key::Character("k" | "K") => {
                    Some(Message::MovePreviewCursor(Motion::Up, self.count()))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowDown)
                | keyboard::Key::Character("j" | "J") => {
                    Some(Message::MovePreviewCursor(Motion::Down, self.count()))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
                | keyboard::Key::Character("h" | "H") => {
                    Some(Message::MovePreviewCursor(Motion::Left, self.count()))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowRight)
                | keyboard::Key::Character("l" | "L") => {
                    Some(Message::MovePreviewCursor(Motion::Right, self.count()))
                }
                // Full pages: PageUp/PageDown scroll the preview like
                // vim's `Ctrl+F`/`Ctrl+B`, with the count repeating pages.
                keyboard::Key::Named(keyboard::key::Named::PageUp) => {
                    Some(Message::ScrollPreviewPage(Page::FullUp, self.count()))
                }
                keyboard::Key::Named(keyboard::key::Named::PageDown) => {
                    Some(Message::ScrollPreviewPage(Page::FullDown, self.count()))
                }
                // `zz`/`zt`/`zb` — scroll the caret to the middle, top, or
                // bottom of the viewport. The second key is always fresh.
                keyboard::Key::Character("z") if self.pending_z && !repeat => {
                    Some(Message::ScrollPreviewCaret(Placement::Center))
                }
                keyboard::Key::Character("t") if self.pending_z && !repeat => {
                    Some(Message::ScrollPreviewCaret(Placement::Top))
                }
                keyboard::Key::Character("b") if self.pending_z && !repeat => {
                    Some(Message::ScrollPreviewCaret(Placement::Bottom))
                }
                // The first `z` of a `zz`/`zt`/`zb` sequence.
                keyboard::Key::Character("z") if !repeat => Some(Message::PreviewZPressed),
                // `gg` — the second `g` of the sequence is always a fresh
                // press.
                keyboard::Key::Character("g") if self.pending_g && !repeat => {
                    Some(Message::MovePreviewJump(Jump::First, self.jump_count()))
                }
                // The first `g` of a `gg`/`ge` sequence.
                keyboard::Key::Character("g") if !repeat => Some(Message::PreviewGPressed),
                // `ge` — the `e` of the sequence is always a fresh press.
                keyboard::Key::Character("e" | "E") if self.pending_g && !repeat => Some(
                    Message::MovePreviewWord(WordMotion::PreviousEnd, self.count()),
                ),
                keyboard::Key::Character("e" | "E") => {
                    Some(Message::MovePreviewWord(WordMotion::NextEnd, self.count()))
                }
                keyboard::Key::Character("w" | "W") => Some(Message::MovePreviewWord(
                    WordMotion::NextStart,
                    self.count(),
                )),
                keyboard::Key::Character("b" | "B") => Some(Message::MovePreviewWord(
                    WordMotion::PreviousStart,
                    self.count(),
                )),
                keyboard::Key::Character("G") => {
                    Some(Message::MovePreviewJump(Jump::Last, self.jump_count()))
                }
                keyboard::Key::Character("v" | "V") if !repeat => Some(Message::ToggleVisualMode),
                keyboard::Key::Character("c" | "C") if !repeat => Some(Message::OpenNotePopup),
                // Enter opens the active comment for editing, like the
                // sidebar's edit affordance.
                keyboard::Key::Named(keyboard::key::Named::Enter) if !repeat => {
                    Some(Message::EditActiveComment)
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) if !repeat => {
                    Some(Message::PreviewCancel)
                }
                _ => None,
            };
        }

        if modifiers.control() && !repeat {
            match modified_key.as_ref() {
                keyboard::Key::Character("o" | "O") => Some(Message::OpenFile),
                keyboard::Key::Character("p" | "P") if !self.preview_only => {
                    Some(Message::TogglePreview)
                }
                // Show or hide the comments sidebar.
                keyboard::Key::Character("b" | "B") => Some(Message::ToggleSidebar),
                // Save the document, asking for a path if it was never
                // saved. With the note popup open, Ctrl+S saves the note
                // instead — the popup guard above took that path already.
                keyboard::Key::Character("s" | "S") => Some(Message::SaveFile),
                // Find in the document, in any mode.
                keyboard::Key::Character("f" | "F") => Some(Message::OpenFind),
                // Step to the next match while the find popup is open.
                keyboard::Key::Character("g" | "G") if self.find_open => Some(Message::FindNext),
                // Browse the saved comments in the preview.
                keyboard::Key::Character("n" | "N") if self.preview() => Some(Message::NextComment),
                // Half pages: Ctrl+D/Ctrl+U scroll the preview like vim,
                // with the count repeating half pages.
                keyboard::Key::Character("d" | "D") if self.preview() => {
                    Some(Message::ScrollPreviewPage(Page::HalfDown, self.count()))
                }
                keyboard::Key::Character("u" | "U") if self.preview() => {
                    Some(Message::ScrollPreviewPage(Page::HalfUp, self.count()))
                }
                // The comments sidebar's buttons: Ctrl+Enter triggers Add,
                // Ctrl+Shift+Enter triggers Publish.
                keyboard::Key::Named(keyboard::key::Named::Enter) => Some(
                    if modifiers.shift() {
                        Message::PublishPressed
                    } else {
                        Message::AddGlobalComment
                    },
                ),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Keeps the mode stack in sync with a dispatched message, so
    /// transitions triggered without keys (button and backdrop clicks)
    /// land exactly like the key-driven ones.
    pub fn note(&mut self, message: &Message) {
        // Any message other than arming a prefix ends a pending
        // `gg`/`ge`/`zz`-style sequence and a pending count, like any other
        // key would. Help is transparent to this state: opening and closing
        // it must not alter what was underneath.
        let keeps_pending = matches!(
            message,
            Message::PreviewGPressed
                | Message::PreviewZPressed
                | Message::PreviewCountPressed(_)
                | Message::OpenHelp
                | Message::CloseHelp
                | Message::HelpCardPressed
        );

        if keeps_pending {
            // Arming one prefix disarms the other: a `g` or `z` in the
            // middle of a sequence ends it, like any unrelated key would.
            if matches!(message, Message::PreviewGPressed) {
                self.pending_g = true;
                self.pending_z = false;
            }

            if matches!(message, Message::PreviewZPressed) {
                self.pending_z = true;
                self.pending_g = false;
            }

            if let Message::PreviewCountPressed(digit) = message {
                // Digits extend the count and end any pending prefix — a
                // count never carries across one in vim.
                self.pending_g = false;
                self.pending_z = false;

                let digits = self.pending_count.unwrap_or(0);
                self.pending_count =
                    Some(digits.saturating_mul(10).saturating_add(*digit).min(MAX_COUNT));
            }
        } else {
            self.pending_g = false;
            self.pending_z = false;
            self.pending_count = None;
        }

        match message {
            Message::FileLoaded(Some(_)) => {
                self.note_open = false;
                self.find_open = false;

                if matches!(self.layer, Layer::Visual) {
                    self.layer = Layer::View;
                }
            }
            Message::TogglePreview if !self.preview_only => {
                self.note_open = false;
                self.layer = if self.preview() {
                    Layer::Write
                } else {
                    Layer::View
                };
            }
            Message::ToggleVisualMode if self.preview() => {
                self.layer = if matches!(self.layer, Layer::Visual) {
                    Layer::View
                } else {
                    Layer::Visual
                };
            }
            Message::OpenNotePopup => self.note_open = true,
            Message::CloseNotePopup | Message::SaveNote => self.note_open = false,
            Message::OpenFind => self.find_open = true,
            Message::CloseFind => self.find_open = false,
            Message::OpenHelp => self.help_open = true,
            Message::CloseHelp => self.help_open = false,
            // The unsaved-changes dialog: shown when update decides the
            // document is modified, answered by its three buttons or
            // Escape.
            Message::UnsavedChanges => self.unsaved_open = true,
            Message::UnsavedCancel | Message::UnsavedSave | Message::UnsavedDiscard => {
                self.unsaved_open = false
            }
            // Editing the active comment opens the note popup from update,
            // where the active comment is known — an empty answer keeps
            // every mode as it is.
            Message::EditActiveComment => {}
            Message::PreviewCancel => {
                if matches!(self.layer, Layer::Visual) {
                    self.layer = Layer::View;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Keymap, Mode};
    use crate::preview::{Jump, Motion, Page, Placement, WordMotion};
    use crate::Message;
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
        keymap.note(&Message::TogglePreview);
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

        keymap.note(&Message::TogglePreview);
        assert_eq!(keymap.mode(), Mode::View);
        assert!(keymap.preview());

        keymap.note(&Message::ToggleVisualMode);
        assert_eq!(keymap.mode(), Mode::Visual);

        // The note popup floats above visual mode and closing it returns
        // there.
        keymap.note(&Message::OpenNotePopup);
        assert_eq!(keymap.mode(), Mode::Note);
        keymap.note(&Message::CloseNotePopup);
        assert_eq!(keymap.mode(), Mode::Visual);

        keymap.note(&Message::ToggleVisualMode);
        assert_eq!(keymap.mode(), Mode::View);

        keymap.note(&Message::TogglePreview);
        assert_eq!(keymap.mode(), Mode::Write);
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
            Some(Message::MovePreviewCursor(Motion::Down, 1))
        ));
        assert!(matches!(
            pressed("j", true),
            Some(Message::MovePreviewCursor(Motion::Down, 1))
        ));
        assert!(matches!(
            pressed("k", true),
            Some(Message::MovePreviewCursor(Motion::Up, 1))
        ));
        assert!(matches!(
            pressed("h", true),
            Some(Message::MovePreviewCursor(Motion::Left, 1))
        ));
        assert!(matches!(
            pressed("l", true),
            Some(Message::MovePreviewCursor(Motion::Right, 1))
        ));
        assert!(matches!(
            pressed("w", true),
            Some(Message::MovePreviewWord(WordMotion::NextStart, 1))
        ));
        assert!(matches!(
            pressed("b", true),
            Some(Message::MovePreviewWord(WordMotion::PreviousStart, 1))
        ));
        assert!(matches!(
            pressed("e", true),
            Some(Message::MovePreviewWord(WordMotion::NextEnd, 1))
        ));

        // One-shot actions ignore repeats.
        assert!(matches!(pressed("c", false), Some(Message::OpenNotePopup)));
        assert!(pressed("c", true).is_none());

        // The `g` prefix never arms or fires on repeat — `gg` and `ge` need
        // two fresh presses.
        assert!(matches!(
            pressed("g", false),
            Some(Message::PreviewGPressed)
        ));
        assert!(pressed("g", true).is_none());

        // `v` toggles visual mode on fresh presses only.
        assert!(matches!(
            pressed("v", false),
            Some(Message::ToggleVisualMode)
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
            Some(Message::PreviewGPressed)
        ));
        keymap.note(&Message::PreviewGPressed);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::MovePreviewJump(Jump::First, 0))
        ));

        // The sequence fired; a third `g` starts over.
        keymap.note(&Message::MovePreviewJump(Jump::First, 0));
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::PreviewGPressed)
        ));

        // Any other key ends the pending sequence.
        keymap.note(&Message::PreviewGPressed);
        assert!(matches!(
            keymap.handle(key_press("j", false)),
            Some(Message::MovePreviewCursor(Motion::Down, 1))
        ));
        keymap.note(&Message::MovePreviewCursor(Motion::Down, 1));
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::PreviewGPressed)
        ));

        // `ge` moves to the end of the previous word.
        keymap.note(&Message::PreviewGPressed);
        assert!(matches!(
            keymap.handle(key_press("e", false)),
            Some(Message::MovePreviewWord(WordMotion::PreviousEnd, 1))
        ));
    }

    /// While the note popup is open, plain keys go to its text area — only
    /// Escape closes it and Ctrl+S saves the comment.
    #[test]
    fn note_popup_swallows_keys() {
        let mut keymap = viewing();
        keymap.note(&Message::OpenNotePopup);

        assert!(matches!(
            keymap.handle(escape()),
            Some(Message::CloseNotePopup)
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
            Some(Message::SaveNote)
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
                Some(Message::OpenFind)
            ));
            keymap.note(&Message::OpenFind);
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
                Some(Message::ToggleSidebar)
            ));

            assert!(matches!(keymap.handle(escape()), Some(Message::CloseFind)));
            keymap.note(&Message::CloseFind);
            assert!(keymap.mode() != Mode::Find);
        }
    }

    /// While the find popup is open, Enter and Ctrl+G step to the next
    /// match and Shift+Enter to the previous one; Ctrl+G does nothing
    /// once the popup is closed.
    #[test]
    fn find_navigation_keys_step_through_matches() {
        let mut keymap = viewing();
        keymap.note(&Message::OpenFind);

        assert!(matches!(
            keymap.handle(named(key::Named::Enter, Modifiers::default())),
            Some(Message::FindNext)
        ));
        // Holding Enter keeps stepping, like holding `n` in vim.
        assert!(matches!(
            keymap.handle(named(key::Named::Enter, Modifiers::default())),
            Some(Message::FindNext)
        ));
        assert!(matches!(
            keymap.handle(named(key::Named::Enter, Modifiers::SHIFT)),
            Some(Message::FindPrevious)
        ));
        assert!(matches!(
            keymap.handle(key_press_with("g", Modifiers::CTRL, false)),
            Some(Message::FindNext)
        ));

        keymap.note(&Message::CloseFind);
        assert!(keymap
            .handle(key_press_with("g", Modifiers::CTRL, false))
            .is_none());
    }

    /// Help opens from every underlying mode, swallows keys while visible,
    /// closes with Escape, and leaves a pending preview prefix untouched.
    #[test]
    fn help_is_a_transparent_modal_overlay() {
        let mut write = Keymap::new(false);
        assert_eq!(write.mode(), Mode::Write);
        assert!(matches!(
            write.handle(key_press("?", false)),
            Some(Message::OpenHelp)
        ));
        write.note(&Message::OpenHelp);
        assert!(write.help_open());
        assert_eq!(write.mode(), Mode::Write);
        assert!(write.handle(key_press("j", false)).is_none());
        assert!(matches!(write.handle(escape()), Some(Message::CloseHelp)));
        write.note(&Message::CloseHelp);
        assert!(!write.help_open());
        assert_eq!(write.mode(), Mode::Write);

        let mut view = viewing();
        view.note(&Message::PreviewGPressed);
        view.note(&Message::OpenHelp);
        assert_eq!(view.mode(), Mode::View);
        view.note(&Message::CloseHelp);
        assert!(matches!(
            view.handle(key_press("g", false)),
            Some(Message::MovePreviewJump(Jump::First, 0))
        ));
    }

    /// Ctrl+S saves the document in write and view mode; with the note
    /// popup open it saves the note instead.
    #[test]
    fn ctrl_s_saves_the_file_unless_a_note_is_open() {
        for keymap in [Keymap::new(false), viewing()] {
            assert!(matches!(
                keymap.handle(key_press_with("s", Modifiers::CTRL, false)),
                Some(Message::SaveFile)
            ));
        }

        let mut keymap = viewing();
        keymap.note(&Message::OpenNotePopup);
        assert!(matches!(
            keymap.handle(key_press_with("s", Modifiers::CTRL, false)),
            Some(Message::SaveNote)
        ));
    }

    /// Ctrl+B toggles the comments sidebar in any mode.
    #[test]
    fn ctrl_b_toggles_the_sidebar() {
        for keymap in [Keymap::new(false), viewing()] {
            assert!(matches!(
                keymap.handle(key_press_with("b", Modifiers::CTRL, false)),
                Some(Message::ToggleSidebar)
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
        assert!(keymap.preview_only());

        assert!(keymap
            .handle(key_press_with("p", Modifiers::CTRL, false))
            .is_none());

        keymap.note(&Message::TogglePreview);
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
            Some(Message::PreviewCountPressed(3))
        ));
        keymap.note(&Message::PreviewCountPressed(3));
        assert_eq!(keymap.pending_count(), 3);

        assert!(matches!(
            keymap.handle(key_press("j", false)),
            Some(Message::MovePreviewCursor(Motion::Down, 3))
        ));
        keymap.note(&Message::MovePreviewCursor(Motion::Down, 3));
        assert_eq!(keymap.pending_count(), 0);

        // 10k: a `0` extends a pending count rather than starting a motion.
        keymap.note(&Message::PreviewCountPressed(1));
        assert!(matches!(
            keymap.handle(key_press("0", false)),
            Some(Message::PreviewCountPressed(0))
        ));
        keymap.note(&Message::PreviewCountPressed(0));
        assert!(matches!(
            keymap.handle(key_press("k", false)),
            Some(Message::MovePreviewCursor(Motion::Up, 10))
        ));
        keymap.note(&Message::MovePreviewCursor(Motion::Up, 10));

        // A lone `0` is the element-start motion; a count then repeats it.
        assert!(matches!(
            keymap.handle(key_press("0", false)),
            Some(Message::MovePreviewCursor(Motion::Start, 1))
        ));

        keymap.note(&Message::MovePreviewCursor(Motion::Start, 1));

        // 2h and 3l carry their counts.
        keymap.note(&Message::PreviewCountPressed(2));
        assert!(matches!(
            keymap.handle(key_press("h", false)),
            Some(Message::MovePreviewCursor(Motion::Left, 2))
        ));
        keymap.note(&Message::MovePreviewCursor(Motion::Left, 2));
        keymap.note(&Message::PreviewCountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("l", false)),
            Some(Message::MovePreviewCursor(Motion::Right, 3))
        ));
        keymap.note(&Message::MovePreviewCursor(Motion::Right, 3));

        // 3w carries its count like the character motions.
        keymap.note(&Message::PreviewCountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("w", false)),
            Some(Message::MovePreviewWord(WordMotion::NextStart, 3))
        ));

        // Held digits never extend the count.
        keymap.note(&Message::PreviewCountPressed(1));
        assert!(keymap.handle(key_press("1", true)).is_none());

        // Any other message clears the pending count.
        keymap.note(&Message::ToggleSidebar);
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

        keymap.note(&Message::PreviewCountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::PreviewGPressed)
        ));
        keymap.note(&Message::PreviewGPressed);
        assert_eq!(keymap.pending_count(), 3);

        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::MovePreviewJump(Jump::First, 3))
        ));
        keymap.note(&Message::MovePreviewJump(Jump::First, 3));

        // 5G carries the count; plain G carries none.
        keymap.note(&Message::PreviewCountPressed(5));
        assert!(matches!(
            keymap.handle(key_press("G", false)),
            Some(Message::MovePreviewJump(Jump::Last, 5))
        ));
        keymap.note(&Message::MovePreviewJump(Jump::Last, 5));
        assert!(matches!(
            keymap.handle(key_press("G", false)),
            Some(Message::MovePreviewJump(Jump::Last, 0))
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
            Some(Message::PreviewZPressed)
        ));
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Message::ScrollPreviewCaret(Placement::Center))
        ));
        keymap.note(&Message::ScrollPreviewCaret(Placement::Center));

        // zt puts the caret at the top.
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("t", false)),
            Some(Message::ScrollPreviewCaret(Placement::Top))
        ));
        keymap.note(&Message::ScrollPreviewCaret(Placement::Top));

        // zb puts the caret at the bottom — `b` returns to its word motion
        // once the prefix ended.
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("b", false)),
            Some(Message::ScrollPreviewCaret(Placement::Bottom))
        ));
        keymap.note(&Message::ScrollPreviewCaret(Placement::Bottom));
        assert!(matches!(
            keymap.handle(key_press("b", false)),
            Some(Message::MovePreviewWord(WordMotion::PreviousStart, 1))
        ));

        // Any other key ends a pending `z` sequence.
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("j", false)),
            Some(Message::MovePreviewCursor(Motion::Down, 1))
        ));
        keymap.note(&Message::MovePreviewCursor(Motion::Down, 1));
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
            Some(Message::ScrollPreviewPage(Page::FullDown, 1))
        ));
        assert!(matches!(
            keymap.handle(named(key::Named::PageUp, Modifiers::default())),
            Some(Message::ScrollPreviewPage(Page::FullUp, 1))
        ));
        assert!(matches!(
            keymap.handle(key_press_with("d", Modifiers::CTRL, false)),
            Some(Message::ScrollPreviewPage(Page::HalfDown, 1))
        ));
        assert!(matches!(
            keymap.handle(key_press_with("u", Modifiers::CTRL, false)),
            Some(Message::ScrollPreviewPage(Page::HalfUp, 1))
        ));

        // 3Ctrl+D scrolls three half pages.
        let mut keymap = viewing();
        keymap.note(&Message::PreviewCountPressed(3));
        assert!(matches!(
            keymap.handle(key_press_with("d", Modifiers::CTRL, false)),
            Some(Message::ScrollPreviewPage(Page::HalfDown, 3))
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
            Some(Message::EditActiveComment)
        ));

        // Enter repeats do not re-trigger the popup.
        let held_enter = |repeat: bool| {
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key::Named::Enter),
                modified_key: keyboard::Key::Named(key::Named::Enter),
                physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::default(),
                text: None,
                repeat,
            }
        };
        assert!(keymap.handle(held_enter(true)).is_none());

        // While the note popup is open, Enter types a newline instead.
        let mut note = viewing();
        note.note(&Message::OpenNotePopup);
        assert!(note
            .handle(named(key::Named::Enter, Modifiers::default()))
            .is_none());

        // While find is open, Enter steps to the next match.
        let mut find = viewing();
        find.note(&Message::OpenFind);
        assert!(matches!(
            find.handle(named(key::Named::Enter, Modifiers::default())),
            Some(Message::FindNext)
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
                Some(Message::AddGlobalComment)
            ));
            assert!(matches!(
                keymap.handle(key_press_named_enter(Modifiers::CTRL | Modifiers::SHIFT, false)),
                Some(Message::PublishPressed)
            ));
        }

        // With the note popup open, the popup keeps every Ctrl combo but
        // Ctrl+S.
        let mut note = viewing();
        note.note(&Message::OpenNotePopup);
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
        keymap.note(&Message::PreviewGPressed);
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Message::PreviewZPressed)
        ));
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::PreviewGPressed)
        ));
        keymap.note(&Message::PreviewGPressed);
        // Now gg would fire — but the point is z's g did not. Arm z and
        // check the mirrored case.
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::MovePreviewJump(Jump::First, 0))
        ));
        keymap.note(&Message::MovePreviewJump(Jump::First, 0));

        // zgb: the middle g ends the z sequence, so b is just a word
        // motion — never zb's scroll.
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::PreviewGPressed)
        ));
        keymap.note(&Message::PreviewGPressed);
        assert!(matches!(
            keymap.handle(key_press("b", false)),
            Some(Message::MovePreviewWord(WordMotion::PreviousStart, 1))
        ));
        keymap.note(&Message::MovePreviewWord(WordMotion::PreviousStart, 1));

        // ztz fires normally after the fix: arm z fresh and complete it.
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("t", false)),
            Some(Message::ScrollPreviewCaret(Placement::Top))
        ));

        // A digit ends a pending prefix: z3z is two fresh presses.
        keymap.note(&Message::PreviewZPressed);
        keymap.note(&Message::PreviewCountPressed(3));
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Message::PreviewZPressed)
        ));
        keymap.note(&Message::PreviewZPressed);
        assert!(matches!(
            keymap.handle(key_press("z", false)),
            Some(Message::ScrollPreviewCaret(Placement::Center))
        ));
    }

    /// The unsaved-changes dialog swallows every key but Escape, and its
    /// messages open and close it; the mode beneath stays untouched.
    #[test]
    fn unsaved_dialog_is_a_modal() {
        let mut keymap = viewing();
        keymap.note(&Message::UnsavedChanges);
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
            Some(Message::UnsavedCancel)
        ));

        // Answering closes it too.
        keymap.note(&Message::UnsavedSave);
        assert!(!keymap.unsaved_open());
        keymap.note(&Message::UnsavedChanges);
        keymap.note(&Message::UnsavedDiscard);
        assert!(!keymap.unsaved_open());
        assert_eq!(keymap.mode(), Mode::View);
    }

    fn key_press_named_enter(modifiers: Modifiers, _repeat: bool) -> keyboard::Event {
        named(key::Named::Enter, modifiers)
    }
}
