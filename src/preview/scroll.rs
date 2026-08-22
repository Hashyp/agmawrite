//! Preview widget-tree scrolling operations.
//!
//! The application composes the preview surface, but scrolling stays local to
//! the preview feature: operations measure the preview viewport and caret and
//! emit [`Message::ScrollBy`] for the reducer to apply.

use super::{Message, Page, Placement};

use iced::advanced::widget::operation::{Outcome, Scrollable};
use iced::advanced::widget::Operation;
use iced::widget::{
    operation::{scroll_by as iced_scroll_by, AbsoluteOffset},
    Id,
};
use iced::{Rectangle, Task, Vector};

const PREVIEW_SCROLL_ID: &str = "preview-scroll";
const PREVIEW_CARET_ID: &str = "preview-caret";
const PREVIEW_ANCHOR_ID: &str = "preview-comment-anchor";
/// Margin kept between the preview caret and the viewport edges.
const CARET_MARGIN: f32 = 8.0;
/// Deltas smaller than half a logical pixel do not visibly move the viewport.
const MIN_SCROLL_DELTA: f32 = 0.5;

/// The widget id attached to the preview scrollable.
pub(crate) fn scrollable_id() -> Id {
    Id::new(PREVIEW_SCROLL_ID)
}

/// The widget id attached to the currently focused preview caret element.
pub(crate) fn caret_id() -> Id {
    Id::new(PREVIEW_CARET_ID)
}

/// The widget id attached to the element the active comment anchors to.
pub(crate) fn anchor_id() -> Id {
    Id::new(PREVIEW_ANCHOR_ID)
}

/// Scrolls the preview by an absolute delta emitted by a measuring operation.
pub(crate) fn scroll_by(delta: f32) -> Task<Message> {
    iced_scroll_by(scrollable_id(), AbsoluteOffset { x: 0.0, y: delta })
}

/// Measures the preview scrollable and caret, then minimally reveals the caret.
pub(crate) fn reveal_caret() -> Task<Message> {
    iced::advanced::widget::operate(RevealTarget::new(CaretScroll::Reveal, caret_id()))
}

/// Measures the preview scrollable and the active comment's anchored
/// element, then minimally reveals that element — activating a comment
/// scrolls to its text without placing a caret on it.
pub(crate) fn reveal_anchor() -> Task<Message> {
    iced::advanced::widget::operate(RevealTarget::new(CaretScroll::Reveal, anchor_id()))
}

/// Measures the preview viewport and scrolls by a whole or half page.
pub(crate) fn scroll_page(page: Page, count: usize) -> Task<Message> {
    iced::advanced::widget::operate(PageScroll {
        scroll_id: scrollable_id(),
        viewport: None,
        page,
        count,
    })
}

/// Places the caret at the requested viewport location (`zz`, `zt`, or `zb`).
pub(crate) fn place_caret_in_view(placement: Placement) -> Task<Message> {
    let scroll = match placement {
        Placement::Center => CaretScroll::Center,
        Placement::Top => CaretScroll::Top,
        Placement::Bottom => CaretScroll::Bottom,
    };

    iced::advanced::widget::operate(RevealTarget::new(scroll, caret_id()))
}

/// How a [`RevealCaret`] operation scrolls the caret into place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaretScroll {
    /// The minimum scroll that brings the caret back into view.
    Reveal,
    /// `zz` — the caret lands in the middle of the viewport.
    Center,
    /// `zt` — the caret lands at the top of the viewport.
    Top,
    /// `zb` — the caret lands at the bottom of the viewport.
    Bottom,
}

/// Calculates the vertical scroll needed for a caret and viewport geometry.
///
/// `caret_top` must already be translated from content coordinates into the
/// viewport's coordinate space. Returning `None` preserves the current scroll.
fn caret_delta(
    viewport_top: f32,
    viewport_height: f32,
    caret_top: f32,
    caret_height: f32,
    scroll: CaretScroll,
) -> Option<f32> {
    let caret_bottom = caret_top + caret_height;
    let viewport_bottom = viewport_top + viewport_height;

    let delta = match scroll {
        CaretScroll::Reveal => {
            if caret_top < viewport_top + CARET_MARGIN {
                caret_top - viewport_top - CARET_MARGIN
            } else if caret_bottom > viewport_bottom - CARET_MARGIN {
                caret_bottom - viewport_bottom + CARET_MARGIN
            } else {
                return None;
            }
        }
        CaretScroll::Center => {
            let caret_middle = caret_top + caret_height / 2.0;
            caret_middle - (viewport_top + viewport_height / 2.0)
        }
        CaretScroll::Top => caret_top - viewport_top - CARET_MARGIN,
        CaretScroll::Bottom => caret_bottom - viewport_bottom + CARET_MARGIN,
    };

    (delta.abs() >= MIN_SCROLL_DELTA).then_some(delta)
}

/// Calculates a whole- or half-page scroll while retaining caret margin as
/// context. A zero count means one page, matching the preview motion policy.
fn page_delta(viewport_height: f32, page: Page, count: usize) -> f32 {
    let full = (viewport_height - 2.0 * CARET_MARGIN).max(1.0);
    let half = full / 2.0;

    let (distance, sign) = match page {
        Page::HalfDown => (half, 1.0),
        Page::HalfUp => (half, -1.0),
        Page::FullDown => (full, 1.0),
        Page::FullUp => (full, -1.0),
    };

    sign * distance * count.max(1) as f32
}

/// Measures the preview scrollable and one target element in the widget tree
/// — the caret's element, or the element the active comment anchors to.
struct RevealTarget {
    scroll_id: Id,
    target_id: Id,
    viewport: Option<(Rectangle, Vector)>,
    target: Option<Rectangle>,
    scroll: CaretScroll,
}

impl RevealTarget {
    fn new(scroll: CaretScroll, target_id: Id) -> Self {
        Self {
            scroll_id: scrollable_id(),
            target_id,
            viewport: None,
            target: None,
            scroll,
        }
    }
}

impl Operation<Message> for RevealTarget {
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
        if Some(&self.target_id) == id && self.target.is_none() {
            self.target = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        let Some((viewport, translation)) = self.viewport else {
            return Outcome::None;
        };
        let Some(target) = self.target else {
            return Outcome::None;
        };

        // Child layouts live in unscrolled content space; the viewport shows
        // them shifted up by the current translation.
        let target_top = target.y - translation.y;

        caret_delta(
            viewport.y,
            viewport.height,
            target_top,
            target.height,
            self.scroll,
        )
        .map_or(Outcome::None, |delta| {
            Outcome::Some(Message::ScrollBy(delta))
        })
    }
}

/// Measures the preview viewport for whole- and half-page scrolling.
struct PageScroll {
    scroll_id: Id,
    viewport: Option<Rectangle>,
    page: Page,
    count: usize,
}

impl Operation<Message> for PageScroll {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        _translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if Some(&self.scroll_id) == id {
            self.viewport = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        let Some(viewport) = self.viewport else {
            return Outcome::None;
        };

        Outcome::Some(Message::ScrollBy(page_delta(
            viewport.height,
            self.page,
            self.count,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{caret_delta, page_delta, CaretScroll};
    use crate::preview::Page;

    #[test]
    fn reveal_delta_only_moves_a_caret_beyond_the_margins() {
        assert_eq!(
            caret_delta(100.0, 200.0, 120.0, 20.0, CaretScroll::Reveal),
            None
        );
        assert_eq!(
            caret_delta(100.0, 200.0, 100.0, 10.0, CaretScroll::Reveal),
            Some(-8.0)
        );
        assert_eq!(
            caret_delta(100.0, 200.0, 285.0, 10.0, CaretScroll::Reveal),
            Some(3.0)
        );

        // Exactly on the inner margin remains visible.
        assert_eq!(
            caret_delta(100.0, 200.0, 108.0, 184.0, CaretScroll::Reveal),
            None
        );
    }

    #[test]
    fn placement_delta_targets_center_top_and_bottom() {
        assert_eq!(
            caret_delta(100.0, 200.0, 150.0, 10.0, CaretScroll::Center),
            Some(-45.0)
        );
        assert_eq!(
            caret_delta(100.0, 200.0, 150.0, 10.0, CaretScroll::Top),
            Some(42.0)
        );
        assert_eq!(
            caret_delta(100.0, 200.0, 150.0, 10.0, CaretScroll::Bottom),
            Some(-132.0)
        );
    }

    #[test]
    fn subpixel_caret_delta_keeps_the_viewport_still() {
        assert_eq!(
            caret_delta(0.0, 100.0, 49.9, 1.0, CaretScroll::Center),
            None
        );
        assert_eq!(
            caret_delta(0.0, 100.0, 50.0, 1.0, CaretScroll::Center),
            Some(0.5)
        );
    }

    #[test]
    fn page_delta_preserves_context_direction_and_count() {
        assert_eq!(page_delta(200.0, Page::FullDown, 0), 184.0);
        assert_eq!(page_delta(200.0, Page::FullUp, 2), -368.0);
        assert_eq!(page_delta(200.0, Page::HalfDown, 3), 276.0);
        assert_eq!(page_delta(200.0, Page::HalfUp, 1), -92.0);
        assert_eq!(page_delta(10.0, Page::FullDown, 1), 1.0);
    }
}
