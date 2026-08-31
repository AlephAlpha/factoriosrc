# AGENTS.md

## Workspace
- This is a Rust workspace with 4 crates: `factoriosrc-lib` in `lib/` (core search engine), `ca-rules2/` (rule parsing), `factoriosrc-tui` in `tui/` (CLI/TUI), and `factoriosrc-egui` in `egui/` (desktop GUI + web UI).
- `factoriosrc-egui` also compiles to `wasm32-unknown-unknown` with Trunk (`trunk build` inside `egui/`, entry `egui/index.html`). The same binary serves two entry points: the eframe UI on the main thread and the search loop in a WebWorker (`egui/src/web.rs`, wired up by `egui/assets/worker.js`). WebWorker protocol changes must keep the `READY` handshake in `spawn_worker` and the pump loop in `worker_start`/`pump_once` in sync.
- Most behavior changes belong in `factoriosrc-lib`. `World` in `lib/src/world.rs` owns the search state and unsafe cell graph; both frontends wrap it.
- The non-empty front optimization in `lib/src/world.rs` depends on translation/reflection invariants and on the current rule subset (including `B0` rules, where "empty" means "equal to the background state" and the front covers the first `background_period` generations). Read `docs/front.md` before changing `init_front()`, `front_count`, supported rule families, known-cell constraints, symmetry, transformations, or search-order logic.
- `Config::check()` and `Config::parse_rule()` in `lib/src/config.rs` are the source of truth for validation, supported rules, and automatic search-order selection. Update UI/docs after changing them, not the other way around.
- App-level rule support is narrower than `ca-rules2`: the search app currently accepts rules with neighborhood size `<= 24`, with either 2 states or up to 255 states (Generations rules). Isotropic non-totalistic rules must have a range-1 Moore or hexagonal neighborhood (size `<= 8`), and non-isotropic (MAP) rules must have a range-1 Moore, von Neumann, or hexagonal neighborhood.
- `B0` rules are supported: the cells outside the search range follow a uniform periodic background (`RuleTable::background`), and `Config::check()` requires the period to be a multiple of the background period. For a rule with both `B0` and the maximum survival condition (`S-max`, e.g. `S8`), the background is permanently alive and the population counts dead cells instead.

## Verify
- Normal CI uses stable Rust. Only Miri setup/tests need nightly.
- After making changes, run `cargo fmt` to format the code before committing.
- `just test` now matches `.github/workflows/test.yml` and runs, in order: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --package factoriosrc-lib --no-default-features`, `cargo test --all-features`.
- Focused checks: `cargo test -p ca-rules2`, `cargo test -p factoriosrc-lib --no-default-features`, `cargo test -p factoriosrc-lib`, `cargo test --all-features`, `cargo test -p factoriosrc-lib test_miri -- --exact`, `cargo check -p factoriosrc-egui --target wasm32-unknown-unknown`.
- If you touch unsafe search internals (`lib/src/world.rs`, `lib/src/search.rs`, `lib/src/cell.rs`), run Miri after `just init`: `cargo +nightly miri test test_miri`.

## Features
- `factoriosrc-lib` has no default features. The frontends rely on different implicit optional-dependency features: TUI uses `clap` and `serde`; egui uses `documented`; egui's default `save` feature also pulls in `serde`.
- Changes to `Config`, `World`, or public enums need to preserve the `cfg(feature = "...")` derives and serde renames in `lib/src/config.rs`, `lib/src/world.rs`, and `lib/src/symmetry.rs`.
- `factoriosrc-lib` and `ca-rules2` both enable `#![warn(missing_docs)]`; new public API there needs docs or `cargo clippy -- -D warnings` will fail.

## Docs
- `docs/sat-ideas.md` is a living experiment log for SAT-solver-inspired search experiments (opt-in flags: `--backjump`, `--nogood`, `--nogood-translate`, `--phase-saving`, `--lookahead`). When changing these flags or their rule support, update its "Status at a glance" table and the idea's Status subsection in the same change. Follow its "Maintaining this document" section; record benchmark numbers only in its consolidated benchmark tables.

## Runtime Quirks
- `factoriosrc-tui` auto-falls back to non-TUI mode when stdout is not a TTY (`tui/src/main.rs`). In agent/CI shells, `cargo run --bin factoriosrc-tui ...` behaves like `--no-tui`.
- Interactive TUI mode defaults `step` to `100_000` in `tui/src/app.rs`; non-TUI mode only gets periodic output if you pass `--step`.
- egui runs the search on a background thread in `egui/src/search.rs`; keep TUI and egui start/pause/no-stop behavior aligned when changing search control flow.
- TUI and egui save files are not interchangeable: TUI serializes `tui::App`, egui serializes `egui::search::Search`. The README also says save formats may change between versions.
- `just gui` is the quickest GUI debug path because it sets `RUST_LOG=factoriosrc_egui=DEBUG`.
