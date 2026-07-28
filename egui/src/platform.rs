use eframe::NativeOptions;
#[cfg(feature = "save")]
use std::path::{Path, PathBuf};

/// Desktop app title.
pub const APP_TITLE: &str = "factoriosrc";

/// Native window options for the desktop build.
#[must_use]
pub fn native_options() -> NativeOptions {
    NativeOptions::default()
}

/// Pick a file to load search state from.
#[cfg(feature = "save")]
#[must_use]
pub fn pick_load_path() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_file()
}

/// Pick a file to save search state to.
#[cfg(feature = "save")]
#[must_use]
pub fn pick_save_path(default_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_file_name(default_name)
        .save_file()
}

/// Read saved search state from disk.
#[cfg(feature = "save")]
pub fn read_search_state(path: impl AsRef<Path>) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

/// Write saved search state to disk.
#[cfg(feature = "save")]
pub fn write_search_state(path: impl AsRef<Path>, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}
