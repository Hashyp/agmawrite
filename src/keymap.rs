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

use crate::Message;

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
    preview_only: bool,
    /// Whether a lone `g` is awaiting its second key of a `gg`/`ge`
    /// sequence.
    pending_g: bool,
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
            preview_only,
            pending_g: false,
        }
    }

    /// The current mode, note popup included — the badge reads this.
    pub fn mode(&self) -> Mode {
        if self.note_open {
            Mode::Note
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

        if self.preview() && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            // Auto-repeat is welcome for motions (holding `j`/`k`/`h`/`l`,
            // `w`, `b`, `e`, or `G` keeps moving), but one-shot actions
            // must not repeat.
            return match modified_key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::ArrowUp)
                | keyboard::Key::Character("k" | "K") => {
                    Some(Message::MovePreviewCursor(crate::preview::Motion::Up))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowDown)
                | keyboard::Key::Character("j" | "J") => {
                    Some(Message::MovePreviewCursor(crate::preview::Motion::Down))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
                | keyboard::Key::Character("h" | "H") => {
                    Some(Message::MovePreviewCursor(crate::preview::Motion::Left))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowRight)
                | keyboard::Key::Character("l" | "L") => {
                    Some(Message::MovePreviewCursor(crate::preview::Motion::Right))
                }
                // `gg` — the second `g` of the sequence is always a fresh
                // press.
                keyboard::Key::Character("g") if self.pending_g && !repeat => {
                    Some(Message::MovePreviewJump(crate::preview::Jump::First))
                }
                // The first `g` of a `gg`/`ge` sequence.
                keyboard::Key::Character("g") if !repeat => Some(Message::PreviewGPressed),
                // `ge` — the `e` of the sequence is always a fresh press.
                keyboard::Key::Character("e" | "E") if self.pending_g && !repeat => Some(
                    Message::MovePreviewWord(crate::preview::WordMotion::PreviousEnd),
                ),
                keyboard::Key::Character("e" | "E") => Some(Message::MovePreviewWord(
                    crate::preview::WordMotion::NextEnd,
                )),
                keyboard::Key::Character("w" | "W") => Some(Message::MovePreviewWord(
                    crate::preview::WordMotion::NextStart,
                )),
                keyboard::Key::Character("b" | "B") => Some(Message::MovePreviewWord(
                    crate::preview::WordMotion::PreviousStart,
                )),
                keyboard::Key::Character("G") => {
                    Some(Message::MovePreviewJump(crate::preview::Jump::Last))
                }
                keyboard::Key::Character("v" | "V") if !repeat => Some(Message::ToggleVisualMode),
                keyboard::Key::Character("c" | "C") if !repeat => Some(Message::OpenNotePopup),
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
                // Browse the saved comments in the preview.
                keyboard::Key::Character("n" | "N") if self.preview() => Some(Message::NextComment),
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
        // Any message other than arming the prefix ends a pending `gg`/`ge`
        // sequence, like any other key would.
        self.pending_g = matches!(message, Message::PreviewGPressed);

        match message {
            Message::FileLoaded(Some(_)) => {
                self.note_open = false;

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
    use crate::preview::{Jump, Motion, WordMotion};
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
            Some(Message::MovePreviewCursor(Motion::Down))
        ));
        assert!(matches!(
            pressed("j", true),
            Some(Message::MovePreviewCursor(Motion::Down))
        ));
        assert!(matches!(
            pressed("k", true),
            Some(Message::MovePreviewCursor(Motion::Up))
        ));
        assert!(matches!(
            pressed("h", true),
            Some(Message::MovePreviewCursor(Motion::Left))
        ));
        assert!(matches!(
            pressed("l", true),
            Some(Message::MovePreviewCursor(Motion::Right))
        ));
        assert!(matches!(
            pressed("w", true),
            Some(Message::MovePreviewWord(WordMotion::NextStart))
        ));
        assert!(matches!(
            pressed("b", true),
            Some(Message::MovePreviewWord(WordMotion::PreviousStart))
        ));
        assert!(matches!(
            pressed("e", true),
            Some(Message::MovePreviewWord(WordMotion::NextEnd))
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
            Some(Message::MovePreviewJump(Jump::First))
        ));

        // The sequence fired; a third `g` starts over.
        keymap.note(&Message::MovePreviewJump(Jump::First));
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::PreviewGPressed)
        ));

        // Any other key ends the pending sequence.
        keymap.note(&Message::PreviewGPressed);
        assert!(matches!(
            keymap.handle(key_press("j", false)),
            Some(Message::MovePreviewCursor(Motion::Down))
        ));
        keymap.note(&Message::MovePreviewCursor(Motion::Down));
        assert!(matches!(
            keymap.handle(key_press("g", false)),
            Some(Message::PreviewGPressed)
        ));

        // `ge` moves to the end of the previous word.
        keymap.note(&Message::PreviewGPressed);
        assert!(matches!(
            keymap.handle(key_press("e", false)),
            Some(Message::MovePreviewWord(WordMotion::PreviousEnd))
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
}
