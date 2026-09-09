//! Shared writing and preview rhythm. Markdown heading sizes remain semantic.

pub(crate) const TEXT_SIZE: f32 = 20.0;
pub(crate) const LINE_HEIGHT: f32 = 1.8;
pub(crate) const PARAGRAPH_GAP_SCALE: f32 = 1.5;
/// One blank source line, or the gap between rendered Markdown blocks.
pub(crate) const PARAGRAPH_GAP: f32 = TEXT_SIZE * LINE_HEIGHT * PARAGRAPH_GAP_SCALE;
