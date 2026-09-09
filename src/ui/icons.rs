//! Canvas-drawn icons shared by the application UI.

use iced::widget::canvas;
use iced::{mouse, Point, Rectangle, Renderer, Theme};

/// The design space the icon glyphs are drawn in, before scaling to the
/// canvas size.
const ICON_DESIGN_SIZE: f32 = 16.0;
/// The icon glyph size used by application controls.
pub(crate) const ICON_SIZE: f32 = 24.0;

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
