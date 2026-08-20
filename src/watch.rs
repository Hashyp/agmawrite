use iced::futures::channel::mpsc::Sender;
use std::path::PathBuf;

/// Whether a filesystem event can represent a content or path mutation.
/// Reads are deliberately excluded so consumers can inspect files without
/// creating watcher feedback loops.
pub(crate) fn may_change_file(kind: &notify::EventKind) -> bool {
    !kind.is_access()
}

/// Runs a directory watcher that forwards one cloned event value per burst.
/// This keeps non-feature-specific watcher mechanics out of the composition
/// root while theme watching still lives there temporarily.
pub(crate) fn spawn_directory_events<Message>(
    directory: PathBuf,
    mut sender: Sender<Message>,
    message: Message,
) where
    Message: Clone + Send + 'static,
{
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};

        if !directory.is_dir() {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(watcher) => watcher,
            Err(_) => return,
        };

        if watcher
            .watch(&directory, RecursiveMode::NonRecursive)
            .is_err()
        {
            return;
        }

        while let Ok(event) = rx.recv() {
            let Ok(event) = event else { continue };

            if !may_change_file(&event.kind) {
                continue;
            }

            // Filesystem changes commonly arrive in bursts; collapse them.
            while rx.try_recv().is_ok() {}

            match sender.try_send(message.clone()) {
                Ok(()) => {}
                Err(error) if error.is_full() => {}
                Err(_) => break,
            }
        }
    });
}
