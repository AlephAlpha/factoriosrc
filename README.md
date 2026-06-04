# factoriosrc

Search for patterns in [Factorio (R3,C2,S2,B3,N+)](https://conwaylife.com/forums/viewtopic.php?f=11&t=6166) and other cellular automata, using an algorithm similar to [rlifesrc](https://github.com/AlephAlpha/rlifesrc).

This program is still work in progress. Many features are still missing.

Now it has a simple text-based UI. A simple GUI is work in progress. A web UI will be added in the future.

Since 2026, most of the development has been done by AI assistants. I don't have time to review all the AI-generated code, so there may be bugs. Please report any issues you find.

## Usage

You need to install [Rust](https://rustup.rs/) first.

Build:

```bash
cargo build --release
```

### Text-based UI

Print the help message:

```bash
cargo run --bin factoriosrc-tui --release -- help
```

Search for a c/2 spaceship with [D2-](https://conwaylife.com/wiki/Static_symmetry#D2) symmetry in a bounding box of size 30x10:

```bash
cargo run --bin factoriosrc-tui --release -- new 30 10 2 -x 1 -s D2-
```

Search for a c/3 spaceship in [Hash (R2,C0,S4-6,B5-6,N#)](https://conwaylife.com/forums/viewtopic.php?f=11&t=6166&start=25#p104000) in a bounding box of size 30x8, and save the search state to a file when exiting:

```bash
cargo run --bin factoriosrc-tui --release -- new 30 8 3 -x 1 -r R2,C0,S4-6,B5-6,N# --save save.json
```

Resume the search from the saved state, and save it again when exiting:

```bash
cargo run --bin factoriosrc-tui --release -- load save.json
```

The program is still work in progress, so the usage may change, and the format of the save file may be incompatible between different versions.

### GUI

The GUI is still work in progress. I have only tested it on Linux. I'm not sure if it works on other platforms.

```bash
cargo run --bin factoriosrc-egui --release
```

Hover the mouse over the labels in the configuration panel to see the help messages.

On X11, for HiDPI displays, you may need to set the `WINIT_X11_SCALE_FACTOR` environment variable to 2.

## Todo

Features that rlifesrc has but factoriosrc doesn't:

- [x] Improve the performance. Possibly by using some unsafe code.
- [x] Support transformations (rotation and reflection).
- [x] Count the number of living cells.
  - [x] Max population constraint.
  - [x] Dynamically adjust the max population constraint to find the smallest pattern.
- [ ] Support more rules.
  - [x] Parse rule strings.
  - [ ] Non-totalistic rules.
  - [ ] Generations rules.
  - [ ] Hexagonal rules.
  - [ ] Check the symmetry of a rule. So that we can know what symmetries and transformations are compatible with the rule, and whether [the front optimization](docs/front.md) can be applied.
  - [ ] Update (or completely rewrite) the [ca-rules](https://crates.io/crates/ca-rules) crate.
- [x] Support trying a random state for unknown cells.
- [ ] Set some cells to be known in the configuration.
  - [x] Support setting known cells in the lib.
  - [x] Support setting known cells in the CLI and the TUI.
  - [ ] Support setting known cells in the GUI.
- [ ] Fully custom search order, where the user specifies the exact cell-by-cell traversal (for example, serpentine rows or a spiral from the center).
- [x] Save and load the search state.
- [x] GUI.
  - [x] Save and load the search state in the GUI.
- [ ] Web UI.
  - [ ] Port the GUI to the web. I'm using the [egui](https://github.com/emilk/egui) library, which has a web backend. I still need to figure out how to use WebWorkers, so that the search can run in the background without blocking the UI.
  - [ ] Better support for mobile devices.
- [ ] Better documentation.

Features that rlifesrc doesn't have and factoriosrc may add:

- [ ] Support searching non-periodic patterns. For example, find a parent of a given pattern.
- [ ] Support more symmetries and transformations. (https://github.com/AlephAlpha/rlifesrc/issues/51)
  - [ ] Support hexagonal symmetries and transformations.
  - [ ] Support custom symmetries and transformations.
  - [ ] Design a DSL for defining symmetries and transformations. We may also use the same DSL for setting known cells.
  - [ ] Separate the symmetries and transformations into another crate.
- [x] A seedable RNG. (https://github.com/AlephAlpha/rlifesrc/issues/183)
  - [x] Use a RNG with `serde` support, so that we can save and load the random state.
- [ ] More user-friendly UI.
  - [ ] Add scrollbars in the TUI.
  - [ ] Support mouse input in the TUI, e.g., to choose a field to edit in the config panel.
  - [ ] Set cells to be known by clicking, in both the TUI and the GUI.
  - [ ] Automatically save the search state in the browser cache. (https://github.com/AlephAlpha/rlifesrc/issues/366)
- [ ] Set some cells to be known during the search.
- [ ] More. See rlifesrc's issues.

And finally:

- [ ] Merge factoriosrc into rlifesrc.
