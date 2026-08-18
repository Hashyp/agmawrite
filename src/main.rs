mod comments;
mod interactive_text;
mod preview;

use comments::{Comments, Mark};
use preview::{Caret, CaretPosition, ElementMap, Jump, Motion, WordMotion};

use iced::advanced::widget::operation::{Outcome, Scrollable};
use iced::advanced::widget::Operation;
use iced::widget::{
    button, canvas, column, container, markdown, mouse_area, operation::focus,
    operation::focus_next, operation::scroll_by, operation::AbsoluteOffset, row, scrollable, stack,
    text, text_editor, tooltip, Id, Space,
};
use iced::{
    alignment, application, keyboard, mouse, Background, Border, Color, Element, Font, Length,
    Point, Rectangle, Renderer, Subscription, Task, Theme, Vector,
};

const EDITOR_FONT: Font = Font::with_name("iA Writer Mono S");
const PREVIEW_SCROLL_ID: &str = "preview-scroll";
const PREVIEW_CARET_ID: &str = "preview-caret";
const NOTE_EDITOR_ID: &str = "note-editor";
/// Margin kept between the preview caret and the viewport edges while scrolling.
const CARET_MARGIN: f32 = 8.0;

struct Editor {
    content: text_editor::Content,
    markdown: markdown::Content,
    preview: bool,
    preview_only: bool,
    /// The preview caret: element, grapheme column, and sticky target
    /// column, owned by the preview module.
    caret: Caret,
    /// The numbered preview elements, owned by the preview module.
    preview_elements: ElementMap,
    /// Whether a lone `g` is awaiting its second key of a `gg`/`ge` sequence.
    pending_g: bool,
    /// The fixed end of the visual-mode selection, as a caret position.
    /// `None` outside visual mode.
    visual_anchor: Option<CaretPosition>,
    /// Whether the note popup is open over the preview.
    note_open: bool,
    /// The text of the note popup.
    note_text: text_editor::Content,
    /// Saved comments, the active one, and the publish draft.
    comments: Comments,
}

#[derive(Debug, Clone)]
enum Message {
    Edit(text_editor::Action),
    OpenFile,
    FileLoaded(Option<String>),
    TogglePreview,
    LinkClicked(markdown::Uri),
    MovePreviewCursor(Motion),
    MovePreviewWord(WordMotion),
    MovePreviewJump(Jump),
    PreviewGPressed,
    PreviewCancel,
    ToggleVisualMode,
    ScrollPreviewBy(f32),
    OpenNotePopup,
    CloseNotePopup,
    EditNote(text_editor::Action),
    SaveNote,
    NoteCardPressed,
    EditPublish(text_editor::Action),
    PublishPressed,
    NextComment,
}

struct OpenFileIcon;

impl<Message> canvas::Program<Message> for OpenFileIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let folder = canvas::Path::new(|path| {
            path.move_to(Point::new(2.5, 13.0));
            path.line_to(Point::new(2.5, 3.5));
            path.line_to(Point::new(6.5, 3.5));
            path.line_to(Point::new(8.5, 5.5));
            path.line_to(Point::new(13.5, 5.5));
            path.line_to(Point::new(13.5, 13.0));
            path.close();
        });

        frame.stroke(
            &folder,
            canvas::Stroke::default()
                .with_color(Color::from_rgb(0.65, 0.65, 0.65))
                .with_width(1.4)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round),
        );

        vec![frame.into_geometry()]
    }
}

struct PreviewIcon;

impl<Message> canvas::Program<Message> for PreviewIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let eye = canvas::Path::new(|path| {
            path.move_to(Point::new(1.5, 8.0));
            path.quadratic_curve_to(Point::new(8.0, 1.0), Point::new(14.5, 8.0));
            path.quadratic_curve_to(Point::new(8.0, 15.0), Point::new(1.5, 8.0));
            path.close();
        });

        frame.stroke(
            &eye,
            canvas::Stroke::default()
                .with_color(Color::from_rgb(0.65, 0.65, 0.65))
                .with_width(1.4)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round),
        );

        let pupil = canvas::Path::circle(Point::new(8.0, 8.0), 2.8);
        frame.fill(&pupil, Color::from_rgb(0.65, 0.65, 0.65));

        vec![frame.into_geometry()]
    }
}

async fn open_file() -> Option<String> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Open Markdown file")
        .pick_file()
        .await?;

    Some(String::from_utf8_lossy(&file.read().await).into_owned())
}

fn handle_key_press(
    event: keyboard::Event,
    preview: bool,
    preview_only: bool,
    pending_g: bool,
    note_open: bool,
) -> Option<Message> {
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
    if preview && note_open && !modifiers.alt() && !modifiers.logo() {
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

    if preview && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
        // Auto-repeat is welcome for motions (holding `j`/`k`/`h`/`l`, `w`,
        // `b`, `e`, or `G` keeps moving), but one-shot actions must not
        // repeat.
        return match modified_key.as_ref() {
            keyboard::Key::Named(keyboard::key::Named::ArrowUp)
            | keyboard::Key::Character("k" | "K") => Some(Message::MovePreviewCursor(Motion::Up)),
            keyboard::Key::Named(keyboard::key::Named::ArrowDown)
            | keyboard::Key::Character("j" | "J") => Some(Message::MovePreviewCursor(Motion::Down)),
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
            | keyboard::Key::Character("h" | "H") => Some(Message::MovePreviewCursor(Motion::Left)),
            keyboard::Key::Named(keyboard::key::Named::ArrowRight)
            | keyboard::Key::Character("l" | "L") => {
                Some(Message::MovePreviewCursor(Motion::Right))
            }
            // `gg` — the second `g` of the sequence is always a fresh press.
            keyboard::Key::Character("g") if pending_g && !repeat => {
                Some(Message::MovePreviewJump(Jump::First))
            }
            // The first `g` of a `gg`/`ge` sequence.
            keyboard::Key::Character("g") if !repeat => Some(Message::PreviewGPressed),
            // `ge` — the `e` of the sequence is always a fresh press.
            keyboard::Key::Character("e" | "E") if pending_g && !repeat => {
                Some(Message::MovePreviewWord(WordMotion::PreviousEnd))
            }
            keyboard::Key::Character("e" | "E") => {
                Some(Message::MovePreviewWord(WordMotion::NextEnd))
            }
            keyboard::Key::Character("w" | "W") => {
                Some(Message::MovePreviewWord(WordMotion::NextStart))
            }
            keyboard::Key::Character("b" | "B") => {
                Some(Message::MovePreviewWord(WordMotion::PreviousStart))
            }
            keyboard::Key::Character("G") => Some(Message::MovePreviewJump(Jump::Last)),
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
            keyboard::Key::Character("p" | "P") if !preview_only => Some(Message::TogglePreview),
            // Browse the saved comments in the preview.
            keyboard::Key::Character("n" | "N") if preview && !note_open => {
                Some(Message::NextComment)
            }
            _ => None,
        }
    } else {
        None
    }
}

fn subscription(editor: &Editor) -> Subscription<Message> {
    keyboard::listen()
        .with((
            editor.preview,
            editor.preview_only,
            editor.pending_g,
            editor.note_open,
        ))
        .filter_map(|((preview, preview_only, pending_g, note_open), event)| {
            handle_key_press(event, preview, preview_only, pending_g, note_open)
        })
}

fn update(editor: &mut Editor, message: Message) -> Task<Message> {
    // Any key other than a lone `g` ends a pending `gg`/`ge` sequence.
    if !matches!(message, Message::PreviewGPressed) {
        editor.pending_g = false;
    }

    match message {
        Message::Edit(action) => editor.content.perform(action),
        Message::OpenFile => return Task::perform(open_file(), Message::FileLoaded),
        Message::FileLoaded(Some(contents)) => {
            editor.preview_elements = ElementMap::parse(&contents);
            editor.caret = Caret::new();
            editor.note_open = false;
            editor.visual_anchor = None;
            editor.note_text = text_editor::Content::new();
            editor.comments = Comments::new();
            editor.content = text_editor::Content::with_text(&contents);
            editor.markdown = markdown::Content::parse(&contents);
        }
        Message::FileLoaded(None) => {}
        Message::TogglePreview => {
            if !editor.preview_only {
                editor.preview = !editor.preview;
                editor.note_open = false;
                editor.visual_anchor = None;

                if editor.preview {
                    let contents = editor.content.text();
                    editor.preview_elements = ElementMap::parse(&contents);
                    editor
                        .caret
                        .move_to_source_cursor(&editor.content, editor.preview_elements.elements());
                    editor.markdown = markdown::Content::parse(&contents);

                    return reveal_preview_caret();
                }
            }
        }
        Message::LinkClicked(_uri) => {
            // TODO: open links in the default browser
        }
        Message::MovePreviewCursor(motion) => {
            // The caret always advances; the page only scrolls as much as
            // needed to keep the caret visible.
            if editor
                .caret
                .move_by(editor.preview_elements.elements(), motion)
            {
                return reveal_preview_caret();
            }
        }
        Message::MovePreviewWord(motion) => {
            if editor
                .caret
                .move_word(editor.preview_elements.elements(), motion)
            {
                return reveal_preview_caret();
            }
        }
        Message::MovePreviewJump(jump) => {
            if editor.caret.jump(editor.preview_elements.elements(), jump) {
                return reveal_preview_caret();
            }
        }
        Message::PreviewGPressed => {
            editor.pending_g = true;
        }
        Message::PreviewCancel => {
            editor.visual_anchor = None;
        }
        Message::ToggleVisualMode => {
            editor.visual_anchor = if editor.visual_anchor.is_some() {
                None
            } else {
                Some(editor.caret.position())
            };
        }
        Message::ScrollPreviewBy(y) => {
            return scroll_by(Id::new(PREVIEW_SCROLL_ID), AbsoluteOffset { x: 0.0, y });
        }
        Message::OpenNotePopup => {
            if !editor.note_open {
                editor.note_open = true;

                return focus(Id::new(NOTE_EDITOR_ID));
            }
        }
        Message::CloseNotePopup => {
            editor.note_open = false;
        }
        Message::EditNote(action) => editor.note_text.perform(action),
        Message::EditPublish(action) => editor.comments.edit_draft(action),
        Message::PublishPressed => {
            // TODO: publish the comments
        }
        Message::NextComment => {
            if let Some(anchor) = editor.comments.cycle() {
                // Jump the caret to the comment and reveal it, so the mark
                // is actually in view.
                editor.visual_anchor = None;
                editor.caret.place(anchor);

                return reveal_preview_caret();
            }
        }
        Message::SaveNote => {
            if editor.note_open {
                save_note(editor);
            }
        }
        // Clicks on the card itself are swallowed so they neither close the
        // popup nor reach the preview beneath.
        Message::NoteCardPressed => {}
    }

    Task::none()
}

/// Saves the note popup text as a comment anchored at the preview caret,
/// then closes the popup with a fresh note. Empty notes are discarded.
fn save_note(editor: &mut Editor) {
    editor
        .comments
        .save(&editor.note_text.text(), editor.caret.position());

    editor.note_open = false;
    editor.note_text = text_editor::Content::new();
}

/// Measures the preview scrollable and the caret element in the widget tree,
/// then scrolls the minimum amount needed to bring the caret back into view.
///
/// The page keeps its position once the caret is visible — unlike a
/// proportional scroll, the caret can never outrun the end of the page.
fn reveal_preview_caret() -> Task<Message> {
    iced::advanced::widget::operate(RevealCaret {
        scroll_id: Id::new(PREVIEW_SCROLL_ID),
        caret_id: Id::new(PREVIEW_CARET_ID),
        viewport: None,
        caret: None,
    })
}

struct RevealCaret {
    scroll_id: Id,
    caret_id: Id,
    viewport: Option<(Rectangle, Vector)>,
    caret: Option<Rectangle>,
}

impl Operation<Message> for RevealCaret {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if Some(&self.scroll_id) == id {
            self.viewport = Some((bounds, translation));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if Some(&self.caret_id) == id {
            self.caret = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        let Some((viewport, translation)) = self.viewport else {
            return Outcome::None;
        };
        let Some(caret) = self.caret else {
            return Outcome::None;
        };

        // Child layouts live in unscrolled content space; the viewport shows
        // them shifted up by the current translation.
        let caret_top = caret.y - translation.y;
        let caret_bottom = caret_top + caret.height;
        let viewport_bottom = viewport.y + viewport.height;

        let delta = if caret_top < viewport.y + CARET_MARGIN {
            caret_top - viewport.y - CARET_MARGIN
        } else if caret_bottom > viewport_bottom - CARET_MARGIN {
            caret_bottom - viewport_bottom + CARET_MARGIN
        } else {
            // Already visible; keep the page where it is.
            return Outcome::None;
        };

        Outcome::Some(Message::ScrollPreviewBy(delta))
    }
}

fn editor_style(_theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::BLACK),
        border: Border::default(),
        placeholder: Color::WHITE,
        value: Color::WHITE,
        selection: Color::from_rgb(0.25, 0.25, 0.25),
    }
}

fn markdown_style() -> markdown::Style {
    markdown::Style {
        font: EDITOR_FONT,
        inline_code_font: EDITOR_FONT,
        code_block_font: EDITOR_FONT,
        ..markdown::Style::from(&Theme::Dark)
    }
}

fn icon_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => Color::from_rgb(0.7, 0.7, 0.7),
            _ => Color::WHITE,
        },
        ..Default::default()
    }
}

fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.15, 0.15, 0.15))),
        text_color: Some(Color::WHITE),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
        ..Default::default()
    }
}

struct PreviewViewer<'a> {
    /// Claims the next numbered element per rendered item from the map —
    /// the viewer never counts itself, so the numbering parity between the
    /// parse walk and the viewer lives in the preview module alone.
    claims: preview::Claims<'a>,
    focused_element: usize,
    caret_column: usize,
    /// The `(anchor, caret)` endpoints of the visual-mode selection.
    visual: Option<(CaretPosition, CaretPosition)>,
    /// Saved comments, to know which elements carry one.
    comments: &'a Comments,
}

impl<'a> markdown::Viewer<'a, Message> for PreviewViewer<'a> {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::LinkClicked(url)
    }

    fn heading(
        &self,
        mut settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text: &'a markdown::Text,
        _index: usize,
    ) -> Element<'a, Message> {
        settings.text_size = match level {
            markdown::HeadingLevel::H1 => settings.h1_size,
            markdown::HeadingLevel::H2 => settings.h2_size,
            markdown::HeadingLevel::H3 => settings.h3_size,
            markdown::HeadingLevel::H4 => settings.h4_size,
            markdown::HeadingLevel::H5 => settings.h5_size,
            markdown::HeadingLevel::H6 => settings.h6_size,
        };
        self.text_element(settings, text)
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        self.text_element(settings, text)
    }
}

impl<'a> PreviewViewer<'a> {
    fn text_element(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        // The map numbers elements exactly like the viewer numbers items;
        // if they ever disagree the item renders plainly, without caret,
        // selection, or comment mark.
        let Some((element, preview_element)) = self.claims.claim() else {
            return interactive_text::paragraph(settings, text, None, None, None, false, false);
        };

        let focused = self.focused_element == element;
        let selection = self.visual.and_then(|(anchor, caret)| {
            preview::element_selection(anchor, caret, element, preview_element.len())
        });
        let mark = self.comments.mark_for(element);
        let commented = matches!(mark, Mark::Commented | Mark::Active);
        let active_comment = mark == Mark::Active;

        interactive_text::paragraph(
            settings,
            text,
            selection,
            focused.then_some(self.caret_column),
            focused.then(|| Id::new(PREVIEW_CARET_ID)),
            commented,
            active_comment,
        )
    }
}

fn view(editor: &Editor) -> Element<'_, Message> {
    let position = editor.caret.position();
    let visual = editor.visual_anchor.map(|anchor| (anchor, position));

    let base_area: Element<'_, Message> = if editor.preview {
        scrollable(
            container(markdown::view_with(
                editor.markdown.items(),
                markdown::Settings::with_text_size(20.0, markdown_style()),
                &PreviewViewer {
                    claims: editor.preview_elements.claims(),
                    focused_element: position.element,
                    caret_column: position.column,
                    visual,
                    comments: &editor.comments,
                },
            ))
            .width(Length::Fill)
            .padding([0, 8]),
        )
        .id(Id::new(PREVIEW_SCROLL_ID))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::hidden(),
        ))
        .height(Length::Fill)
        .into()
    } else {
        text_editor(&editor.content)
            .on_action(Message::Edit)
            .font(EDITOR_FONT)
            .size(20)
            .height(Length::Fill)
            .padding(0)
            .line_height(1.8)
            .style(editor_style)
            .into()
    };

    // The note popup floats above the editing area; the backdrop closes it
    // on click and shields the area beneath from events.
    let editing_area: Element<'_, Message> = if editor.note_open {
        stack![base_area, note_popup(editor)].into()
    } else {
        base_area
    };

    let open_button = tooltip(
        button(
            canvas(OpenFileIcon)
                .width(Length::Fixed(16.0))
                .height(Length::Fixed(16.0)),
        )
        .on_press(Message::OpenFile)
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0))
        .padding(0)
        .style(icon_button_style),
        container(text("Ctrl + o, Open").font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    let preview_button = tooltip(
        button(
            canvas(PreviewIcon)
                .width(Length::Fixed(16.0))
                .height(Length::Fixed(16.0)),
        )
        .on_press(Message::TogglePreview)
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0))
        .padding(0)
        .style(icon_button_style),
        container(text("Ctrl + p, Preview").font(EDITOR_FONT).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        iced::widget::tooltip::Position::Top,
    );

    // The main column: top margin, the writing area, and the bottom
    // controls. With comments saved, the comments sidebar sits beside it
    // and spans the whole window height.
    let main = column![
        Space::new()
            .width(Length::Fill)
            .height(Length::FillPortion(1)),
        row![
            Space::new()
                .width(Length::FillPortion(1))
                .height(Length::Fill),
            container(editing_area)
                .width(Length::FillPortion(9))
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::FillPortion(8)),
        {
            let mut controls: Vec<Element<'_, Message>> = vec![
                Space::new()
                    .width(Length::FillPortion(1))
                    .height(Length::Fill)
                    .into(),
                open_button.into(),
            ];

            if !editor.preview_only {
                controls.push(preview_button.into());
            }

            controls.push(mode_badge(editor));

            controls.push(
                Space::new()
                    .width(Length::FillPortion(9))
                    .height(Length::Fill)
                    .into(),
            );

            row(controls)
                .width(Length::Fill)
                .height(Length::FillPortion(1))
                .spacing(4)
                .align_y(alignment::Vertical::Bottom)
        },
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let content: Element<'_, Message> = if editor.comments.is_empty() {
        container(main)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(background_style)
            .into()
    } else {
        container(
            row![
                container(main)
                    .width(Length::FillPortion(9))
                    .height(Length::Fill),
                container(comments_sidebar(editor))
                    .width(Length::FillPortion(2))
                    .height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(background_style)
        .into()
    };

    content
}

/// The comments sidebar: a full-height panel with a scrollable list of
/// saved comments (each quoting the Markdown source it was written for,
/// collapsed to one line and trimmed) and a free text field with a Publish
/// button at the bottom.
fn comments_sidebar<'a>(editor: &'a Editor) -> Element<'a, Message> {
    let source = editor.content.text();

    let cards: Vec<Element<'_, Message>> = editor
        .comments
        .cards(&source, editor.preview_elements.elements())
        .into_iter()
        .map(|card| {
            let active = card.active;

            container(
                column![
                    text(card.quote)
                        .font(EDITOR_FONT)
                        .size(11)
                        .color(if active {
                            Color::from_rgb(0.4, 0.85, 0.78)
                        } else {
                            Color::from_rgb(0.45, 0.45, 0.45)
                        }),
                    text(card.text.to_owned())
                        .font(EDITOR_FONT)
                        .size(13)
                        .color(Color::WHITE),
                ]
                .spacing(4)
                .width(Length::Fill),
            )
            .padding(8)
            .width(Length::Fill)
            .style(move |_theme| comment_card_style(active))
            .into()
        })
        .collect();

    container(
        column![
            container(
                text(format!("COMMENTS ({})", editor.comments.len()))
                    .font(EDITOR_FONT)
                    .size(11)
                    .color(Color::from_rgb(0.6, 0.6, 0.6)),
            )
            .padding(iced::Padding {
                top: 4.0,
                ..iced::Padding::new(0.0)
            }),
            scrollable(column(cards).spacing(8).width(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::hidden(),
                )),
            container(
                column![
                    text("Write a comment…")
                        .font(EDITOR_FONT)
                        .size(11)
                        .color(Color::from_rgb(0.5, 0.5, 0.5)),
                    text_editor(editor.comments.draft())
                        .on_action(Message::EditPublish)
                        .font(EDITOR_FONT)
                        .size(14)
                        .height(Length::Fixed(72.0))
                        .padding(6)
                        .style(publish_editor_style),
                ]
                .spacing(4)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(6)
            .style(publish_field_style),
            button(
                text("Publish")
                    .font(EDITOR_FONT)
                    .size(13)
                    .color(Color::WHITE),
            )
            .on_press(Message::PublishPressed)
            .width(Length::Fill)
            .padding([6, 12])
            .style(publish_button_style),
        ]
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(iced::Padding {
        top: 8.0,
        bottom: 8.0,
        left: 8.0,
        right: 8.0,
    })
    .style(sidebar_style)
    .into()
}

fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.03, 0.03, 0.03))),
        border: Border {
            color: Color::from_rgb(0.2, 0.2, 0.2),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn comment_card_style(active: bool) -> container::Style {
    container::Style {
        background: Some(if active {
            Background::Color(Color::from_rgba(0.3, 0.9, 0.8, 0.1))
        } else {
            Background::Color(Color::from_rgb(0.08, 0.08, 0.08))
        }),
        border: Border {
            color: if active {
                Color::from_rgba(0.3, 0.9, 0.8, 0.9)
            } else {
                Color::from_rgb(0.25, 0.25, 0.25)
            },
            width: if active { 1.5 } else { 1.0 },
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The publish text field stands out with a lighter surface than the
/// comment cards and a clearly visible border.
fn publish_field_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.17, 0.17, 0.17))),
        border: Border {
            color: Color::from_rgb(0.55, 0.55, 0.55),
            width: 1.5,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The editor inside the publish field keeps its own light surface so the
/// two nested boxes read as one input control.
fn publish_editor_style(_theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::from_rgb(0.22, 0.22, 0.22)),
        border: Border::default(),
        placeholder: Color::WHITE,
        value: Color::WHITE,
        selection: Color::from_rgb(0.35, 0.35, 0.35),
    }
}

/// The Publish button spans the sidebar width and reads as the primary
/// action of the panel.
fn publish_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(Color::from_rgb(0.25, 0.5, 1.0)))
            }
            _ => Some(Background::Color(Color::from_rgb(0.15, 0.3, 0.7))),
        },
        border: Border {
            color: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb(0.55, 0.7, 1.0)
                }
                _ => Color::from_rgb(0.35, 0.5, 0.9),
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// The bottom-bar mode indicator: which navigation mode the editor is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Editing the Markdown source.
    Write,
    /// Previewing, caret moves without selecting.
    View,
    /// Previewing, motions extend the selection.
    Visual,
}

fn editor_mode(editor: &Editor) -> Mode {
    if editor.visual_anchor.is_some() {
        Mode::Visual
    } else if editor.preview {
        Mode::View
    } else {
        Mode::Write
    }
}

/// A small badge naming the current mode, placed next to the open icon in
/// the bottom bar. Visual mode is highlighted with the selection blue so
/// the active selection state is obvious at a glance.
fn mode_badge(editor: &Editor) -> Element<'_, Message> {
    let mode = editor_mode(editor);

    let (label, color) = match mode {
        Mode::Visual => ("VISUAL", Color::from_rgb(0.4, 0.65, 1.0)),
        Mode::View => ("VIEW", Color::from_rgb(0.6, 0.6, 0.6)),
        Mode::Write => ("WRITE", Color::from_rgb(0.6, 0.6, 0.6)),
    };

    container(text(label).font(EDITOR_FONT).size(12).color(color))
        .padding([0, 8])
        .height(Length::Fixed(28.0))
        .align_y(alignment::Vertical::Center)
        .style(move |_theme| mode_badge_style(color))
        .into()
}

fn mode_badge_style(color: Color) -> container::Style {
    container::Style {
        text_color: Some(color),
        border: Border {
            color: Color { a: 0.4, ..color },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn background_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::BLACK)),
        ..Default::default()
    }
}

/// The note popup: a translucent backdrop with a centered card holding a
/// text area and a button. Clicking the backdrop closes the popup; clicks
/// on the card are swallowed.
fn note_popup(editor: &Editor) -> Element<'_, Message> {
    let card = mouse_area(
        container(
            column![
                text("Note")
                    .font(EDITOR_FONT)
                    .size(12)
                    .color(Color::from_rgb(0.6, 0.6, 0.6)),
                text_editor(&editor.note_text)
                    .id(Id::new(NOTE_EDITOR_ID))
                    .on_action(Message::EditNote)
                    .font(EDITOR_FONT)
                    .size(20)
                    .height(Length::Fixed(160.0))
                    .padding(8)
                    .style(editor_style),
                row![
                    button(
                        text("Close")
                            .font(EDITOR_FONT)
                            .size(14)
                            .color(Color::from_rgb(0.6, 0.6, 0.6)),
                    )
                    .on_press(Message::CloseNotePopup)
                    .padding([6, 12])
                    .style(popup_button_style),
                    Space::new().width(Length::Fill),
                    button(text("Save").font(EDITOR_FONT).size(14).color(Color::WHITE),)
                        .on_press(Message::SaveNote)
                        .padding([6, 12])
                        .style(popup_button_style),
                ]
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .width(Length::Fixed(440.0))
        .padding(16)
        .style(note_card_style),
    )
    .on_press(Message::NoteCardPressed);

    mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(note_backdrop_style),
    )
    .on_press(Message::CloseNotePopup)
    .into()
}

fn note_backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
        ..Default::default()
    }
}

fn note_card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.1, 0.1, 0.1))),
        border: Border {
            color: Color::from_rgb(0.3, 0.3, 0.3),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn popup_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(Color::from_rgb(0.2, 0.2, 0.2)))
            }
            _ => None,
        },
        border: Border {
            color: Color::from_rgb(0.3, 0.3, 0.3),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

struct Args {
    path: Option<String>,
    preview: bool,
}

const USAGE: &str = "Usage: agmawrite [FILE] [--preview]

Arguments:
  FILE         Path to a Markdown file to open

Options:
  --preview    Open FILE in preview-only mode; editing and switching to
               write mode are disabled
  -h, --help   Print this message";

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut args = Args {
        path: None,
        preview: false,
    };

    for arg in argv {
        match arg.as_str() {
            "--preview" => args.preview = true,
            path if !path.starts_with('-') => {
                if args.path.replace(path.to_string()).is_some() {
                    return Err("unexpected extra file argument".to_string());
                }
            }
            other => return Err(format!("unexpected argument '{other}'")),
        }
    }

    if args.preview && args.path.is_none() {
        return Err("--preview requires a FILE".to_string());
    }

    Ok(args)
}

fn boot(args: &Args) -> (Editor, Task<Message>) {
    let contents = args
        .path
        .as_ref()
        .map(|path| match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("agmawrite: cannot read '{path}': {error}");
                std::process::exit(1);
            }
        });

    let preview_elements = contents
        .as_deref()
        .map(ElementMap::parse)
        .unwrap_or_default();

    let editor = Editor {
        content: contents
            .as_deref()
            .map(text_editor::Content::with_text)
            .unwrap_or_default(),
        markdown: contents
            .as_deref()
            .map(markdown::Content::parse)
            .unwrap_or_default(),
        preview: args.preview,
        preview_only: args.preview,
        caret: Caret::new(),
        preview_elements,
        pending_g: false,
        visual_anchor: None,
        note_open: false,
        note_text: text_editor::Content::new(),
        comments: Comments::new(),
    };

    let task = if args.preview {
        Task::none()
    } else {
        focus_next()
    };

    (editor, task)
}

fn theme(_editor: &Editor) -> Theme {
    Theme::Dark
}

fn main() -> iced::Result {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    if argv.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return Ok(());
    }

    let args = match parse_args(argv) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("agmawrite: {error}\n\n{USAGE}");
            std::process::exit(1);
        }
    };

    application(move || boot(&args), update, view)
        .title("agmawrite")
        .theme(theme)
        .subscription(subscription)
        .font(include_bytes!("../fonts/iAWriterMonoS-Regular.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Italic.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Bold.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-BoldItalic.ttf").as_slice())
        .run()
}

#[cfg(test)]
mod tests {
    use super::comments::{Comments, Mark};
    use super::preview::{Caret, CaretPosition, ElementMap, Motion, WordMotion};
    use super::{editor_mode, handle_key_press, save_note, Editor, Message, Mode};
    use iced::keyboard::{self, key, Modifiers};

    fn key_pressed_with(
        character: &str,
        repeat: bool,
        pending_g: bool,
        note_open: bool,
    ) -> Option<Message> {
        let key = keyboard::Key::Character(character.into());

        handle_key_press(
            keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::default(),
                text: None,
                repeat,
            },
            true,
            false,
            pending_g,
            note_open,
        )
    }

    fn key_pressed(character: &str, repeat: bool) -> Option<Message> {
        key_pressed_with(character, repeat, false, false)
    }

    /// Holding a motion key auto-repeats the motion, like holding `j` or `k`
    /// in vim; one-shot actions and the `g`-prefix never repeat.
    #[test]
    fn held_motion_keys_repeat() {
        // Plain presses and repeats both move.
        assert!(matches!(
            key_pressed("j", false),
            Some(Message::MovePreviewCursor(Motion::Down))
        ));
        assert!(matches!(
            key_pressed("j", true),
            Some(Message::MovePreviewCursor(Motion::Down))
        ));
        assert!(matches!(
            key_pressed("k", true),
            Some(Message::MovePreviewCursor(Motion::Up))
        ));
        assert!(matches!(
            key_pressed("h", true),
            Some(Message::MovePreviewCursor(Motion::Left))
        ));
        assert!(matches!(
            key_pressed("l", true),
            Some(Message::MovePreviewCursor(Motion::Right))
        ));
        assert!(matches!(
            key_pressed("w", true),
            Some(Message::MovePreviewWord(WordMotion::NextStart))
        ));
        assert!(matches!(
            key_pressed("b", true),
            Some(Message::MovePreviewWord(WordMotion::PreviousStart))
        ));
        assert!(matches!(
            key_pressed("e", true),
            Some(Message::MovePreviewWord(WordMotion::NextEnd))
        ));

        // One-shot actions ignore repeats.
        assert!(matches!(
            key_pressed("c", false),
            Some(Message::OpenNotePopup)
        ));
        assert!(key_pressed("c", true).is_none());

        // The `g` prefix never arms or fires on repeat — `gg` and `ge` need
        // two fresh presses.
        assert!(matches!(
            key_pressed("g", false),
            Some(Message::PreviewGPressed)
        ));
        assert!(key_pressed("g", true).is_none());

        // `v` toggles visual mode on fresh presses only.
        assert!(matches!(
            key_pressed("v", false),
            Some(Message::ToggleVisualMode)
        ));
        assert!(key_pressed("v", true).is_none());
    }

    /// The bottom bar names the navigation mode: write when editing, view
    /// when previewing, and visual while a selection is anchored.
    #[test]
    fn mode_badge_reflects_editor_state() {
        let mut editor = Editor {
            content: iced::widget::text_editor::Content::new(),
            markdown: iced::widget::markdown::Content::parse(""),
            preview: false,
            preview_only: false,
            caret: Caret::new(),
            preview_elements: ElementMap::default(),
            pending_g: false,
            visual_anchor: None,
            note_open: false,
            note_text: iced::widget::text_editor::Content::new(),
            comments: Comments::new(),
        };

        assert_eq!(editor_mode(&editor), Mode::Write);

        editor.preview = true;
        assert_eq!(editor_mode(&editor), Mode::View);

        editor.visual_anchor = Some(CaretPosition {
            element: 0,
            column: 0,
        });
        assert_eq!(editor_mode(&editor), Mode::Visual);

        // Leaving visual mode returns to view.
        editor.visual_anchor = None;
        assert_eq!(editor_mode(&editor), Mode::View);
    }

    /// Saving the popup note stores a comment anchored at the caret and
    /// resets the popup; empty notes are discarded.
    #[test]
    fn saving_a_note_adds_a_comment() {
        let mut caret = Caret::new();
        caret.place(CaretPosition {
            element: 1,
            column: 2,
        });

        let mut editor = Editor {
            content: iced::widget::text_editor::Content::with_text("# Title\n\nbody"),
            markdown: iced::widget::markdown::Content::parse(""),
            preview: true,
            preview_only: false,
            caret,
            preview_elements: ElementMap::parse("# Title\n\nbody"),
            pending_g: false,
            visual_anchor: None,
            note_open: true,
            note_text: iced::widget::text_editor::Content::with_text("  fix this  \n"),
            comments: Comments::new(),
        };

        save_note(&mut editor);

        assert!(!editor.note_open);
        assert_eq!(editor.note_text.text(), "");
        assert_eq!(editor.comments.len(), 1);
        assert_eq!(
            editor
                .comments
                .cards("# Title\n\nbody", editor.preview_elements.elements())[0]
                .text,
            "fix this"
        );
        // The freshly saved comment is the active one, anchored at the caret
        // position (element 1, column 2).
        assert_eq!(editor.comments.mark_for(1), Mark::Active);
        assert_eq!(
            editor.comments.cycle(),
            Some(CaretPosition {
                element: 1,
                column: 2
            })
        );

        // An empty note only closes the popup; the first comment stays.
        editor.note_open = true;
        editor.note_text = iced::widget::text_editor::Content::with_text("   ");
        save_note(&mut editor);
        assert_eq!(editor.comments.len(), 1);
        assert!(!editor.note_open);
    }

    /// While the note popup is open, plain keys go to its text area — only
    /// Escape closes it and Ctrl+S saves the comment.
    #[test]
    fn note_popup_swallows_keys() {
        let escape = || {
            let key = keyboard::Key::Named(keyboard::key::Named::Escape);

            handle_key_press(
                keyboard::Event::KeyPressed {
                    key: key.clone(),
                    modified_key: key,
                    physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::default(),
                    text: None,
                    repeat: false,
                },
                true,
                false,
                false,
                true,
            )
        };

        assert!(matches!(escape(), Some(Message::CloseNotePopup)));

        // Motions, `c`, and typing characters produce nothing while the
        // popup is open.
        for key in ["j", "k", "h", "l", "w", "b", "e", "g", "c", "G", "x"] {
            assert!(
                key_pressed_with(key, false, false, true).is_none(),
                "'{key}' should be swallowed by the note popup"
            );
        }

        // Ctrl+S saves the note while the popup is open.
        let ctrl_s = keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("s".into()),
            modified_key: keyboard::Key::Character("s".into()),
            physical_key: key::Physical::Unidentified(key::NativeCode::Xkb(0)),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::CTRL,
            text: None,
            repeat: false,
        };

        assert!(matches!(
            handle_key_press(ctrl_s, true, false, false, true),
            Some(Message::SaveNote)
        ));
    }
}
