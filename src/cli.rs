/// Command-line arguments used to configure the editor at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Args {
    pub(crate) path: Option<String>,
    pub(crate) preview: bool,
}

/// A successfully parsed command line either starts the application or prints
/// help without booting it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParseOutcome {
    Run(Args),
    Help,
}

pub(crate) const USAGE: &str = "Usage: agmawrite [FILE] [--preview]

Arguments:
  FILE         Path to a Markdown file to open

Options:
  --preview    Open FILE in preview-only mode; editing and switching to
               write mode are disabled
  -h, --help   Print this message";

/// Parses command-line arguments without printing or terminating the process.
pub(crate) fn parse_args(argv: Vec<String>) -> Result<ParseOutcome, String> {
    if argv.iter().any(|arg| arg == "-h" || arg == "--help") {
        return Ok(ParseOutcome::Help);
    }

    let mut args = Args {
        path: None,
        preview: false,
    };

    for arg in argv {
        match arg.as_str() {
            "--preview" => args.preview = true,
            path if !path.starts_with('-') => {
                if args.path.replace(path.to_string()).is_some() {
                    return Err("unexpected extra file argument".to_string());
                }
            }
            other => return Err(format!("unexpected argument '{other}'")),
        }
    }

    if args.preview && args.path.is_none() {
        return Err("--preview requires a FILE".to_string());
    }

    Ok(ParseOutcome::Run(args))
}

#[cfg(test)]
mod tests {
    use super::{parse_args, Args, ParseOutcome};

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn no_arguments_runs_with_defaults() {
        assert_eq!(
            parse_args(Vec::new()),
            Ok(ParseOutcome::Run(Args {
                path: None,
                preview: false,
            }))
        );
    }

    #[test]
    fn one_file_runs_in_write_mode() {
        assert_eq!(
            parse_args(strings(&["notes.md"])),
            Ok(ParseOutcome::Run(Args {
                path: Some("notes.md".to_string()),
                preview: false,
            }))
        );
    }

    #[test]
    fn file_and_preview_are_accepted_in_either_order() {
        let expected = Ok(ParseOutcome::Run(Args {
            path: Some("notes.md".to_string()),
            preview: true,
        }));

        assert_eq!(parse_args(strings(&["notes.md", "--preview"])), expected);
        assert_eq!(parse_args(strings(&["--preview", "notes.md"])), expected);
    }

    #[test]
    fn preview_requires_a_file() {
        assert_eq!(
            parse_args(strings(&["--preview"])),
            Err("--preview requires a FILE".to_string())
        );
    }

    #[test]
    fn duplicate_file_arguments_are_rejected() {
        assert_eq!(
            parse_args(strings(&["one.md", "two.md"])),
            Err("unexpected extra file argument".to_string())
        );
    }

    #[test]
    fn unknown_flags_are_rejected() {
        assert_eq!(
            parse_args(strings(&["--unknown"])),
            Err("unexpected argument '--unknown'".to_string())
        );
    }

    #[test]
    fn short_and_long_help_return_help_outcome() {
        assert_eq!(parse_args(strings(&["-h"])), Ok(ParseOutcome::Help));
        assert_eq!(parse_args(strings(&["--help"])), Ok(ParseOutcome::Help));
    }
}
