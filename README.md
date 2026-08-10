# factoriosrc

Search for patterns in [Factorio (R3,C2,S2,B3,N+)](https://conwaylife.com/forums/viewtopic.php?f=11&t=6166) and other cellular automata, using an algorithm similar to [rlifesrc](https://github.com/AlephAlpha/rlifesrc).

This program is still work in progress. Many features are still missing.

Now it has a text-based UI, an egui desktop GUI, and an egui web UI.

Since 2026, most of the development has been done by AI agents. I don't have time to review all the AI-generated code, so there may be bugs. Please report any issues you find.

## Usage

You can try factoriosrc in your browser without installing anything, at [https://alephalpha.github.io/factoriosrc/](https://alephalpha.github.io/factoriosrc/).

If you want to build and run the program locally, you need to install [Rust](https://rustup.rs/) first.

Build:

```bash
cargo build --release
```

### Text-based UI

Print the top-level help:

```bash
cargo run --bin factoriosrc-tui --release -- --help
```

Print command-specific help:

```bash
cargo run --bin factoriosrc-tui --release -- new --help
```

Search for a c/2 spaceship with [D2-](https://conwaylife.com/wiki/Static_symmetry#D2) symmetry in a bounding box of size 30x10:

```bash
cargo run --bin factoriosrc-tui --release -- new 30 10 2 -x 1 -s D2-
```

Search for a c/3 spaceship in [Hash (R2,C0,S4-6,B5-6,N#)](https://conwaylife.com/forums/viewtopic.php?t=6202) in a bounding box of size 30x8, and save the search state to a file when exiting:

```bash
cargo run --bin factoriosrc-tui --release -- new 30 8 3 -x 1 -r R2,C0,S4-6,B5-6,N# --save save.json
```

Resume the search from the saved state, and save it again when exiting:

```bash
cargo run --bin factoriosrc-tui --release -- load save.json
```

The program is still work in progress, so the usage may change, and the format of the save file may be incompatible between different versions.

In the TUI:

- Use `o` to open the configuration form.
- Use arrow keys or PgUp/PgDn to pan or scroll when the current view does not fit.
- Use the mouse wheel to scroll configuration and help panels, and click in the known-cells editor to set cells.
- Use `c` to copy the current RLE text.

### GUI

The GUI is still work in progress. I have only tested it on Linux. I'm not sure if it works on other platforms.

```bash
cargo run --bin factoriosrc-egui --release
```

Hover controls for concise help text, and use the Help window for longer field reference. Open Known Cells from the configuration sidebar to pin alive and dead cells per generation.

The GUI uses native file dialogs for loading and saving search state. While viewing results, you can hide or show the Config, Details, and History sidebars to leave more room for the RLE view.

On X11, for HiDPI displays, you may need to set the `WINIT_X11_SCALE_FACTOR` environment variable to 2.

### Web UI

The web UI is a port of the GUI that runs entirely in the browser, compiled to WebAssembly. The search runs in a [WebWorker](https://developer.mozilla.org/docs/Web/API/Web_Workers_API), so it continues even when you switch to another tab.

You need to install the `wasm32` target and [Trunk](https://trunkrs.dev/):

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Run the web UI locally:

```bash
cd egui
trunk serve
```

Build the static site (output in `egui/dist/`):

```bash
trunk build --release
```

The web UI uses browser downloads and file picking (or drag & drop) for saving and loading search state, instead of native file dialogs. Save files are compatible with the desktop GUI, but not with the TUI.

## Todo

Features that rlifesrc has but factoriosrc doesn't:

- [x] Improve the performance. Possibly by using some unsafe code.
- [x] Support transformations (rotation and reflection).
- [x] Count the number of living cells.
  - [x] Max population constraint.
  - [x] Dynamically adjust the max population constraint to find the smallest pattern.
- [x] Support more rules.
  - [x] Parse rule strings.
  - [x] Isotropic non-totalistic rules.
  - [x] Non-isotropic non-totalistic rules.
  - [x] Generations rules.
  - [x] Hexagonal rules.
  - [x] Check the symmetry of a rule. So that we can know what symmetries and transformations are compatible with the rule, and whether [the front optimization](docs/front.md) can be applied.
- [x] Support trying a random state for unknown cells.
- [x] Set some cells to be known in the configuration.
- [ ] Fully custom search order, where the user specifies the exact cell-by-cell traversal (for example, serpentine rows or a spiral from the center).
- [x] Save and load the search state.
- [x] GUI.
  - [x] Save and load the search state in the GUI.
- [ ] Web UI.
  - [x] Port the GUI to the web. The search runs in a WebWorker, so it continues in the background without blocking the UI. The web UI is deployed to GitHub Pages automatically from the `main` branch.
  - [ ] Better support for mobile devices.
- [ ] Better documentation.
  - [x] Keep lib, TUI, and GUI field descriptions and help text aligned as the interfaces evolve.

Features that rlifesrc doesn't have and factoriosrc may add:

- [ ] Support searching non-periodic patterns. For example, find a parent of a given pattern.
- [ ] Support more symmetries and transformations. (https://github.com/AlephAlpha/rlifesrc/issues/51)
  - [ ] Support hexagonal symmetries and transformations.
  - [ ] Support custom symmetries and transformations.
  - [ ] Design a DSL for defining symmetries and transformations. We may also use the same DSL for setting known cells.
  - [x] Separate the symmetries and transformations into another crate. (Now `ca-symmetry`, shared by `ca-rules2` and `factoriosrc-lib`.)
- [x] A seedable RNG. (https://github.com/AlephAlpha/rlifesrc/issues/183)
  - [x] Use a RNG with `serde` support, so that we can save and load the random state.
- [ ] More user-friendly UI.
  - [x] Add scrollbars in the TUI.
  - [x] Support mouse input in the TUI, e.g., to choose a field to edit in the config panel.
  - [x] Set cells to be known by clicking, in both the TUI and the GUI.
  - [ ] Automatically save the search state in the browser cache. (https://github.com/AlephAlpha/rlifesrc/issues/366)
- [ ] Set some cells to be known during the search.
- [ ] More. See rlifesrc's issues.

And finally:

- [ ] Merge factoriosrc into rlifesrc.
