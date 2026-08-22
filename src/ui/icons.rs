//! Canvas-drawn icons shared by the application UI.

use iced::widget::canvas;
use iced::{mouse, Point, Rectangle, Renderer, Theme};

/// The design space the icon glyphs are drawn in, before scaling to the
/// canvas size.
const ICON_DESIGN_SIZE: f32 = 16.0;
/// The icon glyph size used by application controls.
pub(crate) const ICON_SIZE: f32 = 24.0;
/// The bottom-bar icon button size.
pub(crate) const ICON_BUTTON_SIZE: f32 = 42.0;

/// The stroke every icon glyph draws with. The color comes from the theme
/// passed to `draw` — the runtime theme, whose foreground is the omarchy
/// foreground — so icons follow the palette like every other surface.
fn glyph_stroke(theme: &Theme) -> canvas::Stroke<'_> {
    let color = theme.palette().text;

    canvas::Stroke::default()
        .with_color(color)
        .with_width(1.4)
        .with_line_cap(canvas::LineCap::Round)
        .with_line_join(canvas::LineJoin::Round)
}

pub(crate) struct OpenFileIcon;

impl<Message> canvas::Program<Message> for OpenFileIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let folder = canvas::Path::new(|path| {
            path.move_to(Point::new(2.5, 13.0));
            path.line_to(Point::new(2.5, 3.5));
            path.line_to(Point::new(6.5, 3.5));
            path.line_to(Point::new(8.5, 5.5));
            path.line_to(Point::new(13.5, 5.5));
            path.line_to(Point::new(13.5, 13.0));
            path.close();
        });

        frame.stroke(&folder, glyph_stroke(theme));

        vec![frame.into_geometry()]
    }
}

pub(crate) struct PreviewIcon;

impl<Message> canvas::Program<Message> for PreviewIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let eye = canvas::Path::new(|path| {
            path.move_to(Point::new(1.5, 8.0));
            path.quadratic_curve_to(Point::new(8.0, 1.0), Point::new(14.5, 8.0));
            path.quadratic_curve_to(Point::new(8.0, 15.0), Point::new(1.5, 8.0));
            path.close();
        });

        frame.stroke(&eye, glyph_stroke(theme));

        let pupil = canvas::Path::circle(Point::new(8.0, 8.0), 2.8);
        frame.fill(&pupil, theme.palette().text);

        vec![frame.into_geometry()]
    }
}

/// The write-mode icon of the preview toggle: a pencil, drawn in the same
/// stroke style as the eye so the toggle reads as one button with two
/// glyphs.
pub(crate) struct WriteIcon;

impl<Message> canvas::Program<Message> for WriteIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let stroke = || glyph_stroke(theme);

        // A pencil lying diagonal: a triangular tip at the bottom-left, a
        // band above it, and the body running to the top-right.
        let body = canvas::Path::new(|path| {
            path.move_to(Point::new(2.5, 13.5));
            path.line_to(Point::new(3.35, 10.95));
            path.line_to(Point::new(9.07, 5.23));
            path.line_to(Point::new(10.77, 6.93));
            path.line_to(Point::new(5.05, 12.65));
            path.close();
        });
        let band = canvas::Path::new(|path| {
            path.move_to(Point::new(3.35, 10.95));
            path.line_to(Point::new(5.05, 12.65));
        });
        let edge = canvas::Path::new(|path| {
            path.move_to(Point::new(9.6, 4.7));
            path.line_to(Point::new(11.3, 6.4));
        });

        frame.stroke(&body, stroke());
        frame.stroke(&band, stroke());
        frame.stroke(&edge, stroke());

        vec![frame.into_geometry()]
    }
}

pub(crate) struct SaveIcon;

impl<Message> canvas::Program<Message> for SaveIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let stroke = || glyph_stroke(theme);

        // A floppy disk: the body with a beveled corner, the shutter notch
        // on top, and the label slot at the bottom.
        let body = canvas::Path::new(|path| {
            path.move_to(Point::new(2.5, 1.5));
            path.line_to(Point::new(11.0, 1.5));
            path.line_to(Point::new(13.5, 4.0));
            path.line_to(Point::new(13.5, 14.5));
            path.line_to(Point::new(2.5, 14.5));
            path.close();
        });
        let shutter = canvas::Path::new(|path| {
            path.move_to(Point::new(5.0, 1.5));
            path.line_to(Point::new(5.0, 6.0));
            path.line_to(Point::new(10.5, 6.0));
            path.line_to(Point::new(10.5, 1.5));
        });
        let slot = canvas::Path::new(|path| {
            path.move_to(Point::new(4.5, 14.5));
            path.line_to(Point::new(4.5, 10.0));
            path.line_to(Point::new(11.5, 10.0));
            path.line_to(Point::new(11.5, 14.5));
        });

        frame.stroke(&body, stroke());
        frame.stroke(&shutter, stroke());
        frame.stroke(&slot, stroke());

        vec![frame.into_geometry()]
    }
}

/// The comments icon of the collapsed rail: a speech bubble matching the
/// application's other line icons.
pub(crate) struct CommentsIcon;

impl<Message> canvas::Program<Message> for CommentsIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        // The glyph is drawn in a 16x16 design space, scaled to the canvas.
        frame.scale(bounds.width / ICON_DESIGN_SIZE);

        let stroke = || glyph_stroke(theme);

        let bubble = canvas::Path::new(|path| {
            path.move_to(Point::new(8.0, 2.5));
            path.quadratic_curve_to(Point::new(13.5, 2.5), Point::new(13.5, 7.0));
            path.quadratic_curve_to(Point::new(13.5, 10.5), Point::new(9.5, 10.8));
            path.line_to(Point::new(6.0, 13.0));
            path.line_to(Point::new(6.4, 10.5));
            path.quadratic_curve_to(Point::new(2.5, 10.2), Point::new(2.5, 7.0));
            path.quadratic_curve_to(Point::new(2.5, 2.5), Point::new(8.0, 2.5));
            path.close();
        });

        frame.stroke(&bubble, stroke());

        vec![frame.into_geometry()]
    }
}
