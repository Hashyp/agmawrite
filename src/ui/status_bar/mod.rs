//! The global status bar: a text-only Lualine strip that reports the
//! workspace on both surfaces, adapting its segments to what each mode
//! implements. Actions reuse the toolbar protocol.

pub(crate) mod metadata;

use std::path::Path;

use iced::widget::{button, canvas, container, responsive, row, text, tooltip, Space};
use iced::{
    alignment, mouse, Background, Color, Element, Font, Length, Point, Rectangle, Renderer, Theme,
};
use unicode_segmentation::UnicodeSegmentation;

use super::toolbar;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Message {
    Toolbar(toolbar::Message),
    ToggleComments,
}
use crate::input::{PreviewToggle, Surface, ViewProjection};
use crate::theme::Palette;
use metadata::Metadata;

const HEIGHT: f32 = 32.0;
const FONT_SIZE: f32 = 14.0;
const FONT: Font = Font::with_name("JetBrainsMono Nerd Font");
const BOLD: Font = Font {
    weight: iced::font::Weight::Bold,
    ..FONT
};
/// The nf-md `content_copy` glyph — the report's "it was copied" mark.
const COPIED: &str = "󰆏";
/// Below this width the report yields to the actions, as the ruler does.
const REPORT_MIN_WIDTH: f32 = 600.0;

pub(crate) struct Model {
    pub(crate) interaction: ViewProjection,
    pub(crate) filename: String,
    pub(crate) full_path: String,
    pub(crate) location: String,
    pub(crate) progress: String,
    pub(crate) metadata: Metadata,
    pub(crate) palette: Palette,
    pub(crate) comment_count: usize,
    /// The transient yank report, shown where Neovim's message area would
    /// be: beside the mode badge until the next interaction.
    pub(crate) report: Option<String>,
}

pub(crate) fn filename(path: Option<&Path>, modified: bool) -> String {
    let name = path
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy());
    format!(
        "{}{}",
        name.as_deref().unwrap_or("[No Name]"),
        if modified { " [+]" } else { "" }
    )
}

fn mode_label(interaction: ViewProjection) -> &'static str {
    match interaction.surface() {
        Surface::Write => "WRITE",
        Surface::Preview if interaction.visual() => "VISUAL",
        Surface::Preview => "VIEW",
    }
}

#[derive(Clone, Copy)]
struct Colors {
    base: Color,
    raised: Color,
    foreground: Color,
    mode: Color,
    on_mode: Color,
}

impl Colors {
    fn new(palette: Palette, interaction: ViewProjection) -> Self {
        let mode = mode_color(palette, interaction);
        Self {
            base: palette.dark_background,
            raised: Color::from_rgb(
                palette.dark_background.r * 0.7 + mode.r * 0.3,
                palette.dark_background.g * 0.7 + mode.g * 0.3,
                palette.dark_background.b * 0.7 + mode.b * 0.3,
            ),
            foreground: palette.foreground,
            mode,
            // A theme can be deliberately monochrome; use the more legible
            // of its own page/foreground colors rather than hardcoded black.
            on_mode: if contrast(mode, palette.background) >= contrast(mode, palette.foreground) {
                palette.background
            } else {
                palette.foreground
            },
        }
    }
}

fn contrast(a: Color, b: Color) -> f32 {
    fn luminance(c: Color) -> f32 {
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    }
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The badge hue names the surface: WRITE takes the palette's red — the
/// state that changes the document — VIEW its blue, VISUAL its magenta.
fn mode_color(palette: Palette, interaction: ViewProjection) -> Color {
    match interaction.surface() {
        Surface::Write => palette.red,
        Surface::Preview if interaction.visual() => palette.magenta,
        Surface::Preview => palette.blue,
    }
}

/// Mirrors installed LazyVim's a/b/c … x/y/z sections and Powerline angles.
/// Narrow layouts shed metadata before controls; file names never wrap.
pub(crate) fn view(model: Model) -> Element<'static, Message> {
    responsive(move |size| {
        let colors = Colors::new(model.palette, model.interaction);
        let compact = size.width < 600.0;
        let mut segments: Vec<Element<'_, Message>> = vec![segment(
            mode_label(model.interaction),
            colors.mode,
            colors.on_mode,
            true,
        )];
        // The left rail after the badge: the yank report and the git branch
        // share the raised surface, the filename sits on the base — each
        // joined to the last by a wedge, so the chain reads as one section.
        let report = model
            .report
            .as_deref()
            .filter(|_| size.width >= REPORT_MIN_WIDTH)
            .map(|report| format!("{COPIED} {report}"));
        let branch = model
            .metadata
            .branch
            .as_ref()
            .filter(|_| size.width >= 1200.0)
            .map(|branch| format!(" {}", shorten(branch, 24)));
        let mut rail_background = colors.mode;
        for (label, surface) in [
            report.map(|label| (label, colors.raised)),
            branch.map(|label| (label, colors.raised)),
        ]
        .into_iter()
        .flatten()
        {
            segments.push(separator(rail_background, surface, false));
            segments.push(segment(label, surface, colors.foreground, false));
            rail_background = surface;
        }
        if size.width >= 900.0 {
            segments.push(separator(rail_background, colors.base, false));
            let label = format!(
                "󰍔 {}",
                shorten(&model.filename, if size.width < 1200.0 { 18 } else { 32 })
            );
            segments.push(
                tooltip(
                    segment(label, colors.base, colors.foreground, false),
                    container(text(model.full_path.clone()).font(FONT).size(12))
                        .padding([4, 8])
                        .style(move |theme| super::tooltip::style(&model.palette, theme)),
                    tooltip::Position::Top,
                )
                .into(),
            );
        }
        segments.push(Space::new().width(Length::Fill).into());
        if let Some(count) = model.interaction.toolbar().pending_count() {
            segments.push(segment(
                count.to_string(),
                colors.base,
                colors.foreground,
                false,
            ));
        }
        // The surface switch names its destination: Preview while writing,
        // Write while previewing. Preview-only sessions offer no switch.
        if let Some(toggle) = model.interaction.toolbar().preview_toggle() {
            let (label, hint) = match toggle {
                PreviewToggle::Preview => ("Preview", "Open the preview · Ctrl + P"),
                PreviewToggle::Write => ("Write", "Return to writing · Ctrl + P"),
            };
            segments.push(action(
                label,
                hint,
                Message::Toolbar(toolbar::Message::TogglePreview),
                colors,
                model.palette,
            ));
        }
        let show_ruler = size.width >= 640.0;
        if show_ruler {
            segments.push(separator(colors.raised, colors.base, true));
            segments.push(segment(
                format!("{}  {}", model.progress, model.location),
                colors.raised,
                colors.foreground,
                false,
            ));
        }
        // The comments segment is a preview feature — its composer and
        // cycling are preview verbs — so the write bar ends at its ruler.
        if matches!(model.interaction.surface(), Surface::Preview) {
            segments.push(separator(
                colors.mode,
                if show_ruler {
                    colors.raised
                } else {
                    colors.base
                },
                true,
            ));
            segments.push(action(
                if compact {
                    "Comments".to_owned()
                } else {
                    format!("Comments {}", model.comment_count)
                },
                "Toggle comments sidebar · Ctrl + B",
                Message::ToggleComments,
                Colors {
                    base: colors.mode,
                    raised: Palette::darkened(colors.mode, 0.12),
                    foreground: colors.on_mode,
                    ..colors
                },
                model.palette,
            ));
        }
        container(row(segments).align_y(alignment::Vertical::Center))
            .width(Length::Fill)
            .height(HEIGHT)
            .style(move |_| container::Style {
                background: Some(colors.base.into()),
                ..Default::default()
            })
            .into()
    })
    .height(HEIGHT)
    .into()
}

fn shorten(label: &str, limit: usize) -> String {
    if label.graphemes(true).count() <= limit {
        label.to_owned()
    } else {
        format!(
            "{}…",
            label.graphemes(true).take(limit - 1).collect::<String>()
        )
    }
}

fn segment(
    label: impl Into<String>,
    background: Color,
    foreground: Color,
    bold: bool,
) -> Element<'static, Message> {
    centered_label(label, bold)
        .padding([0, 8])
        .style(move |_| container::Style {
            background: Some(background.into()),
            text_color: Some(foreground),
            ..Default::default()
        })
        .into()
}

// Buttons and passive segments use identical label geometry, so their text
// shares a baseline instead of buttons placing their labels at the top edge.
fn centered_label(label: impl Into<String>, bold: bool) -> container::Container<'static, Message> {
    container(
        text(label.into())
            .font(if bold { BOLD } else { FONT })
            .size(FONT_SIZE)
            .wrapping(text::Wrapping::None),
    )
    .height(HEIGHT)
    .align_y(alignment::Vertical::Center)
}

fn action(
    label: impl Into<String>,
    hint: &'static str,
    message: Message,
    colors: Colors,
    palette: Palette,
) -> Element<'static, Message> {
    tooltip(
        button(centered_label(label, false))
            .padding([0, 6])
            .height(HEIGHT)
            .on_press(message)
            .style(move |_, status| button::Style {
                background: Some(Background::Color(match status {
                    button::Status::Hovered | button::Status::Pressed => colors.raised,
                    _ => colors.base,
                })),
                text_color: colors.foreground,
                ..Default::default()
            }),
        container(text(hint).font(FONT).size(12))
            .padding([4, 8])
            .style(move |theme| super::tooltip::style(&palette, theme)),
        tooltip::Position::Top,
    )
    .into()
}

// Draw the Powerline wedges instead of relying on font glyph bearings, keeping
// adjoining backgrounds seamless at every display scale.
struct Separator {
    foreground: Color,
    background: Color,
    left: bool,
}

fn separator(foreground: Color, background: Color, left: bool) -> Element<'static, Message> {
    canvas(Separator {
        foreground,
        background,
        left,
    })
    .width(12)
    .height(HEIGHT)
    .into()
}

impl canvas::Program<Message> for Separator {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.background);
        let (edge, tip) = if self.left {
            (bounds.width, 0.0)
        } else {
            (0.0, bounds.width)
        };
        let path = canvas::Path::new(|path| {
            path.move_to(Point::new(edge, 0.0));
            path.line_to(Point::new(tip, bounds.height / 2.0));
            path.line_to(Point::new(edge, bounds.height));
            path.close();
        });
        frame.fill(&path, self.foreground);
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InteractionState;

    #[test]
    fn the_badge_names_the_surface_and_the_preview_mode() {
        let mut write = InteractionState::editable();
        assert_eq!(mode_label(write.view()), "WRITE");
        write.open_find();
        assert_eq!(mode_label(write.view()), "WRITE");
        let _ = write.close_find();

        assert!(write.toggle_preview());
        assert_eq!(mode_label(write.view()), "VIEW");
        write.toggle_visual().unwrap();
        assert_eq!(mode_label(write.view()), "VISUAL");
    }

    #[test]
    fn overlays_leave_underlying_mode_label_unchanged() {
        let mut state = InteractionState::preview_only();
        state.toggle_visual().unwrap();
        state.open_note().unwrap();
        state.open_find();
        state.open_help();
        assert_eq!(mode_label(state.view()), "VISUAL");
    }

    #[test]
    fn filename_reports_unsaved_changes_without_losing_basename() {
        assert_eq!(
            filename(Some(Path::new("/notes/draft.md")), true),
            "draft.md [+]"
        );
        assert_eq!(filename(None, false), "[No Name]");
    }

    #[test]
    fn truncation_preserves_graphemes() {
        assert_eq!(shorten("a👩‍💻bcd", 4), "a👩‍💻b…");
    }

    #[test]
    fn mode_sections_follow_each_new_palette() {
        let first = Palette::from_colors("blue = \"#81a1c1\"");
        let second = Palette::from_colors("blue = \"#e68e0d\"");
        let mut view = InteractionState::editable();
        assert!(view.toggle_preview());
        assert_ne!(
            mode_color(first, view.view()),
            mode_color(second, view.view())
        );

        let mut visual = InteractionState::preview_only();
        visual.toggle_visual().unwrap();
        assert_eq!(mode_color(second, visual.view()), second.magenta);
    }

    #[test]
    fn write_badge_takes_the_palette_red() {
        let palette = Palette::from_colors("red = \"#ff0000\"");
        assert_eq!(
            mode_color(palette, InteractionState::editable().view()),
            palette.red
        );
    }
}
