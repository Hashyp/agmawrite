//! The shortcuts Help window: its query state, shortcut vocabulary,
//! filtering, and rendering.
//!
//! The application owns where Help sits in the root stack and where focus
//! returns when it closes. `InteractionState` owns whether the modal is visible.

use iced::widget::{
    column, container, mouse_area, operation::focus, row, scrollable, text, text_input, Id,
};
use iced::{alignment, Element, Font, Length, Task};

use crate::editing;
use crate::theme::Palette;
use crate::ui::modal;

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
const INPUT_ID: &str = "help-input";

/// State owned by the Help window.
#[derive(Debug, Default)]
pub struct Help {
    query: String,
}

impl Help {
    pub fn new() -> Self {
        Self::default()
    }

    /// The live shortcut filter.
    fn query(&self) -> &str {
        &self.query
    }

    /// Applies a Help-window message. Closing clears the query so the next
    /// visit always starts with the full shortcut list.
    pub fn update(&mut self, message: Message) {
        match message {
            Message::QueryChanged(query) => self.query = query,
            Message::Close => self.query.clear(),
            Message::Open | Message::CardPressed => {}
        }
    }
}

/// Events produced by widgets inside the Help window.
#[derive(Debug, Clone)]
pub enum Message {
    Open,
    QueryChanged(String),
    CardPressed,
    Close,
}

/// Gives focus to the Help search field after the root opens the window.
pub fn focus_input<T>() -> Task<T> {
    focus(Id::new(INPUT_ID))
}

/// Every keyboard shortcut handled by the application. Keeping the
/// vocabulary beside the window prevents its displayed behavior from
/// drifting away from its filter and rendering.
const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl + ?", "Open this shortcuts help window."),
    (
        "Esc",
        "Close help/popups, leave visual mode, or unfocus a text field.",
    ),
    ("Ctrl + O", "Open a Markdown file."),
    ("Ctrl + P", "Toggle between writing and preview mode."),
    (
        "Ctrl + S",
        "Save the document; save the note when its popup is open.",
    ),
    (
        "j / k",
        "Move the focused button while the unsaved-changes dialog is open.",
    ),
    (
        "Enter",
        "In the unsaved-changes dialog: save the document and proceed.",
    ),
    ("Ctrl + F", "Open find in the document."),
    (
        "/",
        "Open find from the preview canvas (view or visual mode).",
    ),
    ("Ctrl + G", "Find the next match while find is open."),
    ("Ctrl + B", "Show or hide the comments sidebar."),
    ("Ctrl + N", "Jump to the next saved comment in preview."),
    (
        "Ctrl + A",
        "Select all text in the focused editor or field.",
    ),
    ("Ctrl + C", "Copy selected text."),
    ("Ctrl + X", "Cut selected text."),
    ("Ctrl + V", "Paste text."),
    (
        "Arrow keys",
        "Move the focused text cursor, or the caret in preview.",
    ),
    (
        "Shift + movement keys",
        "Extend a focused selection while moving.",
    ),
    (
        "Ctrl + ← / →",
        "Move by words in the focused editor or field.",
    ),
    (
        "Ctrl + Shift + ← / →",
        "Extend the focused selection by words.",
    ),
    ("Home / End", "Move to the start or end of a line or field."),
    (
        "Ctrl + Home / End",
        "Move to the start or end of the source document.",
    ),
    ("Page Up / Page Down", "Move by pages in the source editor."),
    (
        "Backspace / Delete",
        "Delete text before or after the focused cursor.",
    ),
    (
        "h/H · j/J · k/K · l/L",
        "Move the preview caret left, down, up, or right.",
    ),
    ("w/W", "Move to the next word start in preview."),
    ("b/B", "Move to the previous word start in preview."),
    ("e/E", "Move to the next word end in preview."),
    ("0", "Move to the start of the preview element."),
    (
        "1-9 then motion",
        "Repeat a preview motion: 3j, 10k, 2h, 3l, 3w, 5G, 3gg.",
    ),
    (
        "Ctrl + D / Ctrl + U",
        "Scroll the preview half a page down or up.",
    ),
    ("Page Up / Page Down", "Scroll the preview a full page."),
    (
        "z then z/t/b",
        "Center, top, or bottom the preview around the caret.",
    ),
    ("g then e/E", "Move to the previous word end in preview."),
    ("g then g", "Jump to the first preview element."),
    ("G", "Jump to the last preview element."),
    ("v/V", "Toggle visual selection mode in preview."),
    ("c/C", "Open a note popup at the preview caret."),
    (
        "Enter",
        "Insert a line break, or find the next match while find is open.",
    ),
    (
        "Shift + Enter",
        "Find the previous match while find is open.",
    ),
    ("Enter (preview)", "Open the active comment for editing."),
    ("Ctrl + Enter", "Add the sidebar draft as a global comment."),
    (
        "Ctrl + Shift + Enter",
        "Trigger the sidebar's Publish button.",
    ),
];

/// Shortcut rows matching `query` over either column,
/// ASCII-case-insensitively like document find.
fn matches(query: &str) -> Vec<(&'static str, &'static str)> {
    SHORTCUTS
        .iter()
        .filter(|(shortcut, description)| {
            query.is_empty()
                || !editing::byte_matches(shortcut, query).is_empty()
                || !editing::byte_matches(description, query).is_empty()
        })
        .copied()
        .collect()
}

/// Renders the complete centered Help window, including its modal
/// backdrop, search field, filtered rows, and click policy.
pub fn view(help: &Help, palette: Palette) -> Element<'static, Message> {
    let matches = matches(help.query());
    let list: Element<'static, Message> = if matches.is_empty() {
        text("No matching shortcuts")
            .font(EDITOR_FONT)
            .size(13)
            .color(palette.dark_foreground)
            .into()
    } else {
        let rows: Vec<Element<'static, Message>> = matches
            .into_iter()
            .map(|(shortcut, description)| {
                row![
                    container(
                        text(shortcut)
                            .font(EDITOR_FONT)
                            .size(13)
                            .color(palette.foreground)
                    )
                    .width(Length::Fixed(240.0)),
                    text(description)
                        .font(EDITOR_FONT)
                        .size(13)
                        .color(palette.light_foreground),
                ]
                .spacing(12)
                .align_y(alignment::Vertical::Center)
                .width(Length::Fill)
                .into()
            })
            .collect();

        column(rows).spacing(9).width(Length::Fill).into()
    };

    let card = mouse_area(
        container(
            column![
                text("Keyboard shortcuts")
                    .font(EDITOR_FONT)
                    .size(16)
                    .color(palette.foreground),
                text("Esc or Ctrl + ? closes")
                    .font(EDITOR_FONT)
                    .size(12)
                    .color(palette.dark_foreground),
                text_input("Search shortcuts…", help.query())
                    .id(Id::new(INPUT_ID))
                    .on_input(Message::QueryChanged)
                    .font(EDITOR_FONT)
                    .size(13)
                    .padding(6),
                scrollable(list).height(Length::Fixed(440.0)),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .width(Length::Fixed(680.0))
        .padding(20)
        .style(move |_theme| modal::card(&palette)),
    )
    .on_press(Message::CardPressed);

    mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_theme| modal::backdrop(&palette)),
    )
    .on_press(Message::Close)
    .on_scroll(|_| Message::CardPressed)
    .into()
}

#[cfg(test)]
mod tests {
    use super::{matches, Help, Message, SHORTCUTS};

    #[test]
    fn state_filters_and_clears_on_close() {
        let mut help = Help::new();
        help.update(Message::QueryChanged("gg".to_owned()));
        assert_eq!(help.query(), "gg");

        help.update(Message::Close);
        assert_eq!(help.query(), "");
    }

    #[test]
    fn shortcut_vocabulary_includes_standard_editing_commands() {
        for shortcut in ["Ctrl + A", "Ctrl + C", "Ctrl + X", "Ctrl + V"] {
            assert!(SHORTCUTS.iter().any(|(key, _)| *key == shortcut));
        }
        assert_eq!(SHORTCUTS[0].0, "Ctrl + ?");
    }

    #[test]
    fn search_matches_both_columns_case_insensitively() {
        assert_eq!(matches("").len(), SHORTCUTS.len());

        for query in ["preview", "PREVIEW", "Ctrl"] {
            assert!(!matches(query).is_empty());
        }
        assert!(matches("sidebar")
            .iter()
            .all(|(_, description)| description.to_lowercase().contains("sidebar")));
        assert!(matches("page up")
            .iter()
            .any(|(key, _)| key.to_lowercase().contains("page up")));
        assert!(matches("no shortcut has this text").is_empty());
    }
}
