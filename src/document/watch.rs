use iced::futures::channel::mpsc::Sender;
use iced::Subscription;
use std::path::{Path, PathBuf};

/// A filesystem event concerning the currently opened document.
#[derive(Debug, Clone)]
pub(crate) enum Event {
    ChangedExternally,
}

/// Watches the opened file's directory and reports changes to that file — in
/// write mode and preview alike.
pub(crate) fn subscription(path: &Path) -> Subscription<Event> {
    Subscription::run_with(path.to_path_buf(), |path| {
        let path = path.clone();

        iced::stream::channel(1, move |sender| async move {
            spawn(path, sender);
            // Events arrive on the watcher thread; this runner only keeps the
            // stream alive.
            std::future::pending::<()>().await;
        })
    })
}

/// Spawns the filesystem watcher thread for `path`, forwarding one
/// [`Event::ChangedExternally`] per burst of events touching the file.
/// Editors that save by rename-over are caught by watching the directory
/// rather than the file itself.
fn spawn(path: PathBuf, mut sender: Sender<Event>) {
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};

        let Some(directory) = path.parent().map(Path::to_path_buf) else {
            return;
        };
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(watcher) => watcher,
            Err(error) => {
                eprintln!("agmawrite: cannot watch '{}': {error}", path.display());
                return;
            }
        };

        if let Err(error) = watcher.watch(&directory, RecursiveMode::NonRecursive) {
            eprintln!("agmawrite: cannot watch '{}': {error}", directory.display());
            return;
        }

        while let Ok(event) = rx.recv() {
            let Ok(event) = event else { continue };

            // Reading the file below produces Access events on Linux. If
            // those events trigger another read, the watcher and editor form
            // a feedback loop that consumes an entire CPU core.
            if !crate::watch::may_change_file(&event.kind) {
                continue;
            }

            let touches = event.paths.iter().any(|event_path| {
                event_path == &path
                    || event_path
                        .canonicalize()
                        .is_ok_and(|canonicalized| canonicalized == canonical)
            });

            if !touches {
                continue;
            }

            // A save often produces a burst of events; collapse it into a
            // single reload.
            while rx.try_recv().is_ok() {}

            if !forward_change(&mut sender) {
                break;
            }
        }
    });
}

/// Queues a reload, treating a full one-item channel as an already queued
/// reload rather than as a disconnected watcher.
fn forward_change(sender: &mut Sender<Event>) -> bool {
    match sender.try_send(Event::ChangedExternally) {
        Ok(()) => true,
        Err(error) => error.is_full(),
    }
}

#[cfg(test)]
mod tests {
    use super::forward_change;

    /// Reading a watched document must not trigger another reload, while a
    /// content mutation must still be observed.
    #[test]
    fn access_events_are_ignored() {
        use notify::event::{AccessKind, AccessMode, DataChange, ModifyKind};
        use notify::EventKind;

        assert!(!crate::watch::may_change_file(&EventKind::Access(
            AccessKind::Open(AccessMode::Read),
        )));
        assert!(crate::watch::may_change_file(&EventKind::Modify(
            ModifyKind::Data(DataChange::Content),
        )));
    }

    /// A queued file-change event already represents the latest disk state. A
    /// full channel must therefore coalesce, not kill the watcher.
    #[test]
    fn full_channel_stays_connected() {
        let (mut sender, receiver) = iced::futures::channel::mpsc::channel(0);

        assert!(forward_change(&mut sender));
        assert!(forward_change(&mut sender));

        drop(receiver);
        assert!(!forward_change(&mut sender));
    }
}
