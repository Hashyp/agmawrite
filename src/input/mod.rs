//! Root keyboard input and interaction ownership.
//!
//! [`state`] owns the legal interaction hierarchy, state changes, focus, and
//! read-only UI projections. [`pending`] owns preview counts and multi-key
//! sequence parsing. [`bindings`] is the pure event router: it reads a state
//! snapshot and returns either pass, capture, or an input message. [`guard`]
//! adapts that decision to Iced's root widget without adding policy.
//! [`command`] owns only semantic user intent; the application composition
//! root maps those commands to feature messages and performs the state change.

mod bindings;
mod command;
mod guard;
mod pending;
mod state;

pub(crate) use bindings::InputMessage;
pub(crate) use command::{
    Command, CommentsCommand, DocumentCommand, FindCommand, HelpCommand, PreviewCommand,
};
pub(crate) use guard::guard;
pub(crate) use state::{
    FocusTarget, InteractionState, ModeBadge, Overlay, PreviewToggle, Surface, ToolbarPresentation,
};
