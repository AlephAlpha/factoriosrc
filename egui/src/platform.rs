#[cfg(not(target_arch = "wasm32"))]
use eframe::NativeOptions;
#[cfg(all(feature = "save", not(target_arch = "wasm32")))]
use std::path::{Path, PathBuf};

/// Desktop app title.
pub const APP_TITLE: &str = "factoriosrc";

/// Native window options for the desktop build.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn native_options() -> NativeOptions {
    NativeOptions::default()
}

/// Pick a file to load search state from.
#[cfg(all(feature = "save", not(target_arch = "wasm32")))]
#[must_use]
pub fn pick_load_path() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_file()
}

/// Pick a file to save search state to.
#[cfg(all(feature = "save", not(target_arch = "wasm32")))]
#[must_use]
pub fn pick_save_path(default_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_file_name(default_name)
        .save_file()
}

/// Read saved search state from disk.
#[cfg(all(feature = "save", not(target_arch = "wasm32")))]
pub fn read_search_state(path: impl AsRef<Path>) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

/// Write saved search state to disk.
#[cfg(all(feature = "save", not(target_arch = "wasm32")))]
pub fn write_search_state(path: impl AsRef<Path>, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

/// Open a browser file picker to load a search state.
///
/// The result arrives asynchronously; poll [`take_pending_loads`] for it.
#[cfg(all(target_arch = "wasm32", feature = "save"))]
pub fn request_load() {
    crate::web::request_load();
}

/// Take the `.json` files that were loaded since the last call.
#[cfg(all(target_arch = "wasm32", feature = "save"))]
pub fn take_pending_loads() -> Vec<String> {
    crate::web::take_pending_loads()
}

/// Read any `.json` files dropped onto the canvas.
#[cfg(all(target_arch = "wasm32", feature = "save"))]
pub fn poll_dropped_files(ctx: &egui::Context) {
    crate::web::poll_dropped_files(ctx);
}

/// Trigger a download of the given search state JSON.
#[cfg(all(target_arch = "wasm32", feature = "save"))]
pub fn download_search_state(json: &str) {
    crate::web::download_search_state(json);
}
