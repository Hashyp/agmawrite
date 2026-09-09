//! The status bar's action protocol: the document and surface controls it
//! renders as text. The controls live in [`super::status_bar`]; this module
//! owns only the messages they emit, so the bar composes an existing
//! protocol instead of absorbing it.

/// Actions produced by the status bar's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Message {
    Open,
    Save,
    TogglePreview,
}
