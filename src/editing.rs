//! Write-mode editing helpers: what Enter means on a list line, and
//! finding text in the document.

use iced::widget::text_editor::Position;
use unicode_segmentation::UnicodeSegmentation;

/// What pressing Enter does, based on the line before the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Continuation {
    /// A plain line break.
    Break,
    /// Continue a list: break the line, then insert this prefix
    /// (e.g. `"- "`, `"2. "`, `"- [ ] "`), indent included.
    Continue(String),
    /// The list item is empty: remove this many characters (the marker)
    /// before breaking the line, ending the list.
    Outdent(usize),
}

/// Decides what Enter does at the end of `line_before_cursor`.
pub fn continuation(before: &str) -> Continuation {
    let indent_len = before.len() - before.trim_start_matches([' ', '\t']).len();
    let indent = &before[..indent_len];
    let rest = &before[indent_len..];

    // The marker's length in `rest` and the prefix it continues with on
    // the next line.
    let (marker_len, next): (usize, String) = 'marker: {
        // Unordered bullet, possibly with a task-list checkbox.
        if rest.len() >= 2
            && matches!(rest.as_bytes()[0], b'-' | b'*' | b'+')
            && rest.as_bytes()[1] == b' '
        {
            let bullet = rest.as_bytes()[0] as char;
            let item = &rest[2..];

            for checkbox in ["[ ] ", "[x] ", "[X] "] {
                if item.starts_with(checkbox) {
                    // A continued task always starts unchecked.
                    break 'marker ("- [ ] ".len(), format!("{bullet} [ ] "));
                }
            }

            break 'marker (2, format!("{bullet} "));
        }

        // Ordered list item: digits + ". " or ") ", the number incremented.
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();

        if digits > 0 {
            let after = &rest[digits..];

            for delimiter in [". ", ") "] {
                if after.starts_with(delimiter) {
                    let number: usize = rest[..digits].parse().unwrap_or(0);
                    break 'marker (digits + 2, format!("{}{}", number + 1, delimiter));
                }
            }
        }

        return Continuation::Break;
    };

    if rest[marker_len..].trim().is_empty() {
        // An empty item: Enter removes the marker and ends the list.
        Continuation::Outdent(before.chars().count())
    } else {
        Continuation::Continue(format!("{indent}{next}"))
    }
}

/// The byte ranges of all occurrences of `query` in `text`,
/// ASCII-case-insensitively. (Non-ASCII bytes compare exactly, which keeps
/// matching byte-index exact.)
fn byte_matches(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let haystack = text.as_bytes();
    let needle = query.as_bytes();
    let mut ranges = Vec::new();

    if needle.is_empty() || needle.len() > haystack.len() {
        return ranges;
    }

    for start in 0..=haystack.len() - needle.len() {
        let end = start + needle.len();

        if haystack[start..end].eq_ignore_ascii_case(needle)
            && text.is_char_boundary(start)
            && text.is_char_boundary(end)
        {
            ranges.push(start..end);
        }
    }

    ranges
}

/// All occurrences of `query` in `text` as grapheme column ranges — the
/// units the preview caret, selection, and match highlighting use.
pub fn matches_in(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    byte_matches(text, query)
        .into_iter()
        .map(|range| {
            let start = text[..range.start].graphemes(true).count();
            let end = start + text[range].graphemes(true).count();
            start..end
        })
        .collect()
}

/// The first occurrence of `query` in `text` as `(line, start column, end
/// column)`, the columns counted in characters — the units the source
/// editor's cursor uses. Single-line queries only match within a line.
pub fn first_match(text: &str, query: &str) -> Option<(usize, usize, usize)> {
    let range = byte_matches(text, query).into_iter().next()?;

    let line = text[..range.start].matches('\n').count();
    let line_start = text[..range.start].rfind('\n').map_or(0, |index| index + 1);
    let start_column = text[line_start..range.start].chars().count();
    let end_column = start_column + text[range].chars().count();

    Some((line, start_column, end_column))
}

/// The source editor position of a byte `offset`: its line and its
/// character column — the units the source editor's cursor uses. Offsets
/// past the end clamp to the end of the text.
pub fn position_at(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let line = text[..offset].matches('\n').count();
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let column = text[line_start..offset].chars().count();

    Position { line, column }
}

#[cfg(test)]
mod tests {
    use super::Continuation::{Break, Continue, Outdent};
    use super::{continuation, first_match, matches_in, position_at};
    use iced::widget::text_editor::Position;

    #[test]
    fn continues_bullets_with_their_indent() {
        assert_eq!(continuation("- item"), Continue("- ".to_owned()));
        assert_eq!(continuation("* item"), Continue("* ".to_owned()));
        assert_eq!(continuation("+ item"), Continue("+ ".to_owned()));
        assert_eq!(continuation("  - nested"), Continue("  - ".to_owned()));
    }

    #[test]
    fn continues_tasks_unchecked() {
        assert_eq!(continuation("- [ ] todo"), Continue("- [ ] ".to_owned()));
        assert_eq!(continuation("- [x] done"), Continue("- [ ] ".to_owned()));
        assert_eq!(continuation("* [X] done"), Continue("* [ ] ".to_owned()));
    }

    #[test]
    fn continues_ordered_lists_incrementing() {
        assert_eq!(continuation("1. first"), Continue("2. ".to_owned()));
        assert_eq!(continuation("9) nine"), Continue("10) ".to_owned()));
        assert_eq!(continuation("  4. nested"), Continue("  5. ".to_owned()));
    }

    #[test]
    fn empty_items_end_the_list() {
        assert_eq!(continuation("- "), Outdent(2));
        assert_eq!(continuation("  - "), Outdent(4));
        assert_eq!(continuation("- [ ] "), Outdent(6));
        assert_eq!(continuation("3. "), Outdent(3));
    }

    #[test]
    fn plain_lines_break_plainly() {
        assert_eq!(continuation("plain"), Break);
        assert_eq!(continuation(""), Break);
        assert_eq!(continuation("> quote"), Break);
        assert_eq!(continuation("# heading"), Break);
        // Not markers without the trailing space.
        assert_eq!(continuation("-tight"), Break);
        assert_eq!(continuation("1.versioned"), Break);
    }

    #[test]
    fn finds_all_matches_as_grapheme_ranges() {
        assert_eq!(matches_in("one two ONE", "one"), vec![0..3, 8..11]);
        assert_eq!(matches_in("one two", "three").len(), 0);
        assert_eq!(matches_in("one", "").len(), 0);
        assert_eq!(matches_in("one", "one two").len(), 0);
        // ASCII case folds, multibyte characters compare exactly — their
        // bytes never false-match ASCII needle bytes.
        assert_eq!(matches_in("naïve NAÏVE", "naïve"), vec![0..5]);
        assert_eq!(matches_in("ONE one", "one"), vec![0..3, 4..7]);
    }

    #[test]
    fn finds_the_first_match_as_char_positions() {
        assert_eq!(first_match("hello world", "wor"), Some((0, 6, 9)));
        assert_eq!(first_match("one\ntwo\nthree", "t"), Some((1, 0, 1)));
        assert_eq!(first_match("nothing here", "zzz"), None);
        assert_eq!(first_match("abc", ""), None);
    }

    /// Byte offsets map onto the line and character column the source
    /// editor's cursor uses; columns count characters, and offsets past
    /// the end clamp to the text's end.
    #[test]
    fn maps_byte_offsets_to_editor_positions() {
        assert_eq!(position_at("hello\nworld", 0), Position { line: 0, column: 0 });
        assert_eq!(position_at("hello\nworld", 6), Position { line: 1, column: 0 });
        assert_eq!(position_at("héllo\nworld", 8), Position { line: 1, column: 1 });
        assert_eq!(position_at("hello", 99), Position { line: 0, column: 5 });
    }
}
