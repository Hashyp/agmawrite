//! Slow-changing Lualine metadata, read off the UI thread and never during view.

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::Subscription;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Metadata {
    pub(crate) branch: Option<String>,
}

/// Restart on document changes; poll HEAD too, so external branch switches and
/// worktrees update without requiring the document itself to change.
pub(crate) fn subscription(path: Option<PathBuf>) -> Subscription<Metadata> {
    Subscription::run_with(("preview-status-metadata", path), |(_, path)| {
        let path = path.clone();
        iced::stream::channel(
            1,
            move |mut sender: iced::futures::channel::mpsc::Sender<Metadata>| async move {
                std::thread::spawn(move || {
                    let mut previous = None;
                    loop {
                        let metadata = Metadata {
                            branch: path.as_deref().and_then(branch_for_file),
                        };
                        if previous.as_ref() != Some(&metadata) {
                            match sender.try_send(metadata.clone()) {
                                Ok(()) => previous = Some(metadata),
                                Err(error) if error.is_full() => {}
                                Err(_) => break,
                            }
                        }
                        if sender.is_closed() {
                            break;
                        }
                        std::thread::sleep(Duration::from_secs(2));
                    }
                });
                std::future::pending::<()>().await;
            },
        )
    })
}

fn branch_for_file(path: &Path) -> Option<String> {
    let absolute = std::path::absolute(path).ok()?;
    for directory in absolute.parent()?.ancestors() {
        let git = directory.join(".git");
        let git_dir = if git.is_dir() {
            git
        } else if git.is_file() {
            let pointer = std::fs::read_to_string(&git).ok()?;
            directory.join(pointer.trim().strip_prefix("gitdir: ")?)
        } else {
            continue;
        };
        let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
        return head_label(&head);
    }
    None
}

fn head_label(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        return (!branch.is_empty()).then(|| branch.to_owned());
    }
    (head.len() >= 7 && head.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| head[..7].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_keeps_branch_names_with_slashes() {
        assert_eq!(
            head_label("ref: refs/heads/prototype/status\n"),
            Some("prototype/status".into())
        );
    }

    #[test]
    fn detached_head_uses_short_commit() {
        assert_eq!(head_label("abc1234def5678\n"), Some("abc1234".into()));
    }

    #[test]
    fn malformed_head_is_hidden() {
        assert_eq!(head_label("ref: refs/tags/v1"), None);
    }

    #[test]
    fn finds_worktree_head_through_relative_gitdir_pointer() {
        let root =
            std::env::temp_dir().join(format!("agmawrite-status-git-{}", std::process::id()));
        std::fs::create_dir_all(root.join("worktree/nested")).unwrap();
        std::fs::create_dir_all(root.join("metadata")).unwrap();
        std::fs::write(root.join("worktree/.git"), "gitdir: ../metadata\n").unwrap();
        std::fs::write(root.join("metadata/HEAD"), "ref: refs/heads/topic\n").unwrap();
        let branch = branch_for_file(&root.join("worktree/nested/notes.md"));
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(branch.as_deref(), Some("topic"));
    }
}
