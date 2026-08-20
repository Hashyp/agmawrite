/// Whether a filesystem event can represent a content or path mutation.
/// Reads are deliberately excluded so consumers can inspect files without
/// creating watcher feedback loops.
pub(crate) fn may_change_file(kind: &notify::EventKind) -> bool {
    !kind.is_access()
}

#[cfg(test)]
mod tests {
    /// Opening a watched file to reload it must not trigger another reload;
    /// mutations still do.
    #[test]
    fn access_events_cannot_change_files() {
        use notify::event::{AccessKind, AccessMode, DataChange, ModifyKind};
        use notify::EventKind;

        assert!(!super::may_change_file(&EventKind::Access(
            AccessKind::Open(AccessMode::Read),
        )));
        assert!(super::may_change_file(&EventKind::Modify(
            ModifyKind::Data(DataChange::Content),
        )));
    }
}
