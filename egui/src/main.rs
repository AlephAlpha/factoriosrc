#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![warn(clippy::nursery)]

#[cfg(not(target_arch = "wasm32"))]
use eframe::Result;
use factoriosrc_egui::App;
#[cfg(not(target_arch = "wasm32"))]
use factoriosrc_egui::{APP_TITLE, native_options};

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    env_logger::init();

    eframe::run_native(
        APP_TITLE,
        native_options(),
        Box::new(|_cc| Ok(Box::<App>::default())),
    )
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;

    // The same wasm binary also runs inside the search WebWorker, where
    // `init()` calls this `main` too. In a worker there is no window, so
    // return immediately; `worker.js` calls `worker_start` instead.
    if web_sys::window().is_none() {
        return;
    }

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_cc| Ok(Box::new(App::default()))),
            )
            .await
            .expect("Failed to start the web app");
    });
}
