//! Write-mode editing helpers: what Enter means on a list line.

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

#[cfg(test)]
mod tests {
    use super::continuation;
    use super::Continuation::{Break, Continue, Outdent};

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
}
