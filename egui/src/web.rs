//! Web platform glue: runs the search in a WebWorker, and provides browser
//! file loading and downloading for the save/load feature.
//!
//! This module is only compiled for the web target. The UI itself
//! (`App` in `app.rs`) is platform-independent.

use crate::{
    app::AppConfig,
    search::{Event, Message, Search, SearchApi},
};
use factoriosrc_lib::Status;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent, Worker, WorkerOptions, WorkerType};

/// Commands sent from the main thread to the worker.
#[derive(serde::Serialize, serde::Deserialize)]
enum WorkerCommand {
    /// Start a new search with the given configuration.
    InitConfig(AppConfig),
    /// Resume a search from a saved JSON string.
    InitSave(String),
    /// Send an event to the running search.
    Event(Event),
}

// ---------------------------------------------------------------------------
// Main thread

/// A handle to the search running in a WebWorker.
pub struct WebSearchThread {
    /// The worker running the search.
    worker: Worker,
    /// Messages received from the worker, waiting to be picked up by the UI.
    queue: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<Message>>>,
}

impl std::fmt::Debug for WebSearchThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSearchThread").finish_non_exhaustive()
    }
}

thread_local! {
    /// JSON strings loaded from files (picked or dropped), waiting to be
    /// picked up by the app.
    static PENDING_LOADS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

impl WebSearchThread {
    /// Create a new [`WebSearchThread`] from an [`AppConfig`].
    pub fn new(config: AppConfig) -> Self {
        let queue = std::rc::Rc::new(std::cell::RefCell::new(std::collections::VecDeque::new()));
        let worker = spawn_worker(queue.clone(), WorkerCommand::InitConfig(config));
        Self { worker, queue }
    }

    /// Create a new [`WebSearchThread`] from a JSON string.
    ///
    /// This also returns the [`AppConfig`] so that the main thread can
    /// update the UI with the new world configuration.
    pub fn load(s: &str) -> Result<(Self, AppConfig), serde_json::Error> {
        // Validate the save file by trying to load it in the main thread.
        // The worker will load it again later.
        let search = Search::load(s)?;
        let config = AppConfig {
            config: search.world.config().clone(),
            step: search.step,
            increase_world_size: search.increase_world_size,
            no_stop: search.no_stop,
        };

        let queue = std::rc::Rc::new(std::cell::RefCell::new(std::collections::VecDeque::new()));
        let worker = spawn_worker(queue.clone(), WorkerCommand::InitSave(s.to_owned()));

        Ok((Self { worker, queue }, config))
    }
}

impl SearchApi for WebSearchThread {
    /// Send an [`Event`] to the worker.
    fn send(&self, event: Event) {
        let _ = self
            .worker
            .post_message(&command_js(&WorkerCommand::Event(event)));
    }

    /// Try to receive a [`Message`] from the worker without blocking.
    fn try_recv(&self) -> Option<Message> {
        let mut queue = self.queue.borrow_mut();
        let mut last = None;
        for message in queue.drain(..) {
            if !message.is_frame() {
                return Some(message);
            }
            last = Some(message);
        }
        last
    }

    /// Terminate the worker.
    fn terminate(self: Box<Self>) {
        self.worker.terminate();
    }
}

/// Create a worker that pushes incoming messages into the given queue.
///
/// The given init command is not posted immediately: the worker installs its
/// `onmessage` handler asynchronously (after the module is loaded), and
/// messages posted before that may be lost. Instead, we wait for a `READY`
/// message from the worker, which is posted at the end of [`worker_start`].
fn spawn_worker(
    queue: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<Message>>>,
    init_command: WorkerCommand,
) -> Worker {
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker =
        Worker::new_with_options("worker.js", &options).expect("Failed to create the worker");

    let pending_init: std::rc::Rc<std::cell::RefCell<Option<JsValue>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Some(command_js(&init_command))));
    let worker_holder: std::rc::Rc<std::cell::RefCell<Option<Worker>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let worker_holder_for_closure = worker_holder.clone();

    let on_message = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
        let Some(json) = event.data().as_string() else {
            log::warn!("Ignoring a non-string message from the worker.");
            return;
        };
        if json == "READY" {
            if let Some(init) = pending_init.borrow_mut().take()
                && let Some(worker) = worker_holder_for_closure.borrow().as_ref()
            {
                let _ = worker.post_message(&init);
            }
            return;
        }
        log::debug!("Received {} bytes from the worker.", json.len());
        match serde_json::from_str::<Vec<Message>>(&json) {
            Ok(messages) => queue.borrow_mut().extend(messages),
            Err(err) => log::warn!("Failed to parse a message from the worker: {err}"),
        }
    });
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    // Keep the closure alive for the lifetime of the worker.
    on_message.forget();
    *worker_holder.borrow_mut() = Some(worker.clone());

    worker
}

/// Serialize a [`WorkerCommand`] into a JSON string in a `JsValue`.
fn command_js(command: &WorkerCommand) -> JsValue {
    JsValue::from_str(&serde_json::to_string(command).expect("Failed to serialize the command"))
}

/// Open a browser file picker for a `.json` file.
///
/// The result is pushed to the pending loads queue.
pub fn request_load() {
    let window = web_sys::window().expect("No window");
    let document = window.document().expect("No document");

    let input = document
        .create_element("input")
        .expect("Failed to create the file input element")
        .dyn_into::<web_sys::HtmlInputElement>()
        .expect("The created element is not an input element");
    input.set_type("file");
    input.set_accept(".json");
    // The input must be attached to the DOM for `click()` to work.
    let _ = input.style().set_property("display", "none");
    document.body().expect("No body").append_child(&input).ok();

    let input_clone = input.clone();
    let on_change = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
        let Some(file) = input_clone.files().and_then(|files| files.get(0)) else {
            log::warn!("No file was selected.");
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            match read_file(&file).await {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(json) => PENDING_LOADS.with(|pending| pending.borrow_mut().push(json)),
                    Err(_) => log::warn!("The selected file is not valid UTF-8."),
                },
                Err(err) => log::warn!("Failed to read the selected file: {err}"),
            }
        });
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    // Keep the closure alive for as long as the input element may be used.
    on_change.forget();

    input.click();
}

/// Take the `.json` files that were loaded since the last call.
pub fn take_pending_loads() -> Vec<String> {
    PENDING_LOADS.with(|pending| std::mem::take(&mut *pending.borrow_mut()))
}

/// Read any `.json` files dropped onto the canvas, and add them to the pending loads.
pub fn poll_dropped_files(ctx: &egui::Context) {
    let files: Vec<_> = ctx.input(|input| input.raw.dropped_files.clone());
    for file in files {
        let name = file.path().display().to_string();
        let is_json = file
            .path()
            .extension()
            .is_some_and(|extension| extension == "json");
        if !is_json {
            log::info!("Ignoring dropped file {name:?}: not a JSON file.");
            continue;
        }
        wasm_bindgen_futures::spawn_local(async move {
            match file.bytes_async().await {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(json) => PENDING_LOADS.with(|pending| pending.borrow_mut().push(json)),
                    Err(_) => log::warn!("The dropped file {name:?} is not valid UTF-8."),
                },
                Err(err) => log::warn!("Failed to read the dropped file {name:?}: {err}"),
            }
        });
    }
}

/// Download the given JSON string as a `save.json` file.
pub fn download_search_state(json: &str) {
    let window = web_sys::window().expect("No window");
    let document = window.document().expect("No document");

    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(json));
    let blob = web_sys::Blob::new_with_str_sequence(&parts).expect("Failed to create the blob");
    let url =
        web_sys::Url::create_object_url_with_blob(&blob).expect("Failed to create the object URL");

    let anchor = document
        .create_element("a")
        .expect("Failed to create the anchor element")
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .expect("The created element is not an anchor element");
    anchor.set_href(&url);
    anchor.set_download("save.json");
    // The anchor must be attached to the DOM for `click()` to work.
    let _ = document.body().expect("No body").append_child(&anchor);
    anchor.click();
    anchor.remove();

    web_sys::Url::revoke_object_url(&url).expect("Failed to revoke the object URL");
    log::info!("Search state downloaded as save.json");
}

/// Read a file into a byte vector.
async fn read_file(file: &web_sys::File) -> Result<Vec<u8>, String> {
    let array_buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|err| format!("{err:?}"))?;
    Ok(js_sys::Uint8Array::new(&array_buffer).to_vec())
}

// ---------------------------------------------------------------------------
// Worker thread

/// A closure that handles messages arriving from the main thread.
type MessageHandler = Closure<dyn Fn(MessageEvent)>;

/// A closure that drives one pump of the worker loop.
type PumpHandler = Closure<dyn FnMut()>;

thread_local! {
    /// The search currently running in the worker.
    static SEARCH: std::cell::RefCell<Option<Search>> = const { std::cell::RefCell::new(None) };
    /// Commands from the main thread that have not been processed yet.
    static PENDING: std::cell::RefCell<Vec<JsValue>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Keep-alive slot for the `onmessage` handler.
    static ON_MESSAGE: std::cell::RefCell<Option<MessageHandler>> =
        const { std::cell::RefCell::new(None) };
    /// Keep-alive slot for the pump closure, which reschedules itself.
    static PUMP: std::cell::RefCell<Option<PumpHandler>> =
        const { std::cell::RefCell::new(None) };
    /// Timestamp of the last pump, used to adapt the step budget.
    static LAST_PUMP: std::cell::RefCell<f64> = const { std::cell::RefCell::new(0.0) };
}

/// Entry point of the worker. Called by `worker.js` after the module is loaded.
#[wasm_bindgen]
pub fn worker_start() {
    // Redirect `log` messages to the console, just like the main thread.
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();

    let on_message = MessageHandler::new(move |event: MessageEvent| {
        PENDING.with(|pending| pending.borrow_mut().push(event.data()));
    });
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    ON_MESSAGE.with(|slot| *slot.borrow_mut() = Some(on_message));

    let pump = PumpHandler::new(pump_once);
    PUMP.with(|slot| *slot.borrow_mut() = Some(pump));
    LAST_PUMP.with(|timestamp| *timestamp.borrow_mut() = js_sys::Date::now());

    schedule_pump(&scope, 0);

    // Tell the main thread that we are ready to receive the init command.
    let _ = scope.post_message(&JsValue::from_str("READY"));
}

/// Schedule the next pump, keeping the pump closure alive.
fn schedule_pump(scope: &DedicatedWorkerGlobalScope, delay: u32) {
    let js: js_sys::Function = PUMP
        .with(|slot| {
            slot.borrow()
                .as_ref()
                .expect("The pump closure is missing")
                .as_ref()
                .clone()
        })
        .unchecked_into();
    let _ = scope.set_timeout_with_callback_and_timeout_and_arguments_0(&js, delay as i32);
}

/// Process pending commands, step the search, and report the results.
fn pump_once() {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let mut messages = Vec::new();

    // Process the commands that arrived since the last pump.
    let pending: Vec<JsValue> = PENDING.with(|pending| std::mem::take(&mut *pending.borrow_mut()));
    for value in pending {
        let Some(json) = value.as_string() else {
            log::warn!("Ignoring a non-string command from the main thread.");
            continue;
        };
        let Ok(command) = serde_json::from_str::<WorkerCommand>(&json) else {
            log::warn!("Ignoring an unparsable command from the main thread.");
            continue;
        };
        match command {
            WorkerCommand::InitConfig(config) => {
                let search = Search::new(config);
                messages.push(search.snapshot().into());
                SEARCH.with(|slot| *slot.borrow_mut() = Some(search));
            }
            WorkerCommand::InitSave(saved) => match Search::load(&saved) {
                Ok(search) => {
                    messages.push(search.snapshot().into());
                    SEARCH.with(|slot| *slot.borrow_mut() = Some(search));
                }
                Err(err) => log::error!("Failed to load the search state in the worker: {err}"),
            },
            WorkerCommand::Event(event) => {
                let message = SEARCH.with(|slot| {
                    slot.borrow_mut()
                        .as_mut()
                        .map(|search| search.handle_event(event))
                });
                if let Some(message) = message {
                    messages.push(message);
                }
            }
        }
    }

    // Run the search. In the foreground we run exactly one step batch per pump,
    // so that the UI updates at the same rate as on the desktop (one snapshot
    // per batch of `step` search steps). When the gap between pumps is large,
    // e.g. because the tab is hidden and our timers are throttled, we run a
    // longer burst so that the search keeps progressing in the background.
    //
    // The burst must stop as soon as the search is no longer running (e.g. it
    // found a solution or exhausted the search space), and solutions found in
    // the middle of a burst must be reported immediately; otherwise they would
    // be lost, and the search would continue past them.
    let now = js_sys::Date::now();
    let gap = now - LAST_PUMP.with(|timestamp| *timestamp.borrow());
    LAST_PUMP.with(|timestamp| *timestamp.borrow_mut() = now);

    let running = SEARCH.with(|slot| slot.borrow().as_ref().is_some_and(Search::is_running));
    if running {
        if gap > 800.0 {
            let deadline = now + (gap / 2.0).clamp(200.0, 4000.0);
            let mut last_step = None;
            while js_sys::Date::now() < deadline {
                let message =
                    SEARCH.with(|slot| slot.borrow_mut().as_mut().map(Search::step_batch));
                if let Some(message) = message {
                    if matches!(
                        &message,
                        Message::Snapshot(snapshot) if snapshot.status == Status::Solved
                    ) {
                        // Report solutions immediately, so that they are not
                        // lost when the burst continues (e.g. with `no_stop`).
                        last_step = None;
                        messages.push(message);
                    } else {
                        last_step = Some(message);
                    }
                }
                if !SEARCH.with(|slot| slot.borrow().as_ref().is_some_and(Search::is_running)) {
                    break;
                }
            }
            if let Some(message) = last_step {
                messages.push(message);
            }
        } else if let Some(message) =
            SEARCH.with(|slot| slot.borrow_mut().as_mut().map(Search::step_batch))
        {
            messages.push(message);
        }
    }

    if !messages.is_empty() {
        let json = serde_json::to_string(&messages).expect("Failed to serialize the messages");
        let _ = scope.post_message(&JsValue::from_str(&json));
    }

    // Yield to the event loop so that new commands can be received.
    let running = SEARCH.with(|slot| slot.borrow().as_ref().is_some_and(Search::is_running));
    let delay = if running { 0 } else { 100 };
    schedule_pump(&scope, delay);
}
