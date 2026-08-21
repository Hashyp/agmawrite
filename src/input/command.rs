//! Semantic commands produced by keyboard input.
//!
//! Commands describe user intent by feature domain. The application maps
//! them to its current root message vocabulary at the composition boundary.

use crate::preview::{Jump, Motion, Page, Placement, WordMotion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Document(DocumentCommand),
    Preview(PreviewCommand),
    Comments(CommentsCommand),
    Find(FindCommand),
    Help(HelpCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentCommand {
    Open,
    Save,
    CancelUnsaved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCommand {
    Toggle,
    Move(Motion, usize),
    MoveWord(WordMotion, usize),
    Jump(Jump, usize),
    Cancel,
    ToggleVisual,
    ScrollPage(Page, usize),
    ScrollCaret(Placement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentsCommand {
    OpenNote,
    CloseNote,
    SaveNote,
    EditActive,
    Next,
    ToggleSidebar,
    AddGlobal,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindCommand {
    Open,
    Close,
    Next,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpCommand {
    Open,
    Close,
}
