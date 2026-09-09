//! Exercise the real preview widget layout without a window or GPU.
use super::{view, State, ViewContext};
use crate::{comments, theme::Palette};
use iced::advanced::{layout, renderer::Headless, widget::Tree};
use iced::{Font, Pixels, Renderer, Size};

fn preview_blocks(source: &str, width: f32) -> Vec<layout::Node> {
    let renderer = iced::futures::executor::block_on(Renderer::new(
        Font::MONOSPACE,
        Pixels(20.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer");
    let state = State::new(source);
    let comments = comments::State::default();
    let mut element = view(
        &state,
        ViewContext::new(&comments, "", None, Palette::default()),
    );
    let mut tree = Tree::new(element.as_widget());
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(width, 2000.0)),
    );
    // Scrollable -> inset container -> Markdown column -> rendered blocks.
    node.children()[0].children()[0].children().to_vec()
}

#[test]
fn quoted_paragraphs_use_the_same_gap_without_widening_the_quote_gutter() {
    let blocks = preview_blocks("> first\n>\n> second", 600.0);
    assert_eq!(blocks[0].bounds().height, 134.0);
    // Stock 4px quote rule plus the existing 17.5px gutter, not a 54px gutter.
    assert_eq!(blocks[0].children()[1].bounds().x, 21.5);
}

#[test]
fn task_list_markers_keep_the_same_height_as_their_text() {
    let blocks = preview_blocks("- [x] done\n- [ ] pending", 600.0);
    assert_eq!(blocks[0].bounds().height, 88.0);
}

#[test]
fn separate_paragraphs_inside_a_list_item_keep_the_paragraph_gap() {
    let blocks = preview_blocks("- first\n\n  second paragraph\n\n- next", 600.0);
    assert_eq!(blocks[0].bounds().height, 178.0);
}

#[test]
fn headings_keep_their_sizes_but_use_the_shared_gap() {
    let blocks = preview_blocks("# Heading\n\nbody\n\n## Subheading\n\nbody", 600.0);
    assert_eq!(blocks[0].bounds().height, 80.0); // 40px * 1.8 + decoration inset
    assert_eq!(blocks[2].bounds().height, 71.0); // 35px * 1.8 + decoration inset
    for pair in blocks.windows(2) {
        // Include the two 4px text insets in the content-to-content gap.
        assert_eq!(
            pair[1].bounds().y - pair[0].bounds().y - pair[0].bounds().height + 8.0,
            54.0
        );
    }
}

#[test]
fn wrapped_prose_keeps_ordinary_line_spacing_inside_each_paragraph() {
    let blocks = preview_blocks("alpha beta gamma delta epsilon zeta\n\nnext", 140.0);
    let height = blocks[0].bounds().height - 8.0;
    assert!(height > 36.0);
    assert_eq!(height % 36.0, 0.0);
    assert_eq!(blocks[1].bounds().y - blocks[0].bounds().y - height, 54.0);
}

#[test]
fn blank_lines_inside_preview_code_do_not_get_paragraph_gaps() {
    let blocks = preview_blocks("before\n\n```text\none\n\nthree\n```\n\nafter", 600.0);
    // Three 27px code lines (15px font * 1.8), plus 8px text inset
    // and 7.5px code-surface padding. The blank code line stays ordinary.
    assert_eq!(blocks[1].bounds().height, 96.5);
}

#[test]
fn paragraph_spacing_does_not_turn_tight_list_items_into_paragraph_gaps() {
    for source in [
        "before\n\n1. one\n2. two\n\nafter",
        "before\n\n- one\n- two\n\nafter",
    ] {
        let blocks = preview_blocks(source, 600.0);
        assert_eq!(blocks.len(), 3);
        // Two 36px lines, each with the widget's 8px decoration inset.
        // There must be no additional inter-item paragraph gap.
        assert_eq!(blocks[1].bounds().height, 88.0);
        assert_eq!(blocks[1].bounds().y - blocks[0].bounds().y, 90.0);
        assert_eq!(blocks[2].bounds().y - blocks[1].bounds().y, 134.0);
    }
}

#[test]
fn preview_paragraphs_have_the_same_line_and_gap_rhythm_as_write_mode() {
    let blocks = preview_blocks("first\n\nsecond", 600.0);
    assert_eq!(blocks.len(), 2);
    // 36px prose line + a 54px paragraph separator, in either mode.
    assert_eq!(blocks[1].bounds().y - blocks[0].bounds().y, 90.0);
}
