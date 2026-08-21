//! Temporary compatibility façade for input subscriptions and the root guard.
//!
//! Binding resolution lives in [`super::bindings`]. This copyable snapshot
//! remains only until the subscription and guard migrate to the pure router in
//! later phases.

use iced::keyboard;

use super::bindings::{route, Decision, InputMessage};
#[cfg(test)]
use super::state::Surface;
use super::state::{InteractionState, Overlay};

/// A copyable interaction snapshot used by compatibility input adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Keymap {
    interaction: InteractionState,
}

impl Keymap {
    #[cfg(test)]
    pub(crate) fn new(preview_only: bool) -> Self {
        Self::from(if preview_only {
            InteractionState::preview_only()
        } else {
            InteractionState::editable()
        })
    }

    #[cfg(test)]
    pub fn preview(&self) -> bool {
        matches!(self.interaction.view().surface(), Surface::Preview)
    }

    pub fn note_open(&self) -> bool {
        self.interaction.view().contains(Overlay::Note)
    }

    pub fn find_open(&self) -> bool {
        self.interaction.view().contains(Overlay::Find)
    }

    pub fn help_open(&self) -> bool {
        matches!(self.interaction.view().overlay(), Some(Overlay::Help))
    }

    pub fn unsaved_open(&self) -> bool {
        self.interaction.view().unsaved_action().is_some()
    }

    /// Delegates a keyboard subscription event to the pure binding router.
    pub(crate) fn handle(&self, event: keyboard::Event) -> Option<InputMessage> {
        match route(&self.interaction, &iced::Event::Keyboard(event)) {
            Decision::Dispatch(message) => Some(message),
            Decision::Pass | Decision::Capture => None,
        }
    }
}

impl From<InteractionState> for Keymap {
    fn from(interaction: InteractionState) -> Self {
        Self { interaction }
    }
}
