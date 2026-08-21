//! Root keyboard-event interception without a dependency on app messages.

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{layout, renderer, Clipboard, Layout, Shell, Widget};
use iced::{keyboard, mouse, Element, Length, Rectangle, Renderer, Size, Theme, Vector};

use super::Keymap;
use crate::help;

/// An input-local consequence of intercepting a root event.
///
/// The app maps dispatch actions to its own message vocabulary. `Pass` and
/// `Capture` are handled entirely inside this widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardAction {
    Pass,
    Capture,
    OpenHelp,
    CloseHelp,
    CloseFind,
    CloseNote,
    CancelUnsaved,
}

/// Applies the modal keyboard policy in strict top-to-bottom priority order.
fn action(keymap: Keymap, event: &iced::Event) -> GuardAction {
    // 1. Help opens, closes, or captures before every modal beneath it.
    match help::event_action(keymap.help_open(), event) {
        help::EventAction::Open => return GuardAction::OpenHelp,
        help::EventAction::Close => return GuardAction::CloseHelp,
        help::EventAction::Capture => return GuardAction::Capture,
        help::EventAction::Pass => {}
    }

    // 2. Passed events belong to Help's focused search field while open.
    if keymap.help_open() {
        return GuardAction::Pass;
    }

    // 3. Unsaved changes blocks input-method commits and all key presses;
    // Escape is the only key with a semantic action.
    if keymap.unsaved_open() && matches!(event, iced::Event::InputMethod(_)) {
        return GuardAction::Capture;
    }

    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        modifiers,
        repeat,
        ..
    }) = event
    else {
        return GuardAction::Pass;
    };

    if keymap.unsaved_open() {
        return if !repeat
            && matches!(
                modified_key.as_ref(),
                keyboard::Key::Named(keyboard::key::Named::Escape)
            ) {
            GuardAction::CancelUnsaved
        } else {
            GuardAction::Capture
        };
    }

    let plain_escape = !repeat
        && !modifiers.control()
        && !modifiers.alt()
        && !modifiers.logo()
        && matches!(
            modified_key.as_ref(),
            keyboard::Key::Named(keyboard::key::Named::Escape)
        );

    if plain_escape {
        // 4. Find closes before the note composer when both are present.
        if keymap.find_open() {
            return GuardAction::CloseFind;
        }

        // 5. Note closes after Find and before focused widgets see Escape.
        if keymap.note_open() {
            return GuardAction::CloseNote;
        }
    }

    // 6. Everything else reaches the focused widget unchanged.
    GuardAction::Pass
}

/// Wraps the complete interface with one input guard.
///
/// `map_action` is supplied by the composition root, keeping this package
/// generic over the app's message type and dependency-free from it.
pub(crate) fn guard<'a, Message>(
    content: Element<'a, Message>,
    keymap: Keymap,
    map_action: impl Fn(GuardAction) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: 'a,
{
    struct KeyboardGuard<'a, Message, Map> {
        content: Element<'a, Message>,
        keymap: Keymap,
        map_action: Map,
    }

    impl<Message, Map> Widget<Message, Theme, Renderer> for KeyboardGuard<'_, Message, Map>
    where
        Map: Fn(GuardAction) -> Message,
    {
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
            match action(self.keymap, event) {
                GuardAction::Pass => {
                    self.content.as_widget_mut().update(
                        tree, event, layout, cursor, renderer, clipboard, shell, viewport,
                    );
                    return;
                }
                GuardAction::Capture => {}
                action => shell.publish((self.map_action)(action)),
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

    Element::new(KeyboardGuard {
        content,
        keymap,
        map_action,
    })
}

#[cfg(test)]
mod tests {
    use super::{action, GuardAction};
    use crate::document::UnsavedAction;
    use crate::input::{Keymap, Transition};
    use iced::keyboard::{self, key, Location, Modifiers};

    fn pressed(key: keyboard::Key, modifiers: Modifiers) -> iced::Event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    fn escape() -> iced::Event {
        pressed(
            keyboard::Key::Named(key::Named::Escape),
            Modifiers::default(),
        )
    }

    #[test]
    fn help_guard_routes_keys_to_the_help_window() {
        let write = Keymap::new(false);
        let question = pressed(keyboard::Key::Character("?".into()), Modifiers::SHIFT);
        assert_eq!(action(write, &question), GuardAction::Pass);

        let chord = pressed(
            keyboard::Key::Character("?".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
        );
        let slash_chord = pressed(keyboard::Key::Character("/".into()), Modifiers::CTRL);
        assert_eq!(action(write, &chord), GuardAction::OpenHelp);
        assert_eq!(action(write, &slash_chord), GuardAction::OpenHelp);

        let mut help = write;
        help.note(Transition::HelpOpened);
        let typing = pressed(keyboard::Key::Character("x".into()), Modifiers::default());
        assert_eq!(action(help, &typing), GuardAction::Pass);
        assert_eq!(action(help, &chord), GuardAction::CloseHelp);
        assert_eq!(
            action(
                help,
                &iced::Event::InputMethod(iced::advanced::input_method::Event::Closed)
            ),
            GuardAction::Pass
        );

        let tab = pressed(keyboard::Key::Named(key::Named::Tab), Modifiers::default());
        assert_eq!(action(help, &tab), GuardAction::Capture);
        assert_eq!(action(help, &escape()), GuardAction::CloseHelp);
    }

    #[test]
    fn help_over_unsaved_over_find_over_note_unwinds_to_note() {
        let mut keymap = Keymap::new(false);
        keymap.note(Transition::PreviewToggled);
        keymap.note(Transition::NoteOpened);
        keymap.note(Transition::FindOpened);
        keymap.note(Transition::UnsavedOpened(UnsavedAction::OpenFile));
        keymap.note(Transition::HelpOpened);

        assert!(keymap.preview());
        assert!(keymap.note_open());
        assert!(keymap.find_open());
        assert!(keymap.unsaved_open());
        assert!(keymap.help_open());
        assert_eq!(action(keymap, &escape()), GuardAction::CloseHelp);

        keymap.note(Transition::HelpClosed);
        assert_eq!(action(keymap, &escape()), GuardAction::CancelUnsaved);

        keymap.note(Transition::UnsavedClosed);
        assert_eq!(action(keymap, &escape()), GuardAction::CloseFind);

        keymap.note(Transition::FindClosed);
        assert!(keymap.note_open());
        assert_eq!(action(keymap, &escape()), GuardAction::CloseNote);
    }

    #[test]
    fn modal_priority_is_help_unsaved_find_note_then_focused_widgets() {
        let mut keymap = Keymap::new(false);
        keymap.note(Transition::PreviewToggled);
        keymap.note(Transition::NoteOpened);
        keymap.note(Transition::FindOpened);
        keymap.note(Transition::UnsavedOpened(UnsavedAction::OpenFile));
        keymap.note(Transition::HelpOpened);

        // Help has first refusal even when every lower modal is open, and
        // its field receives passed input-method events.
        assert_eq!(action(keymap, &escape()), GuardAction::CloseHelp);
        assert_eq!(
            action(
                keymap,
                &iced::Event::InputMethod(iced::advanced::input_method::Event::Closed)
            ),
            GuardAction::Pass
        );

        keymap.note(Transition::HelpClosed);
        assert_eq!(action(keymap, &escape()), GuardAction::CancelUnsaved);
        assert_eq!(
            action(
                keymap,
                &iced::Event::InputMethod(iced::advanced::input_method::Event::Closed)
            ),
            GuardAction::Capture
        );
        assert_eq!(
            action(
                keymap,
                &pressed(keyboard::Key::Character("x".into()), Modifiers::default())
            ),
            GuardAction::Capture
        );

        keymap.note(Transition::UnsavedClosed);
        assert_eq!(action(keymap, &escape()), GuardAction::CloseFind);

        keymap.note(Transition::FindClosed);
        assert_eq!(action(keymap, &escape()), GuardAction::CloseNote);

        keymap.note(Transition::NoteClosed);
        assert_eq!(action(keymap, &escape()), GuardAction::Pass);
        assert_eq!(
            action(
                keymap,
                &pressed(keyboard::Key::Character("x".into()), Modifiers::default())
            ),
            GuardAction::Pass
        );
    }
}
