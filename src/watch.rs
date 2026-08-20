/// Whether a filesystem event can represent a content or path mutation.
/// Reads are deliberately excluded so consumers can inspect files without
/// creating watcher feedback loops.
pub(crate) fn may_change_file(kind: &notify::EventKind) -> bool {
    !kind.is_access()
}
