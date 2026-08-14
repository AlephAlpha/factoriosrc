// The search worker: loads the same wasm binary as the main page, but instead
// of starting the UI, it starts the search loop. The `init()` call invokes the
// crate's `main`, which detects that it runs inside a worker (no window) and
// returns immediately; we then start the search loop ourselves.
import init, { worker_start } from "./factoriosrc-egui.js";

try {
    await init();
    worker_start();
} catch (e) {
    self.postMessage({ fatal: String(e) + "\n" + (e?.stack ?? "") });
}
