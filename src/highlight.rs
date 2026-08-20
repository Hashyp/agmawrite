//! Syntax dimming for the write-mode editor: Markdown's own markers —
//! heading hashes, list bullets, checkboxes, ordered numbers, blockquote
//! bars, code fences, and thematic breaks — render in a dimmer grey than
//! the text, so the writing stands out from the syntax.

use std::ops::Range;

use iced::advanced::text::highlighter::{Format, Highlighter};
use iced::{Color, Font, Theme};

/// The dimmed grey Markdown syntax markers render in.
const MARKER_COLOR: Color = Color::from_rgb(0.45, 0.45, 0.45);

/// The amber find matches tint in — the same amber the preview paints
/// them with.
const FIND_MATCH_COLOR: Color = Color::from_rgb(0.95, 0.72, 0.25);

/// A highlighted stretch of a source line: a Markdown syntax marker, or
/// a find match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    Marker,
    FindMatch,
}

/// Turns a highlight into its text format: Markdown markers dim to grey,
/// find matches glow amber.
pub fn format(highlight: &Highlight, _theme: &Theme) -> Format<Font> {
    Format {
        color: match highlight {
            Highlight::Marker => MARKER_COLOR,
            Highlight::FindMatch => FIND_MATCH_COLOR,
        }
        .into(),
        font: None,
    }
}

/// The write-mode highlighter, driven by the find query: Markdown markers
/// dim, and every match of the query tints amber. Stateless like
/// `PlainText`: every line is scanned fresh, so no cache invalidation is
/// needed.
pub struct MarkdownMarkers {
    query: String,
}

impl Highlighter for MarkdownMarkers {
    type Settings = String;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            query: settings.clone(),
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.query = new_settings.clone();
    }

    fn change_line(&mut self, _line: usize) {}

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        line_highlights(line, &self.query).into_iter()
    }

    fn current_line(&self) -> usize {
        usize::MAX
    }
}

/// The highlights of a source line: the byte ranges holding Markdown
/// markers and find matches, sorted by position. A match overlapping a
/// marker wins the stretch they share — the match is what the eye is
/// looking for.
fn line_highlights(line: &str, query: &str) -> Vec<(Range<usize>, Highlight)> {
    let matches = crate::editing::byte_matches(line, query);

    let mut markers: Vec<(Range<usize>, Highlight)> = marker_ranges(line)
        .into_iter()
        .flat_map(|range| {
            subtract(range, &matches)
                .into_iter()
                .map(|range| (range, Highlight::Marker))
                .collect::<Vec<_>>()
        })
        .collect();

    markers.extend(
        matches
            .into_iter()
            .map(|range| (range, Highlight::FindMatch)),
    );
    markers.sort_by_key(|(range, _)| range.start);
    markers
}

/// Removes `cuts` from `range`, returning the leftover sub-ranges in
/// order.
fn subtract(range: Range<usize>, cuts: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut leftovers = Vec::new();
    let mut start = range.start;

    for cut in cuts {
        if cut.end <= start || cut.start >= range.end {
            continue;
        }

        if cut.start > start {
            leftovers.push(start..cut.start);
        }

        start = start.max(cut.end);
    }

    if start < range.end {
        leftovers.push(start..range.end);
    }

    leftovers
}

/// The byte ranges of a source line that hold Markdown syntax markers
/// rather than text.
fn marker_ranges(line: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];

    if rest.is_empty() {
        return ranges;
    }

    // Fenced code block markers: a run of at least three backticks or
    // tildes, optionally followed by an info string.
    for fence in ['`', '~'] {
        let run = rest.bytes().take_while(|&byte| byte == fence as u8).count();

        if run >= 3 {
            ranges.push(indent..indent + run);
            return ranges;
        }
    }

    // Thematic breaks: at least three of the same `-`, `*`, or `_`,
    // whitespace allowed between.
    let significant: Vec<char> = rest.chars().filter(|c| !c.is_whitespace()).collect();

    if significant.len() >= 3
        && matches!(significant[0], '-' | '*' | '_')
        && significant.iter().all(|&c| c == significant[0])
    {
        ranges.push(indent..line.trim_end().len());
        return ranges;
    }

    // Blockquote bars, one range per `>`, each possibly followed by a
    // space; the remainder can still be a heading or a list item.
    let mut offset = indent;
    let mut rest = rest;

    while let Some(after) = rest.strip_prefix('>') {
        ranges.push(offset..offset + 1);
        offset += 1;
        rest = after;

        if let Some(after) = rest.strip_prefix(' ') {
            offset += 1;
            rest = after;
        }
    }

    // Heading hashes: one to six `#` followed by a space.
    let hashes = rest.bytes().take_while(|&byte| byte == b'#').count();

    if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        ranges.push(offset..offset + hashes);
        return ranges;
    }

    // Unordered list bullets, with an optional task-list checkbox.
    if rest.len() >= 2
        && matches!(rest.as_bytes()[0], b'-' | b'*' | b'+')
        && rest.as_bytes()[1] == b' '
    {
        ranges.push(offset..offset + 1);

        let item_offset = offset + 2;
        let item_rest = &rest[2..];

        for checkbox in ["[ ] ", "[x] ", "[X] "] {
            if item_rest.starts_with(checkbox) {
                ranges.push(item_offset..item_offset + 3);
                break;
            }
        }

        return ranges;
    }

    // Ordered list numbers: digits followed by `. ` or `) `.
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();

    if digits > 0 {
        let after = &rest[digits..];

        if after.starts_with(". ") || after.starts_with(") ") {
            ranges.push(offset..offset + digits + 1);
        }
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::Highlight::{FindMatch, Marker};
    use super::{line_highlights, marker_ranges};

    /// The byte ranges of `line` that dim, as slices for readability.
    fn dimmed(line: &str) -> Vec<&str> {
        marker_ranges(line)
            .into_iter()
            .map(|range| &line[range])
            .collect()
    }

    #[test]
    fn dims_heading_hashes() {
        assert_eq!(dimmed("# Title"), ["#"]);
        assert_eq!(dimmed("###### Deep"), ["######"]);
        assert_eq!(dimmed("  ## Indented"), ["##"]);
        // Not headings: no space, or too many hashes.
        assert_eq!(dimmed("#tag").len(), 0);
        assert_eq!(dimmed("####### Too deep").len(), 0);
    }

    #[test]
    fn dims_list_markers_and_checkboxes() {
        assert_eq!(dimmed("- item"), ["-"]);
        assert_eq!(dimmed("* item"), ["*"]);
        assert_eq!(dimmed("+ item"), ["+"]);
        assert_eq!(dimmed("  - nested"), ["-"]);
        assert_eq!(dimmed("- [ ] todo"), ["-", "[ ]"]);
        assert_eq!(dimmed("- [x] done"), ["-", "[x]"]);
        assert_eq!(dimmed("1. first"), ["1."]);
        assert_eq!(dimmed("12) twelfth"), ["12)"]);
        // Not list items: no trailing space.
        assert_eq!(dimmed("-tight").len(), 0);
        assert_eq!(dimmed("1.versioned").len(), 0);
    }

    #[test]
    fn dims_blockquote_bars() {
        assert_eq!(dimmed("> quote"), [">"]);
        assert_eq!(dimmed("> > nested"), [">", ">"]);
        assert_eq!(dimmed(">> nested"), [">", ">"]);
        // A list item inside a quote dims both markers.
        assert_eq!(dimmed("> - item"), [">", "-"]);
        assert_eq!(dimmed("> ## Quoted heading"), [">", "##"]);
    }

    #[test]
    fn dims_fences_and_breaks() {
        assert_eq!(dimmed("```rust"), ["```"]);
        assert_eq!(dimmed("~~~~"), ["~~~~"]);
        assert_eq!(dimmed("`` not a fence").len(), 0);

        assert_eq!(dimmed("---"), ["---"]);
        assert_eq!(dimmed("* * *"), ["* * *"]);
        assert_eq!(dimmed("___"), ["___"]);
        assert_eq!(dimmed("--").len(), 0);
        // A break-like line of mixed chars is not a break.
        assert_eq!(dimmed("-*-").len(), 0);
    }

    #[test]
    fn plain_text_dims_nothing() {
        assert_eq!(dimmed("just text").len(), 0);
        assert_eq!(dimmed("").len(), 0);
        assert_eq!(dimmed("   ").len(), 0);
        assert_eq!(dimmed("text with # hash inside").len(), 0);
    }

    /// Find matches tint their range amber, and a match overlapping a
    /// marker wins the stretch they share; the leftover markers stay
    /// dimmed. Everything comes out sorted by position.
    #[test]
    fn find_matches_tint_and_win_over_markers() {
        let highlights = |line, query| {
            line_highlights(line, query)
                .into_iter()
                .map(|(range, kind)| (line[range].to_owned(), kind))
                .collect::<Vec<_>>()
        };

        // A match alone tints.
        assert_eq!(
            highlights("plain text", "text"),
            vec![("text".to_owned(), FindMatch)]
        );

        // A match covering the marker wins outright.
        assert_eq!(
            highlights("# heading", "#"),
            vec![("#".to_owned(), FindMatch)]
        );

        // A marker beside a match: both survive, in order.
        assert_eq!(
            highlights("- item", "item"),
            vec![("-".to_owned(), Marker), ("item".to_owned(), FindMatch),]
        );

        // A match slicing through the middle of a marker range leaves the
        // marker's leftover stretches dimmed around it.
        assert_eq!(
            highlights("- [x] done", "x"),
            vec![
                ("-".to_owned(), Marker),
                ("[".to_owned(), Marker),
                ("x".to_owned(), FindMatch),
                ("]".to_owned(), Marker),
            ]
        );

        // No query: markers only, exactly as before find existed.
        assert_eq!(highlights("- item", ""), vec![("-".to_owned(), Marker)]);
    }
}
