//! Markdown preview rendering and its scrollable surface.
//!
//! The viewer owns the correspondence between parsed Markdown items and the
//! preview model's numbered elements. The app supplies only read-only comment,
//! find, and palette collaborators and maps the resulting preview-local
//! messages at the composition boundary.

#[cfg(test)]
mod layout_tests;

use std::ops::Range;

use super::decorations::{self, Config, Pipeline};
use super::scroll::{anchor_id, caret_id, scrollable_id};
use super::{Message, PreviewElement, State};
use crate::{comments, interactive_text, theme::Palette};

use iced::widget::{checkbox, column, container, markdown, rich_text, row, rule, scrollable, text};
use iced::{border, Background, Color, Element, Font, Length};

const PREVIEW_FONT: Font = Font::with_name("iA Writer Mono S");
const MIN_HEADING_CONTRAST: f32 = 4.5;
const CONTRAST_SEARCH_STEPS: u8 = 32;

/// Resolves a heading level to an active-theme role, softened toward the
/// theme's foreground. If that tint is too faint on the current background,
/// progressively less of the role is used until ordinary-text contrast holds.
fn heading_color(level: &markdown::HeadingLevel, palette: &Palette) -> Color {
    let (role, strength) = match level {
        markdown::HeadingLevel::H1 => (palette.magenta, 0.65),
        markdown::HeadingLevel::H2 => (palette.blue, 0.60),
        markdown::HeadingLevel::H3 => (palette.cyan, 0.55),
        markdown::HeadingLevel::H4 => (palette.green, 0.45),
        markdown::HeadingLevel::H5 => (palette.yellow, 0.30),
        markdown::HeadingLevel::H6 => (palette.magenta, 0.12),
    };

    for step in 0..=CONTRAST_SEARCH_STEPS {
        let role_weight = strength * (1.0 - f32::from(step) / f32::from(CONTRAST_SEARCH_STEPS));
        let candidate = mix(role, palette.foreground, role_weight);

        if candidate.relative_contrast(palette.background) >= MIN_HEADING_CONTRAST {
            return candidate;
        }
    }

    palette.foreground
}

/// Mixes `role_weight` of a semantic role with the active theme foreground.
fn mix(role: Color, foreground: Color, role_weight: f32) -> Color {
    let foreground_weight = 1.0 - role_weight;

    Color::from_rgb(
        role.r * role_weight + foreground.r * foreground_weight,
        role.g * role_weight + foreground.g * foreground_weight,
        role.b * role_weight + foreground.b * foreground_weight,
    )
}

/// Read-only collaborators needed to decorate and style the preview surface.
///
/// The comments state is borrowed only for its decoration queries; find data
/// is passed as the already-resolved query/current match rather than exposing
/// the app's find controller to the preview feature.
pub(crate) struct ViewContext<'a> {
    comments: &'a comments::State,
    find_query: &'a str,
    current_find_match: Option<(usize, Range<usize>)>,
    palette: Palette,
}

impl<'a> ViewContext<'a> {
    pub(crate) fn new(
        comments: &'a comments::State,
        find_query: &'a str,
        current_find_match: Option<(usize, Range<usize>)>,
        palette: Palette,
    ) -> Self {
        Self {
            comments,
            find_query,
            current_find_match,
            palette,
        }
    }
}

/// Builds the complete preview surface, including the hidden-scrollbar
/// scrollable targeted by preview-local scroll operations.
pub(crate) fn view<'a>(state: &'a State, context: ViewContext<'a>) -> Element<'a, Message> {
    let position = state.caret();
    let visual = state.visual_selection();
    let palette = context.palette;

    scrollable(
        container(
            PreviewViewer {
                claims: state.claims(),
                focused_element: position.element,
                caret_column: position.column,
                visual,
                comments: context.comments,
                find_query: context.find_query,
                current_match: context.current_find_match,
                palette,
            }
            .blocks(
                markdown::Settings::with_text_size(
                    crate::typography::TEXT_SIZE,
                    markdown_style(&palette),
                ),
                state.markdown().items(),
            ),
        )
        .width(Length::Fill)
        .padding([0, 8]),
    )
    .id(scrollable_id())
    .direction(scrollable::Direction::Vertical(
        scrollable::Scrollbar::hidden(),
    ))
    .height(Length::Fill)
    .into()
}

fn markdown_style(palette: &Palette) -> markdown::Style {
    markdown::Style {
        font: PREVIEW_FONT,
        inline_code_font: PREVIEW_FONT,
        code_block_font: PREVIEW_FONT,
        inline_code_highlight: markdown::Highlight {
            background: Background::Color(palette.raised()),
            border: border::rounded(4),
        },
        inline_code_color: palette.foreground,
        link_color: palette.accent,
        ..markdown::Style::from(palette.iced())
    }
}

/// The palette-styled container every code surface paints on — code blocks
/// and image chips — instead of iced's stock `#111111` panel.
fn surface_style(surface: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(surface)),
        border: border::rounded(4),
        ..Default::default()
    }
}

/// A code block's lines without interactive decorations — the fallback
/// when the parse walk and the viewer's numbering disagree. Mirrors
/// iced's own code block, but on the palette's surface instead of its
/// stock dark panel.
fn plain_code_block<'a>(
    settings: markdown::Settings,
    lines: &'a [markdown::Text],
) -> Element<'a, Message> {
    scrollable(
        container(column(lines.iter().map(|line| {
            rich_text(line.spans(settings.style))
                .on_link_click(Message::LinkClicked)
                .font(settings.style.code_block_font)
                .size(settings.code_size)
                .line_height(crate::typography::LINE_HEIGHT)
                .into()
        })))
        .padding(settings.code_size),
    )
    .direction(scrollable::Direction::Horizontal(
        scrollable::Scrollbar::default()
            .width(settings.code_size / 2)
            .scroller_width(settings.code_size / 2),
    ))
    .into()
}

struct PreviewViewer<'a> {
    /// Claims the next numbered element per rendered item from the map —
    /// the viewer never counts itself, so the numbering parity between the
    /// parse walk and the viewer lives in the preview module alone.
    claims: super::Claims<'a>,
    focused_element: usize,
    caret_column: usize,
    /// The `(anchor, caret)` endpoints of the visual-mode selection.
    visual: Option<(super::CaretPosition, super::CaretPosition)>,
    /// Saved comments, to know which elements carry one.
    comments: &'a comments::State,
    /// The find popup's query; its matches paint as highlights.
    find_query: &'a str,
    /// The current find match, as the element and range it lives in.
    current_match: Option<(usize, Range<usize>)>,
    /// The palette the preview paints with: the base text color and code
    /// surfaces come from it directly, the decoration producers' colors
    /// through its [`Config`].
    palette: Palette,
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
        self.text_element(settings, text, heading_color(level, &self.palette))
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        self.text_element(settings, text, self.palette.foreground)
    }

    fn quote(
        &self,
        settings: markdown::Settings,
        contents: &'a [markdown::Item],
    ) -> Element<'a, Message> {
        row![rule::vertical(4), self.blocks(settings, contents)]
            .height(Length::Shrink)
            .spacing(settings.spacing)
            .into()
    }

    fn ordered_list(
        &self,
        settings: markdown::Settings,
        start: u64,
        bullets: &'a [markdown::Bullet],
    ) -> Element<'a, Message> {
        self.list(settings, Some(start), bullets)
    }

    fn unordered_list(
        &self,
        settings: markdown::Settings,
        bullets: &'a [markdown::Bullet],
    ) -> Element<'a, Message> {
        self.list(settings, None, bullets)
    }

    fn image(
        &self,
        settings: markdown::Settings,
        _url: &'a String,
        _title: &'a str,
        alt: &markdown::Text,
    ) -> Element<'a, Message> {
        // Image alt text rides on the same palette surface as code, never
        // iced's stock dark chip.
        let surface = self.palette.raised();

        container(rich_text(alt.spans(settings.style)).on_link_click(Message::LinkClicked))
            .padding(settings.spacing.0)
            .style(move |_theme| surface_style(surface))
            .into()
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        // The map numbers code blocks like any element; if they ever
        // disagree, fall back to the plain, non-interactive look.
        let Some((element, preview_element)) = self.claims.claim() else {
            let surface = self.palette.raised();

            return container(plain_code_block(settings, lines))
                .width(Length::Fill)
                .padding(settings.code_size / 4.0)
                .style(move |_theme| surface_style(surface))
                .into();
        };

        let decorations = self.decorations(element, preview_element);

        // The code block keeps the default look — a palette surface, inset
        // — with the interactive code inside instead of the plain lines.
        let surface = self.palette.raised();

        container(interactive_text::code(
            settings,
            preview_element.text(),
            self.palette.foreground,
            decorations,
        ))
        .width(Length::Fill)
        .padding(settings.code_size / 4.0)
        .style(move |_theme| surface_style(surface))
        .into()
    }
}

impl<'a> PreviewViewer<'a> {
    fn blocks(
        &self,
        settings: markdown::Settings,
        items: &'a [markdown::Item],
    ) -> Element<'a, Message> {
        // Settings.spacing also controls table padding and horizontal gutters.
        // Keep paragraph spacing separate, including the text widgets' insets
        // in (not on top of) the shared content-to-content gap.
        column(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| markdown::item(self, settings, item, index)),
        )
        .spacing(crate::typography::PARAGRAPH_GAP - 2.0 * interactive_text::PARAGRAPH_PADDING)
        .into()
    }

    fn list(
        &self,
        settings: markdown::Settings,
        start: Option<u64>,
        bullets: &'a [markdown::Bullet],
    ) -> Element<'a, Message> {
        let marker_width = start.map_or(settings.text_size.0, |start| {
            let last = start.saturating_add(bullets.len().saturating_sub(1) as u64);
            (last.to_string().len() as f32 + 1.0) * settings.text_size.0 * 0.6
        });
        // Iced's default list multiplies settings.spacing for both indentation
        // and every bullet. Paragraph gaps belong between blocks, not between
        // tight items; keep a separate, font-sized marker gutter instead.
        column(bullets.iter().enumerate().map(|(index, bullet)| {
            let (markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. }) = bullet;
            let marker: Element<'a, Message> =
                if let (None, markdown::Bullet::Task { done, .. }) = (start, bullet) {
                    container(checkbox(*done).size(settings.text_size))
                        .center_y(settings.text_size * crate::typography::LINE_HEIGHT)
                        .align_x(iced::alignment::Horizontal::Right)
                        .width(marker_width)
                        .into()
                } else {
                    text(start.map_or_else(
                        || "•".to_owned(),
                        |start| format!("{}.", start.saturating_add(index as u64)),
                    ))
                    .font(settings.style.font)
                    .size(settings.text_size)
                    .line_height(crate::typography::LINE_HEIGHT)
                    .align_x(iced::alignment::Horizontal::Right)
                    .width(marker_width)
                    .into()
                };
            row![
                container(marker).padding([interactive_text::PARAGRAPH_PADDING, 0.0]),
                self.blocks(settings, items),
            ]
            .spacing(settings.text_size * 0.5)
            .into()
        }))
        .into()
    }

    /// Assembles the ordered producer pipeline shared by paragraphs and code
    /// blocks. Producers append generic primitives; this viewer only supplies
    /// their read-only collaborators and registration order.
    fn decorations(
        &self,
        element: usize,
        preview_element: &PreviewElement,
    ) -> interactive_text::TextDecorations {
        let config = Config::from_palette(&self.palette);

        Pipeline::new()
            // Preserve paint order: the current-line band, comment element
            // tint and outlines, span tint, ordinary/current find matches,
            // visual selection, then caret.
            .append(|output| {
                decorations::append_current_line(
                    output,
                    element,
                    self.focused_element,
                    self.caret_column,
                    &config,
                )
            })
            .append(|output| {
                decorations::append_comments(
                    output,
                    self.comments,
                    element,
                    preview_element.len(),
                    anchor_id(),
                    &config,
                )
            })
            .append(|output| {
                decorations::append_find(
                    output,
                    preview_element,
                    element,
                    self.find_query,
                    self.current_match.as_ref(),
                    &config,
                )
            })
            .append(|output| {
                decorations::append_visual(
                    output,
                    element,
                    preview_element.len(),
                    self.visual,
                    &config,
                )
            })
            .append(|output| {
                decorations::append_caret(
                    output,
                    element,
                    self.focused_element,
                    self.caret_column,
                    caret_id(),
                    &config,
                )
            })
            .finish()
    }

    fn text_element(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
        color: Color,
    ) -> Element<'a, Message> {
        // The map numbers elements exactly like the viewer numbers items;
        // if they ever disagree the item renders plainly, without caret,
        // selection, or comment mark.
        let Some((element, preview_element)) = self.claims.claim() else {
            return interactive_text::paragraph(
                settings,
                text,
                color,
                interactive_text::TextDecorations::default(),
            );
        };

        let decorations = self.decorations(element, preview_element);

        interactive_text::paragraph(settings, text, color, decorations)
    }
}

#[cfg(test)]
mod tests {
    use super::{heading_color, markdown_style, surface_style, MIN_HEADING_CONTRAST};
    use crate::theme::Palette;
    use iced::widget::markdown::HeadingLevel;
    use iced::{Background, Color};

    /// The markdown style paints with palette roles: links take the accent,
    /// inline code the foreground, and code sits on the raised surface —
    /// never iced's stock `#111111` chip with white glyphs.
    #[test]
    fn markdown_style_uses_palette_roles() {
        let mut palette = Palette::default();
        palette.foreground = palette.red;
        palette.accent = palette.blue;
        palette.lighter_background = palette.green;
        palette.dark_background = palette.yellow;

        let style = markdown_style(&palette);

        assert_eq!(style.link_color, palette.blue);
        assert_eq!(style.inline_code_color, palette.red);
        assert_eq!(
            style.inline_code_highlight.background,
            Background::Color(palette.raised())
        );
    }

    /// Heading levels take distinct semantic roles from the active palette,
    /// softened by progressively smaller amounts toward its foreground.
    #[test]
    fn heading_colors_use_level_specific_palette_roles() {
        let palette = Palette {
            background: Color::BLACK,
            foreground: Color::WHITE,
            magenta: Color::from_rgb(1.0, 0.0, 0.0),
            blue: Color::from_rgb(0.0, 1.0, 0.0),
            cyan: Color::from_rgb(0.0, 0.0, 1.0),
            green: Color::from_rgb(1.0, 1.0, 0.0),
            yellow: Color::from_rgb(1.0, 0.0, 1.0),
            ..Palette::default()
        };

        let colors = [
            heading_color(&HeadingLevel::H1, &palette),
            heading_color(&HeadingLevel::H2, &palette),
            heading_color(&HeadingLevel::H3, &palette),
            heading_color(&HeadingLevel::H4, &palette),
            heading_color(&HeadingLevel::H5, &palette),
            heading_color(&HeadingLevel::H6, &palette),
        ];

        let expected = [
            Color::from_rgb(1.0, 0.35, 0.35),
            Color::from_rgb(0.4, 1.0, 0.4),
            Color::from_rgb(0.45, 0.45, 1.0),
            Color::from_rgb(1.0, 1.0, 0.55),
            Color::from_rgb(1.0, 0.7, 1.0),
            Color::from_rgb(1.0, 0.88, 0.88),
        ];
        let close = colors.into_iter().zip(expected).all(|(actual, expected)| {
            (actual.r - expected.r).abs() < 1e-6
                && (actual.g - expected.g).abs() < 1e-6
                && (actual.b - expected.b).abs() < 1e-6
        });

        assert!(close, "unexpected heading colors: {colors:?}");
    }

    /// A role that disappears into a light background is pulled toward the
    /// theme foreground until it reaches ordinary-text WCAG contrast.
    #[test]
    fn heading_colors_guard_contrast_on_light_themes() {
        let palette = Palette {
            background: Color::WHITE,
            foreground: Color::BLACK,
            magenta: Color::WHITE,
            blue: Color::WHITE,
            cyan: Color::WHITE,
            green: Color::WHITE,
            yellow: Color::WHITE,
            ..Palette::default()
        };

        let colors = [
            heading_color(&HeadingLevel::H1, &palette),
            heading_color(&HeadingLevel::H2, &palette),
            heading_color(&HeadingLevel::H3, &palette),
            heading_color(&HeadingLevel::H4, &palette),
            heading_color(&HeadingLevel::H5, &palette),
            heading_color(&HeadingLevel::H6, &palette),
        ];

        assert!(colors
            .into_iter()
            .all(|color| { color.relative_contrast(palette.background) >= MIN_HEADING_CONTRAST }));
    }

    /// The shared surface style paints the given color with rounded
    /// corners and nothing else.
    #[test]
    fn surface_style_paints_only_the_surface() {
        let palette = Palette::default();
        let style = surface_style(palette.raised());

        assert_eq!(style.background, Some(Background::Color(palette.raised())));
        assert_eq!(style.text_color, None);
        assert_eq!(style.border.width, 0.0);
        assert_eq!(style.border.radius.top_left, 4.0);
    }
}
