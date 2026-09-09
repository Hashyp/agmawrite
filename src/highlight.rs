//! Syntax dimming and heading tints for the write-mode editor: Markdown's
//! own markers — block prefixes like heading hashes, list bullets,
//! checkboxes, ordered numbers, blockquote bars, code fences, and thematic
//! breaks, plus the inline punctuation of emphasis, code spans,
//! strikethroughs, links, and images — render in a dimmer grey than the
//! text, so the writing stands out from the syntax. Heading text goes the
//! other way: each level takes the same palette-role tint the preview
//! paints it with, softened to stay legible.

#[cfg(test)]
mod layout_tests;

use std::ops::Range;

use iced::advanced::text::highlighter::{Format, Highlighter};
use iced::{Color, Font, Theme};

use crate::theme::Palette;

/// The marker grey: the theme's foreground eased toward its background —
/// between the omarchy foreground and muted roles, dimmed but legible.
const MARKER_DIM_FACTOR: f32 = 0.45;

/// The write-mode highlighter's settings: the find query and the active
/// omarchy palette. A changed setting restarts the scan, so new matches
/// tint and theme-switched heading tints re-resolve.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    /// The find query whose matches tint amber.
    pub query: String,
    /// The palette resolving heading tints.
    pub palette: Palette,
}

/// A highlighted stretch of a source line: a Markdown syntax marker, a
/// heading's tinted text, or a find match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Highlight {
    /// Markdown punctuation, dimmed.
    Marker,
    /// Heading text, tinted with its level's palette role — the same
    /// resolution the preview paints the heading with, resolved against
    /// the settings' palette when the line was highlighted.
    Heading(Color),
    /// A match of the find query, in the theme's warning yellow — the
    /// same amber family the preview tints them with.
    FindMatch,
}

/// Turns a highlight into its text format. The runtime theme carries the
/// omarchy palette: markers dim toward its background, find matches paint
/// in its warning yellow. Headings arrive pre-resolved against the
/// palette the settings carried — the theme's five roles cannot recover
/// the omarchy ones, and the format hook is a plain function.
pub fn format(highlight: &Highlight, theme: &Theme) -> Format<Font> {
    let roles = theme.palette();

    Format {
        color: match highlight {
            Highlight::Marker => dimmed(roles.text, roles.background),
            Highlight::Heading(color) => *color,
            Highlight::FindMatch => roles.warning,
        }
        .into(),
        font: None,
    }
}

/// `text` eased toward `background` by [`MARKER_DIM_FACTOR`], per channel.
fn dimmed(text: Color, background: Color) -> Color {
    let mix = |t: f32, b: f32| t + (b - t) * MARKER_DIM_FACTOR;

    Color::from_rgb(
        mix(text.r, background.r),
        mix(text.g, background.g),
        mix(text.b, background.b),
    )
}

/// A fenced code block currently open around a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fence {
    marker: char,
    len: usize,
}

/// The write-mode highlighter, driven by its settings: Markdown markers
/// dim, heading text takes its level's palette tint, and every match of
/// the find query tints amber.
///
/// The editor feeds lines in document order and rewinds with
/// [`change_line`] after an edit, so the highlighter tracks how far it has
/// read and which fence is open — only that way can a line know whether it
/// sits inside a code block. Settings changes (the find query or the
/// palette) restart the scan, because every already-highlighted line must
/// be re-fed with the new matches and tints.
pub struct MarkdownMarkers {
    query: String,
    palette: Palette,
    /// The index of the next line the editor will feed.
    next_line: usize,
    /// The fence open at the start of each fed line: entry `i` is the
    /// fence state entering line `i`, so the vector always holds
    /// `next_line + 1` snapshots.
    fences: Vec<Option<Fence>>,
}

impl Highlighter for MarkdownMarkers {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            query: settings.query.clone(),
            palette: settings.palette,
            next_line: 0,
            fences: vec![None],
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.query = new_settings.query.clone();
        self.palette = new_settings.palette;
        self.next_line = 0;
        self.fences = vec![None];
    }

    fn change_line(&mut self, line: usize) {
        // Never skip unvisited lines: an edit far below the viewport still
        // needs the intervening fences replayed before its layout is known.
        self.next_line = line.min(self.next_line);
        self.fences.truncate(self.next_line + 1);
    }

    fn line_height_scale(&self, line: &str) -> f32 {
        if line.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
            && self.fences.last().copied().flatten().is_none()
        {
            crate::typography::PARAGRAPH_GAP_SCALE
        } else {
            1.0
        }
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let entering = self.fences.last().copied().flatten();
        let highlights = line_highlights(line, &self.query, entering, &self.palette);
        self.fences.push(fence_after(line, entering));
        self.next_line += 1;
        highlights.into_iter()
    }

    fn current_line(&self) -> usize {
        self.next_line
    }
}

/// The fence open after `line`, given the fence open before it: a run of
/// at least three backticks or tildes opens a fence, and a later run of
/// the same character at least as long closes it.
fn fence_after(line: &str, entering: Option<Fence>) -> Option<Fence> {
    if let Some(fence) = entering {
        return closing_fence(line, fence).is_none().then_some(fence);
    }
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return None;
    }

    for marker in ['`', '~'] {
        let run = rest.chars().take_while(|&c| c == marker).count();
        if run >= 3 && (marker != '`' || !rest[run..].contains('`')) {
            return Some(Fence { marker, len: run });
        }
    }

    None
}

/// The highlights of a source line: the byte ranges holding Markdown
/// markers, heading tints, and find matches, sorted by position. Inside a
/// fenced code block only the closing fence marker and find matches
/// highlight — code is prose to the writer, not syntax. A match
/// overlapping a marker or a heading tint wins the stretch they share —
/// the match is what the eye is looking for.
fn line_highlights(
    line: &str,
    query: &str,
    fence: Option<Fence>,
    palette: &Palette,
) -> Vec<(Range<usize>, Highlight)> {
    let matches = crate::editing::byte_matches(line, query);

    // The heading tint: the role the preview would paint this heading
    // with, resolved against the active palette. Code fences have no
    // headings — code is content, not structure.
    let heading = match fence {
        Some(_) => None,
        None => heading_span(line).map(|(level, span)| (span, palette.heading_color(level))),
    };

    let markers: Vec<Range<usize>> = match fence {
        Some(fence) => closing_fence(line, fence).into_iter().collect(),
        None => marker_ranges(line),
    };

    let mut highlights: Vec<(Range<usize>, Highlight)> = Vec::new();

    for range in markers {
        // Punctuation inside a heading belongs to its tinted text —
        // like the preview, the heading reads as one colored unit.
        if heading
            .as_ref()
            .is_some_and(|(span, _)| span.contains(&range.start))
        {
            continue;
        }

        highlights.extend(
            subtract(range, &matches)
                .into_iter()
                .map(|range| (range, Highlight::Marker)),
        );
    }

    if let Some((span, color)) = heading {
        highlights.extend(
            subtract(span, &matches)
                .into_iter()
                .map(|range| (range, Highlight::Heading(color))),
        );
    }

    highlights.extend(
        matches
            .into_iter()
            .map(|range| (range, Highlight::FindMatch)),
    );
    highlights.sort_by_key(|(range, _)| range.start);
    highlights
}

/// The level and text span of a line's heading, if it is one: one to six
/// `#` after blockquote bars, then a space, then the text to the end of
/// the line. The hashes dim as markers; the text takes the level's tint,
/// so the write mode reads like the preview.
fn heading_span(line: &str) -> Option<(usize, Range<usize>)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let (offset, rest) = after_bars(line, indent, &mut Vec::new());

    let hashes = rest.bytes().take_while(|&byte| byte == b'#').count();

    if !(1..=6).contains(&hashes) || !rest[hashes..].starts_with(' ') {
        return None;
    }

    let start =
        offset + hashes + (rest[hashes..].len() - rest[hashes..].trim_start_matches(' ').len());
    let end = line.trim_end().len();

    (start < end).then_some((hashes, start..end))
}

/// Walks a line's blockquote bars, pushing one range per `>` (each
/// possibly followed by a space), and returning the offset and rest of
/// the line after them — the remainder can still be a heading or a list
/// item. Marker dimming and heading tinting share the walk so the two
/// never disagree about where a line's prefixes end.
fn after_bars<'a>(
    line: &'a str,
    indent: usize,
    ranges: &mut Vec<Range<usize>>,
) -> (usize, &'a str) {
    let mut offset = indent;
    let mut rest = &line[indent..];

    while let Some(after) = rest.strip_prefix('>') {
        ranges.push(offset..offset + 1);
        offset += 1;
        rest = after;

        if let Some(after) = rest.strip_prefix(' ') {
            offset += 1;
            rest = after;
        }
    }

    (offset, rest)
}

/// The marker range of the line closing `fence`, if it does: a run of the
/// fence's own character at least as long as the opening one.
fn closing_fence(line: &str, fence: Fence) -> Option<Range<usize>> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let run = rest.chars().take_while(|&c| c == fence.marker).count();

    (indent <= 3
        && run >= fence.len
        && run >= 3
        && rest[run..].bytes().all(|byte| matches!(byte, b' ' | b'\t')))
    .then(|| indent..indent + run)
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
/// rather than text: the block prefixes of the line, then its inline
/// punctuation.
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
    let (offset, rest) = after_bars(line, indent, &mut ranges);

    // Heading hashes: one to six `#` followed by a space.
    let hashes = rest.bytes().take_while(|&byte| byte == b'#').count();

    if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        ranges.push(offset..offset + hashes);
    } else if rest.len() >= 2
        && matches!(rest.as_bytes()[0], b'-' | b'*' | b'+')
        && rest.as_bytes()[1] == b' '
    {
        // Unordered list bullets, with an optional task-list checkbox.
        ranges.push(offset..offset + 1);

        let item_offset = offset + 2;
        let item_rest = &rest[2..];

        for checkbox in ["[ ] ", "[x] ", "[X] "] {
            if item_rest.starts_with(checkbox) {
                ranges.push(item_offset..item_offset + 3);
                break;
            }
        }
    } else {
        // Ordered list numbers: digits followed by `. ` or `) `.
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();

        if digits > 0 {
            let after = &rest[digits..];

            if after.starts_with(". ") || after.starts_with(") ") {
                ranges.push(offset..offset + digits + 1);
            }
        }
    }

    ranges.extend(inline_ranges(line));
    ranges.sort_by_key(|range| range.start);
    ranges
}

/// The byte ranges of a line's inline Markdown punctuation: code-span
/// backticks, emphasis and strikethrough delimiters, and link or image
/// brackets.
fn inline_ranges(line: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();

    let code = code_spans(&chars);

    let mut ranges = Vec::new();
    for (span, run) in &code {
        ranges.push(span.start..span.start + run);
        ranges.push(span.end - run..span.end);
    }

    let spans: Vec<Range<usize>> = code.iter().map(|(span, _)| span.clone()).collect();
    ranges.extend(delimiters(&chars, &spans));
    ranges.extend(links(&chars, &spans));
    ranges.sort_by_key(|range| range.start);
    ranges
}

/// Whether the byte `offset` sits inside any of `ranges`.
fn within(offset: usize, ranges: &[Range<usize>]) -> bool {
    ranges.iter().any(|range| range.contains(&offset))
}

/// Whether the character at `index` is escaped by a preceding backslash.
fn escaped(chars: &[(usize, char)], index: usize) -> bool {
    index > 0 && chars[index - 1].1 == '\\'
}

fn is_whitespace(character: char) -> bool {
    character.is_whitespace()
}

/// Markdown treats everything that is neither a letter, a digit, nor
/// whitespace as punctuation — including its own marker characters.
fn is_punctuation(character: char) -> bool {
    !character.is_alphanumeric() && !character.is_whitespace()
}

fn is_punctuation_or_whitespace(character: char) -> bool {
    is_punctuation(character) || is_whitespace(character)
}

/// A delimiter run is left-flanking when it could open emphasis: not
/// followed by whitespace, and not followed by punctuation unless it also
/// follows whitespace or punctuation.
fn flanks_left(before: Option<char>, after: Option<char>) -> bool {
    match after {
        Some(next) if !is_whitespace(next) => {
            !is_punctuation(next) || before.is_none_or(is_punctuation_or_whitespace)
        }
        _ => false,
    }
}

/// A delimiter run is right-flanking when it could close emphasis: not
/// preceded by whitespace, and not preceded by punctuation unless it also
/// precedes whitespace or punctuation.
fn flanks_right(before: Option<char>, after: Option<char>) -> bool {
    match before {
        Some(previous) if !is_whitespace(previous) => {
            !is_punctuation(previous) || after.is_none_or(is_punctuation_or_whitespace)
        }
        _ => false,
    }
}

/// The code spans of a line, as `(span, run)` pairs: `span` covers the
/// whole span — delimiters and content, so later scans can leave it
/// alone — while `run` is the backtick length, identifying the dimmed
/// delimiter stretches at the span's ends. An opening run pairs with the
/// next run of the same length; the code between the delimiters stays
/// prose-colored, being content rather than syntax.
fn code_spans(chars: &[(usize, char)]) -> Vec<(Range<usize>, usize)> {
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();

    let mut index = 0;
    while index < chars.len() {
        if chars[index].1 != '`' {
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() && chars[index].1 == '`' {
            index += 1;
        }

        if !escaped(chars, start) {
            runs.push((chars[start].0, chars[index - 1].0 + 1, index - start));
        }
    }

    let mut spans = Vec::new();
    let mut pending: Vec<usize> = Vec::new();

    for (index, run) in runs.iter().enumerate() {
        if let Some(open) = pending.iter().position(|&pending| runs[pending].2 == run.2) {
            let open = pending.remove(open);
            spans.push((runs[open].0..run.1, run.2));
        } else {
            pending.push(index);
        }
    }

    spans
}

/// The delimiter runs of emphasis, strong emphasis, and strikethrough —
/// runs of one to three `*` or `_`, or exactly two `~` — paired into
/// dimmed opener/closer ranges.
///
/// Pairing follows CommonMark's flanking rules in a simplified form:
/// `*` and `~~` open when left-flanking and close when right-flanking,
/// while `_` additionally refuses to open or close between word
/// characters, so `file_name_here` never dims. An opener pairs with the
/// nearest later closer of the same character and length.
fn delimiters(chars: &[(usize, char)], code: &[Range<usize>]) -> Vec<Range<usize>> {
    struct Delimiter {
        start: usize,
        end: usize,
        marker: char,
        len: usize,
        opens: bool,
        closes: bool,
    }

    let mut runs: Vec<Delimiter> = Vec::new();

    let mut index = 0;
    while index < chars.len() {
        let (offset, character) = chars[index];

        if !matches!(character, '*' | '_' | '~') || within(offset, code) {
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() && chars[index].1 == character {
            index += 1;
        }

        let len = index - start;

        if !((character != '~' && (1..=3).contains(&len)) || (character == '~' && len == 2)) {
            continue;
        }

        if escaped(chars, start) {
            continue;
        }

        let before = start.checked_sub(1).map(|previous| chars[previous].1);
        let after = chars.get(index).map(|&(_, character)| character);
        let left = flanks_left(before, after);
        let right = flanks_right(before, after);

        let (opens, closes) = match character {
            '*' | '~' => (left, right),
            '_' => (
                left && (!right || before.is_some_and(is_punctuation)),
                right && (!left || after.is_some_and(is_punctuation)),
            ),
            _ => unreachable!("only markdown delimiter characters reach pairing"),
        };

        runs.push(Delimiter {
            start: offset,
            end: chars[index - 1].0 + 1,
            marker: character,
            len,
            opens,
            closes,
        });
    }

    let mut ranges = Vec::new();
    let mut pending: Vec<usize> = Vec::new();

    for (index, run) in runs.iter().enumerate() {
        if run.closes {
            if let Some(open) = pending.iter().position(|pending| {
                runs[*pending].marker == run.marker && runs[*pending].len == run.len
            }) {
                let open = pending.remove(open);
                ranges.push(runs[open].start..runs[open].end);
                ranges.push(run.start..run.end);
                continue;
            }
        }

        if run.opens {
            pending.push(index);
        }
    }

    ranges
}

/// The punctuation of inline links, reference links, and images: `[`,
/// `](` or `][`, `)`, and the `!` that marks an image. The address and
/// the bracketed text stay prose-colored — only the syntax dims.
fn links(chars: &[(usize, char)], code: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let (offset, character) = chars[index];

        if character != '[' || within(offset, code) || escaped(chars, index) {
            index += 1;
            continue;
        }

        let Some(close) = (index + 1..chars.len())
            .find(|&candidate| chars[candidate].1 == ']' && !escaped(chars, candidate))
        else {
            break;
        };

        let following = chars.get(close + 1).map(|&(_, character)| character);

        // The separator (`](` or `][`) and the terminator that ends the
        // address or label — a `)` for inline links, a `]` for reference
        // ones.
        let terminator = match following {
            Some('(') => (close + 2..chars.len())
                .find(|&candidate| chars[candidate].1 == ')' && !escaped(chars, candidate)),
            Some('[') => (close + 2..chars.len())
                .find(|&candidate| chars[candidate].1 == ']' && !escaped(chars, candidate)),
            _ => None,
        };

        match terminator {
            Some(end) => {
                let start = if index > 0 && chars[index - 1].1 == '!' {
                    chars[index - 1].0
                } else {
                    offset
                };

                ranges.push(start..chars[index].0 + 1);
                ranges.push(chars[close].0..chars[close + 1].0 + 1);
                ranges.push(chars[end].0..chars[end].0 + 1);

                index = end + 1;
            }
            None => index = close + 1,
        }
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::dimmed as dimmed_color;
    use super::Highlight::{FindMatch, Heading, Marker};
    use super::{
        closing_fence, fence_after, format, heading_span, line_highlights, marker_ranges, Fence,
        Highlighter as _, MarkdownMarkers, Settings,
    };
    use crate::theme::Palette;
    use iced::{Color, Font};

    /// The test settings: a query with the default palette.
    fn settings(query: &str) -> Settings {
        Settings {
            query: query.to_owned(),
            palette: Palette::default(),
        }
    }

    /// Highlight formats paint with omarchy roles: markers dim the
    /// foreground toward the background, find matches take the warning
    /// yellow — never a hardcoded grey or amber. Headings carry their
    /// resolved tint through untouched.
    #[test]
    fn formats_follow_the_runtime_palette() {
        let theme = Palette::default().runtime_theme();
        let roles = theme.palette();

        let marker = format(&Marker, &theme);
        assert_eq!(
            marker.color,
            Some(dimmed_color(roles.text, roles.background))
        );

        let find = format(&FindMatch, &theme);
        assert_eq!(find.color, Some(roles.warning));
        assert_eq!(find.font, None::<Font>);

        let tint = Color::from_rgb(0.2, 0.4, 0.6);
        let heading = format(&Heading(tint), &theme);
        assert_eq!(heading.color, Some(tint));
        assert_eq!(heading.font, None::<Font>);
    }

    /// The marker grey eases the foreground toward the background per
    /// channel: unchanged at 0, the background at 1, halfway between at
    /// 0.45.
    #[test]
    fn dimming_eases_each_channel() {
        let text = Color::from_rgb(1.0, 0.0, 0.5);
        let background = Color::from_rgb(0.0, 1.0, 0.5);

        let dim = dimmed_color(text, background);
        assert!((dim.r - 0.55).abs() < 1e-6);
        assert!((dim.g - 0.45).abs() < 1e-6);
        assert!((dim.b - 0.5).abs() < 1e-6);
    }

    /// The byte ranges of `line` that dim, as slices for readability.
    fn dimmed(line: &str) -> Vec<&str> {
        marker_ranges(line)
            .into_iter()
            .map(|range| &line[range])
            .collect()
    }

    /// The highlights of `line`, as slices paired with kinds.
    fn highlights<'a>(
        line: &'a str,
        query: &str,
        palette: &Palette,
    ) -> Vec<(&'a str, super::Highlight)> {
        line_highlights(line, query, None, palette)
            .into_iter()
            .map(|(range, kind)| (&line[range], kind))
            .collect()
    }

    /// A heading's text takes its level's role color — the same tint the
    /// preview paints it with — while the hashes keep their dimmed grey.
    #[test]
    fn tints_heading_text_with_the_level_role() {
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

        // The expected tints, mixed by hand from the fixture roles.
        let h1 = Color::from_rgb(1.0, 0.35, 0.35);
        let h2 = Color::from_rgb(0.4, 1.0, 0.4);
        let h6 = Color::from_rgb(1.0, 0.88, 0.88);
        let close = |a: Color, b: Color| {
            (a.r - b.r).abs() < 1e-5 && (a.g - b.g).abs() < 1e-5 && (a.b - b.b).abs() < 1e-5
        };

        // The level tint, extracted from the line's highlights.
        let tint_of = |line: &str| {
            line_highlights(line, "", None, &palette)
                .into_iter()
                .find_map(|(_, kind)| match kind {
                    Heading(color) => Some(color),
                    _ => None,
                })
                .unwrap_or(Color::BLACK)
        };

        assert!(close(tint_of("# Title"), h1));
        assert!(close(tint_of("###### Deep"), h6));
        assert!(close(tint_of("  ## Indented"), h2));
        assert!(close(tint_of("> ## Quoted heading"), h2));

        // Only the hashes dim: the separating spaces and the text beyond
        // them carry no other highlights, and trailing whitespace stays
        // outside the tint.
        assert_eq!(
            highlights("# Title", "", &palette),
            vec![("#", Marker), ("Title", Heading(palette.heading_color(1))),]
        );
        assert_eq!(
            highlights("  ## Indented  ", "", &palette),
            vec![
                ("##", Marker),
                ("Indented", Heading(palette.heading_color(2))),
            ]
        );

        // Quoted headings tint their text beyond the dimmed bar and
        // hashes.
        assert_eq!(
            highlights("> ## Quoted heading", "", &palette),
            vec![
                (">", Marker),
                ("##", Marker),
                ("Quoted heading", Heading(palette.heading_color(2))),
            ]
        );

        // Not headings: no space after the hashes, or too many of them.
        assert_eq!(highlights("#tag", "", &palette), vec![]);
        assert_eq!(highlights("####### Too deep", "", &palette), vec![]);
        assert_eq!(highlights("# ", "", &palette), vec![("#", Marker)]);
    }

    /// Punctuation inside a heading belongs to its tinted text — the
    /// heading reads as one colored unit, like the preview.
    #[test]
    fn heading_text_swallows_its_inline_markers() {
        let palette = Palette::default();

        assert_eq!(
            highlights("# a *b* `c` d", "", &palette),
            vec![
                ("#", Marker),
                ("a *b* `c` d", Heading(palette.heading_color(1))),
            ]
        );
    }

    /// A find match inside heading text wins the stretch it shares; the
    /// leftover text keeps its level tint.
    #[test]
    fn find_matches_win_over_heading_tints() {
        let palette = Palette::default();

        assert_eq!(
            highlights("# Title", "Ti", &palette),
            vec![
                ("#", Marker),
                ("Ti", FindMatch),
                ("tle", Heading(palette.heading_color(1))),
            ]
        );
    }

    /// Inside a code fence there are no headings: the line is code,
    /// prose to the writer, so nothing tints.
    #[test]
    fn fenced_headings_stay_prose() {
        let mut highlighter = MarkdownMarkers::new(&settings(""));

        let opening: Vec<_> = highlighter.highlight_line("```").collect();
        assert_eq!(opening, vec![(0..3, Marker)]);

        let inside: Vec<_> = highlighter.highlight_line("# not a heading").collect();
        assert!(inside.is_empty());
    }

    /// A palette change is a settings change: the scan restarts and the
    /// same heading re-resolves against the new theme.
    #[test]
    fn palette_changes_reresolve_heading_tints() {
        let default = settings("");
        let mut highlighter = MarkdownMarkers::new(&default);

        let before: Vec<_> = highlighter.highlight_line("# Title").collect();
        assert_eq!(
            before,
            vec![
                (0..1, Marker),
                (2..7, Heading(default.palette.heading_color(1))),
            ]
        );

        highlighter.update(&Settings {
            query: String::new(),
            palette: Palette {
                foreground: Color::BLACK,
                ..Palette::default()
            },
        });

        let after: Vec<_> = highlighter.highlight_line("# Title").collect();
        assert_ne!(before, after);
        assert_eq!(
            after,
            vec![(0..1, Marker), (2..7, Heading(Color::BLACK))],
            "an invisible tint falls back to the foreground"
        );
    }

    /// Heading spans give their level and the byte range of the text:
    /// after the hashes and their separating spaces, before trailing
    /// whitespace. Quoted headings start past their bars.
    #[test]
    fn heading_spans_find_their_level_and_text() {
        assert_eq!(heading_span("# Title"), Some((1, 2..7)));
        assert_eq!(heading_span("###### Deep"), Some((6, 7..11)));
        assert_eq!(heading_span("  ## Indented"), Some((2, 5..13)));
        assert_eq!(heading_span("> ### Quoted"), Some((3, 6..12)));
        assert_eq!(heading_span("# Title  "), Some((1, 2..7)));

        // Not headings: no space after the hashes, too many of them, or
        // a different block prefix in the way.
        assert_eq!(heading_span("#"), None);
        assert_eq!(heading_span("# "), None);
        assert_eq!(heading_span("#\ttitle"), None);
        assert_eq!(heading_span("#tag"), None);
        assert_eq!(heading_span("####### Too deep"), None);
        assert_eq!(heading_span("plain text"), None);
        assert_eq!(heading_span("- # not nested"), None);
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
    fn dims_emphasis_delimiters() {
        assert_eq!(dimmed("*emphasis*"), ["*", "*"]);
        assert_eq!(dimmed("**strong**"), ["**", "**"]);
        assert_eq!(dimmed("***all***"), ["***", "***"]);
        assert_eq!(dimmed("_under_"), ["_", "_"]);
        assert_eq!(dimmed("__dunder__"), ["__", "__"]);
        assert_eq!(dimmed("mix *mid* word"), ["*", "*"]);
        assert_eq!(dimmed("a*b*c intraword"), ["*", "*"]);
        assert_eq!(dimmed("**bold** and *em*"), ["**", "**", "*", "*"]);

        // Intraword underscores are identifiers, not emphasis.
        assert_eq!(dimmed("file_name_here").len(), 0);
        // Unpaired delimiters are not emphasis; "* dangling" is a list
        // bullet and belongs to the block markers instead.
        assert_eq!(dimmed("*italic").len(), 0);
        assert_eq!(dimmed("x * spaced *").len(), 0);
        // Escaped delimiters are literal characters.
        assert_eq!(dimmed("\\*literal\\*").len(), 0);
    }

    #[test]
    fn dims_strikethrough_tildes() {
        assert_eq!(dimmed("~~gone~~"), ["~~", "~~"]);
        assert_eq!(dimmed("a ~~b~~ c"), ["~~", "~~"]);
        // A single tilde is not a strikethrough.
        assert_eq!(dimmed("~single~").len(), 0);
    }

    #[test]
    fn dims_code_span_backticks() {
        assert_eq!(dimmed("`code`"), ["`", "`"]);
        assert_eq!(dimmed("say `hi` twice"), ["`", "`"]);
        assert_eq!(dimmed("``two`` and `one`"), ["``", "``", "`", "`"]);
        // Unclosed backticks pair with nothing.
        assert_eq!(dimmed("unclosed `tick").len(), 0);
        // The code between the delimiters stays prose-colored.
        let ranges = marker_ranges("`*not emphasis*`");
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn dims_link_punctuation() {
        assert_eq!(dimmed("[label](https://agma.dev)"), ["[", "](", ")"]);
        assert_eq!(dimmed("![image](photo.png)"), ["![", "](", ")"]);
        assert_eq!(dimmed("see [ref][1] here"), ["[", "][", "]"]);
        assert_eq!(dimmed("[**bold**](url)"), ["[", "**", "**", "](", ")"]);

        // Brackets without an address are not links.
        assert_eq!(dimmed("see [1] in the text").len(), 0);
        assert_eq!(dimmed("[unclosed").len(), 0);
        // Escaped brackets are literal.
        assert_eq!(dimmed("\\[not a link\\]").len(), 0);
    }

    #[test]
    fn plain_text_dims_nothing() {
        assert_eq!(dimmed("just text").len(), 0);
        assert_eq!(dimmed("").len(), 0);
        assert_eq!(dimmed("   ").len(), 0);
        assert_eq!(dimmed("text with # hash inside").len(), 0);
        assert_eq!(dimmed("2 * 3 * 4 = x").len(), 0);
    }

    /// Find matches tint their range amber, and a match overlapping a
    /// marker wins the stretch they share; the leftover markers stay
    /// dimmed. Everything comes out sorted by position.
    #[test]
    fn find_matches_tint_and_win_over_markers() {
        let palette = Palette::default();
        let highlights = |line, query| {
            line_highlights(line, query, None, &palette)
                .into_iter()
                .map(|(range, kind)| (line[range].to_owned(), kind))
                .collect::<Vec<_>>()
        };

        // A match alone tints.
        assert_eq!(
            highlights("plain text", "text"),
            vec![("text".to_owned(), FindMatch)]
        );

        // A match covering the marker wins outright; the heading text
        // beyond the match keeps its level tint.
        assert_eq!(
            highlights("# heading", "#"),
            vec![
                ("#".to_owned(), FindMatch),
                ("heading".to_owned(), Heading(palette.heading_color(1))),
            ]
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

    /// A fenced code block opens with a backtick or tilde run and closes
    /// with a same-character run at least as long; other lines leave the
    /// state untouched.
    #[test]
    fn fences_open_close_and_ignore_strangers() {
        let backtick = Fence {
            marker: '`',
            len: 3,
        };

        assert_eq!(
            fence_after("```rust", None),
            Some(backtick),
            "an opening fence remembers its character and length"
        );
        assert_eq!(
            fence_after("code *inside*", Some(backtick)),
            Some(backtick),
            "content lines keep the fence open"
        );
        assert_eq!(
            fence_after("```", Some(backtick)),
            None,
            "an equal run closes the fence"
        );
        assert_eq!(
            fence_after("````", Some(backtick)),
            None,
            "a longer run closes the fence too"
        );
        assert_eq!(
            fence_after("``", Some(backtick)),
            Some(backtick),
            "a shorter run does not"
        );
        assert_eq!(
            fence_after("~~~", Some(backtick)),
            Some(backtick),
            "a tilde run does not close a backtick fence"
        );

        let tilde = Fence {
            marker: '~',
            len: 4,
        };
        assert_eq!(fence_after("~~~~ md", None), Some(tilde));
        assert_eq!(fence_after("plain text", Some(tilde)), Some(tilde));
    }

    /// Inside a fence only the closing marker and find matches highlight:
    /// code is content, so its asterisks and brackets stay prose-colored.
    #[test]
    fn fenced_lines_dim_only_their_closing_marker() {
        let fence = Fence {
            marker: '`',
            len: 3,
        };

        let inside = |line, query| {
            line_highlights(line, query, Some(fence), &Palette::default())
                .into_iter()
                .map(|(range, kind)| (&line[range], kind))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            inside("* not emphasis", ""),
            vec![],
            "code content never dims"
        );
        assert_eq!(
            inside("# not a heading", ""),
            vec![],
            "block prefixes do not apply inside code"
        );
        assert_eq!(inside("```", ""), vec![("```", Marker)]);

        // Find matches still tint inside fences.
        assert_eq!(
            inside("let value = x;", "value"),
            vec![("value", FindMatch)]
        );

        // A tilde run never closes a backtick fence.
        assert_eq!(inside("~~~", ""), vec![]);

        assert_eq!(
            line_highlights("```rust", "", None, &Palette::default()),
            vec![(0..3, Marker)]
        );
        assert_eq!(
            closing_fence("not a fence", fence),
            None,
            "ordinary lines do not close"
        );
    }

    /// The highlighter follows the editor's feeding protocol: lines arrive
    /// in order, `current_line` reports the next one expected,
    /// `change_line` rewinds to an edit, and settings changes restart the
    /// scan from the top.
    #[test]
    fn tracks_the_line_the_editor_will_feed_next() {
        let settings = settings("");
        let mut highlighter = MarkdownMarkers::new(&settings);
        assert_eq!(highlighter.current_line(), 0);

        highlighter.highlight_line("# heading");
        highlighter.highlight_line("plain");
        assert_eq!(highlighter.current_line(), 2);

        highlighter.change_line(1);
        assert_eq!(highlighter.current_line(), 1);
        highlighter.highlight_line("edited");
        assert_eq!(highlighter.current_line(), 2);

        // An edit beyond the fed lines must not skip unknown fence context.
        highlighter.change_line(9);
        assert_eq!(highlighter.current_line(), 2);
        highlighter.highlight_line("first unvisited line");
        assert_eq!(highlighter.current_line(), 3);

        highlighter.update(&settings);
        assert_eq!(highlighter.current_line(), 0);
    }

    /// Feeding a document in order dims fence markers and leaves the code
    /// between them alone; a rewind into the block restores the same
    /// highlights.
    #[test]
    fn tracks_fences_across_fed_lines() {
        let settings = settings("");
        let mut highlighter = MarkdownMarkers::new(&settings);

        let opening: Vec<_> = highlighter.highlight_line("```rust").collect();
        assert_eq!(opening, vec![(0..3, Marker)]);

        let content: Vec<_> = highlighter.highlight_line("let x = a * b;").collect();
        assert!(content.is_empty(), "code content stays prose-colored");

        let closing: Vec<_> = highlighter.highlight_line("```").collect();
        assert_eq!(closing, vec![(0..3, Marker)]);

        let after: Vec<_> = highlighter.highlight_line("*emphasis* again").collect();
        assert_eq!(after, vec![(0..1, Marker), (9..10, Marker)]);

        // Rewinding into the block replays the same fence state.
        highlighter.change_line(1);
        let replay: Vec<_> = highlighter.highlight_line("let y = c * d;").collect();
        assert!(replay.is_empty());
    }

    /// The real editor drives the highlighter through iced's protocol:
    /// `update` lays the text out, `highlight` feeds lines in order, and
    /// blank lines come out taller — the paragraph gap. This is the
    /// end-to-end guard for the bug that kept `current_line` at
    /// `usize::MAX`, which made the editor skip highlighting entirely.
    #[test]
    fn the_editor_feeds_lines_and_spaces_paragraphs() {
        use iced::advanced::graphics::text::Editor as RenderEditor;
        use iced::advanced::text::editor::Editor as _;
        use iced::advanced::text::Highlighter as _;
        use iced::advanced::text::{LineHeight, Wrapping};
        use iced::{Font, Pixels, Size};

        // Three paragraphs — five lines, two of them blank separators.
        let mut editor = RenderEditor::with_text("# title\n\nparagraph one\n\nparagraph two");
        let mut highlighter = MarkdownMarkers::new(&settings(""));

        editor.update(
            Size::new(600.0, 400.0),
            Font::MONOSPACE,
            Pixels(20.0),
            LineHeight::Relative(1.8),
            Wrapping::default(),
            &mut highlighter,
        );

        // Ordinary lines: 5 x 36px, blanks included until highlighted.
        assert_eq!(highlighter.current_line(), 0);
        assert!((editor.min_bounds().height - 180.0).abs() < 0.5);

        let theme = Palette::default().runtime_theme();
        editor.highlight(Font::MONOSPACE, &mut highlighter, |highlight| {
            format(highlight, &theme)
        });

        // The editor fed the visible lines through the highlighter.
        assert!(
            highlighter.current_line() >= 5,
            "every line of the document was fed, got {}",
            highlighter.current_line()
        );

        // The paragraph gap: blank lines now lay out 1.5x taller,
        // 3 x 36px + 2 x 54px.
        assert!((editor.min_bounds().height - 216.0).abs() < 0.5);
    }
}
