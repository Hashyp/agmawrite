//! Thin Iced widget adaptation for the pure root event router.

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{layout, renderer, Clipboard, Layout, Shell, Widget};
use iced::{mouse, Element, Length, Rectangle, Renderer, Size, Theme, Vector};

use super::bindings::{route, Decision, InputMessage};
use super::InteractionState;

/// Applies one routing decision at the widget boundary.
fn adapt<'shell, Message>(
    decision: Decision,
    map_message: &impl Fn(InputMessage) -> Message,
    shell: &mut Shell<'shell, Message>,
    pass_to_child: impl FnOnce(&mut Shell<'shell, Message>),
) {
    match decision {
        Decision::Pass => pass_to_child(shell),
        Decision::Capture => shell.capture_event(),
        Decision::Dispatch(message) => {
            shell.publish(map_message(message));
            shell.capture_event();
        }
    }
}

/// Routes an event exactly once, then applies that decision exactly once.
fn intercept<'shell, Message>(
    interaction: &InteractionState,
    event: &iced::Event,
    map_message: &impl Fn(InputMessage) -> Message,
    shell: &mut Shell<'shell, Message>,
    pass_to_child: impl FnOnce(&mut Shell<'shell, Message>),
) {
    adapt(route(interaction, event), map_message, shell, pass_to_child);
}

/// Wraps the complete interface with the single root input router.
pub(crate) fn guard<'a, Message>(
    content: Element<'a, Message>,
    interaction: InteractionState,
    map_message: impl Fn(InputMessage) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: 'a,
{
    struct InputGuard<'a, Message, Map> {
        content: Element<'a, Message>,
        interaction: InteractionState,
        map_message: Map,
    }

    impl<Message, Map> Widget<Message, Theme, Renderer> for InputGuard<'_, Message, Map>
    where
        Map: Fn(InputMessage) -> Message,
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
            let Self {
                content,
                interaction,
                map_message,
            } = self;
            intercept(interaction, event, map_message, shell, |shell| {
                content.as_widget_mut().update(
                    tree, event, layout, cursor, renderer, clipboard, shell, viewport,
                );
            });
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

    Element::new(InputGuard {
        content,
        interaction,
        map_message,
    })
}

#[cfg(test)]
mod tests {
    use super::{adapt, intercept, Decision, InputMessage};
    use crate::document::UnsavedAction;
    use crate::input::{
        Command, CommentsCommand, DocumentCommand, FindCommand, HelpCommand, InteractionState,
        PreviewCommand,
    };
    use crate::preview::Motion;
    use iced::advanced::Shell;
    use iced::keyboard::{self, key, Location, Modifiers};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestMessage {
        Child,
        Routed(InputMessage),
    }

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

    fn character(value: &str, modifiers: Modifiers, repeat: bool) -> iced::Event {
        pressed(keyboard::Key::Character(value.into()), modifiers, repeat)
    }

    fn escape() -> iced::Event {
        pressed(
            keyboard::Key::Named(key::Named::Escape),
            Modifiers::default(),
            false,
        )
    }

    fn input_method() -> iced::Event {
        iced::Event::InputMethod(iced::advanced::input_method::Event::Closed)
    }

    fn outcome(state: &InteractionState, event: &iced::Event) -> (Vec<TestMessage>, bool, usize) {
        let mut messages = Vec::new();
        let mut child_updates = 0;
        let captured;
        {
            let mut shell = Shell::new(&mut messages);
            intercept(state, event, &TestMessage::Routed, &mut shell, |shell| {
                child_updates += 1;
                shell.publish(TestMessage::Child);
            });
            captured = shell.is_event_captured();
        }
        (messages, captured, child_updates)
    }

    #[test]
    fn adapter_passes_captures_and_dispatches_exactly_once() {
        let routed = InputMessage::Execute(Command::Help(HelpCommand::Open));
        let cases = [
            ("pass", Decision::Pass, vec![TestMessage::Child], false, 1),
            ("capture", Decision::Capture, vec![], true, 0),
            (
                "dispatch",
                Decision::Dispatch(routed),
                vec![TestMessage::Routed(routed)],
                true,
                0,
            ),
        ];

        for (name, decision, expected_messages, expected_capture, expected_child_updates) in cases {
            let mut messages = Vec::new();
            let mut child_updates = 0;
            let captured;
            {
                let mut shell = Shell::new(&mut messages);
                adapt(decision, &TestMessage::Routed, &mut shell, |shell| {
                    child_updates += 1;
                    shell.publish(TestMessage::Child);
                });
                captured = shell.is_event_captured();
            }

            assert_eq!(messages, expected_messages, "{name}: messages");
            assert_eq!(captured, expected_capture, "{name}: capture");
            assert_eq!(
                child_updates, expected_child_updates,
                "{name}: child updates"
            );
        }
    }

    #[test]
    fn focused_text_and_input_method_events_reach_the_child() {
        let write = InteractionState::editable();
        let mut note = InteractionState::preview_only();
        note.open_note().unwrap();
        let mut find = InteractionState::editable();
        find.open_find();
        let mut help = InteractionState::editable();
        help.open_help();

        for (name, state) in [
            ("write", write),
            ("note", note),
            ("find", find),
            ("help", help),
        ] {
            assert_eq!(
                outcome(&state, &character("x", Modifiers::default(), false)),
                (vec![TestMessage::Child], false, 1),
                "{name}: text"
            );
            assert_eq!(
                outcome(&state, &input_method()),
                (vec![TestMessage::Child], false, 1),
                "{name}: input method"
            );
        }
    }

    #[test]
    fn modal_priority_is_routed_once_then_captured_by_the_adapter() {
        let mut state = InteractionState::preview_only();
        state.open_note().unwrap();
        state.open_find();
        state.open_unsaved(UnsavedAction::OpenFile);
        state.open_help();

        assert_eq!(
            outcome(&state, &escape()),
            (
                vec![TestMessage::Routed(InputMessage::Execute(Command::Help(
                    HelpCommand::Close,
                )))],
                true,
                0,
            )
        );

        let _ = state.close_help();
        assert_eq!(
            outcome(&state, &escape()),
            (
                vec![TestMessage::Routed(InputMessage::Execute(
                    Command::Document(DocumentCommand::CancelUnsaved),
                ))],
                true,
                0,
            )
        );
        assert_eq!(outcome(&state, &input_method()), (vec![], true, 0));

        let _ = state.resolve_unsaved();
        assert_eq!(
            outcome(&state, &escape()),
            (
                vec![TestMessage::Routed(InputMessage::Execute(Command::Find(
                    FindCommand::Close,
                )))],
                true,
                0,
            )
        );

        let _ = state.close_find();
        assert_eq!(
            outcome(&state, &escape()),
            (
                vec![TestMessage::Routed(InputMessage::Execute(
                    Command::Comments(CommentsCommand::CloseNote),
                ))],
                true,
                0,
            )
        );
    }

    #[test]
    fn held_keys_repeat_motions_but_suppress_one_shots_without_duplicates() {
        let preview = InteractionState::preview_only();

        assert_eq!(
            outcome(&preview, &character("j", Modifiers::default(), true)),
            (
                vec![TestMessage::Routed(InputMessage::Execute(
                    Command::Preview(PreviewCommand::Move(Motion::Down, 1)),
                ))],
                true,
                0,
            )
        );
        assert_eq!(
            outcome(&preview, &character("c", Modifiers::default(), true)),
            (vec![], true, 0)
        );
        assert_eq!(
            outcome(&preview, &character("c", Modifiers::default(), false)),
            (
                vec![TestMessage::Routed(InputMessage::Execute(
                    Command::Comments(CommentsCommand::OpenNote),
                ))],
                true,
                0,
            )
        );
        assert_eq!(
            outcome(&preview, &character("3", Modifiers::default(), true)),
            (vec![], true, 0)
        );
    }
}
