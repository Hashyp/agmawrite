use std::path::{Path, PathBuf};

const FEATURE_ROOTS: &[&str] = &[
    "src/document",
    "src/preview",
    "src/comments",
    "src/find",
    "src/input",
    "src/help.rs",
];

fn rust_sources(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return (path.extension().and_then(|extension| extension.to_str()) == Some("rs"))
            .then(|| path.to_path_buf())
            .into_iter()
            .collect();
    }

    let mut sources = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
        .map(|entry| entry.expect("cannot read source directory entry").path())
        .collect();
    entries.sort();

    for entry in entries {
        sources.extend(rust_sources(&entry));
    }

    sources
}

fn has_forbidden_app_dependency(source: &str) -> bool {
    let code: String = source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect();
    let compact: String = code
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();

    compact.contains("crate::app")
        || compact.split(';').any(|statement| {
            let Some(group) = statement
                .rfind("usecrate::{")
                .map(|start| &statement[start + "usecrate::{".len()..])
            else {
                return false;
            };

            group
                .split([',', '{', '}'])
                .any(|import| import == "app" || import.starts_with("app::"))
        })
}

fn is_under_line_limit(source: &str, exclusive_limit: usize) -> bool {
    source.lines().count() < exclusive_limit
}

#[test]
fn feature_and_input_modules_do_not_depend_on_the_app() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();

    for feature in FEATURE_ROOTS {
        for source_path in rust_sources(&root.join(feature)) {
            let source = std::fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", source_path.display()));

            if has_forbidden_app_dependency(&source) {
                violations.push(
                    source_path
                        .strip_prefix(root)
                        .unwrap_or(&source_path)
                        .display()
                        .to_string(),
                );
            }
        }
    }

    assert!(
        violations.is_empty(),
        "feature/input modules must not depend on crate::app: {}",
        violations.join(", ")
    );
}

#[test]
fn composition_files_stay_within_the_agreed_line_limits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for (relative, limit) in [("src/main.rs", 50), ("src/app/mod.rs", 500)] {
        let source = std::fs::read_to_string(root.join(relative))
            .unwrap_or_else(|error| panic!("cannot read {relative}: {error}"));
        let lines = source.lines().count();

        assert!(
            is_under_line_limit(&source, limit),
            "{relative} has {lines} lines; it must stay below {limit}"
        );
    }
}

#[test]
fn library_and_binary_targets_are_built() {
    // Referencing the public library entry point proves this integration test
    // is linked against the library target. Cargo only defines this variable
    // after it has also built the named binary target.
    let _library_entry: fn(Vec<String>) -> iced::Result = agmawrite::run;
    let binary = Path::new(env!("CARGO_BIN_EXE_agmawrite"));

    assert!(
        binary.is_file(),
        "binary target was not built: {}",
        binary.display()
    );
}

#[test]
fn architecture_predicates_reject_forbidden_and_boundary_examples() {
    assert!(has_forbidden_app_dependency("use crate::app::Message;"));
    assert!(has_forbidden_app_dependency(
        "use crate::app::{App, Message};"
    ));
    assert!(has_forbidden_app_dependency(
        "use crate::{comments, app::Message};"
    ));
    assert!(has_forbidden_app_dependency("use crate::\n app::Message;"));
    assert!(!has_forbidden_app_dependency(
        "use crate::preview::Message; // crate::app is only commentary"
    ));

    let forty_nine_lines = "line\n".repeat(49);
    let fifty_lines = "line\n".repeat(50);
    assert!(is_under_line_limit(&forty_nine_lines, 50));
    assert!(!is_under_line_limit(&fifty_lines, 50));
}
