mod app;
mod help;
mod platform;
mod search;
mod snapshot;
mod theme;
mod ui;
#[cfg(all(target_arch = "wasm32", feature = "save"))]
mod web;

// The web build serializes search state between the main thread and the
// worker, so it requires the `save` feature (enabled by default).
#[cfg(all(target_arch = "wasm32", not(feature = "save")))]
compile_error!("The factoriosrc-egui web build requires the `save` feature.");

pub use app::App;
pub use platform::APP_TITLE;
#[cfg(not(target_arch = "wasm32"))]
pub use platform::native_options;
