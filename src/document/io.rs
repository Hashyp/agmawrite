use std::path::{Path, PathBuf};

/// Opens a Markdown file picker and returns the selected path and contents.
pub(crate) async fn open() -> Option<(PathBuf, String)> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Open Markdown file")
        .pick_file()
        .await?;

    let contents = String::from_utf8_lossy(&file.read().await).into_owned();

    Some((file.path().to_path_buf(), contents))
}

/// Opens a save picker and returns the selected path.
pub(crate) async fn pick_save_path() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Save Markdown file")
        .set_file_name("untitled.md")
        .save_file()
        .await
        .map(|file| file.path().to_path_buf())
}

/// Reads a document from disk, keeping the display-ready error local to
/// document I/O.
pub(crate) fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read '{}': {error}", path.display()))
}

/// Writes `contents` to `path`, returning the path on success. The error is
/// kept as a string so the async result remains cloneable by the app message.
pub(crate) async fn save(path: PathBuf, contents: String) -> Result<PathBuf, String> {
    // Documents opened by the editor are small; a blocking write inside the
    // task is sufficient.
    std::fs::write(&path, contents)
        .map_err(|error| format!("cannot save '{}': {error}", path.display()))?;

    Ok(path)
}
