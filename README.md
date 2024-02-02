# factoriosrc

Search for patterns in [Factorio (R3,C2,S2,B3,N+)](https://conwaylife.com/forums/viewtopic.php?f=11&t=6166) cellular automata, using an algorithm similar to [rlifesrc](https://github.com/AlephAlpha/rlifesrc).

This program is still work in progress. The only supported rules are Factorio and Conway's Game of Life. Many features are still missing.

Now it has a simple text-based UI. A web UI will be added in the future.

## Usage

You need to install [Rust](https://rustup.rs/) first.

Build:

```bash
cargo build --release
```

Print the help message:

```bash
cargo run --release -- --help
```

Search for a c/2 spaceship with [D2-](https://conwaylife.com/wiki/Static_symmetry#D2) symmetry in a bounding box of size 30x10:

```bash
cargo run --release -- 30 10 2 -x 1 -s "D2-"
```

The program is still work in progress, so the usage may change in the future.

## Todo

Features that rlifesrc has but factoriosrc doesn't:

- [x] Improve the performance. Possibly by using some unsafe code.
- [ ] Support transformations (rotation and reflection).
- [x] Count the number of living cells.
  - [x] Max population constraint.
  - [x] Dynamically adjust the max population constraint to find the smallest pattern.
- [ ] Support more rules.
  - [x] Parse rule strings.
  - [ ] Non-totalistic rules.
  - [ ] Generations rules.
  - [ ] Hexagonal rules.
  - [ ] Check the symmetry of a rule.
  - [ ] Update (or completely rewrite) the [ca-rules](https://crates.io/crates/ca-rules) crate.
- [x] Support trying a random state for unknown cells.
- [ ] Set some cells to be known in the configuration.
- [ ] Custom search order.
- [ ] Save and load the search state.
- [ ] Web UI.

Features that rlifesrc doesn't have and factoriosrc may add:

- [ ] Support searching non-periodic patterns. For example, find a parent of a given pattern.
- [ ] Support more symmetries and transformations. (https://github.com/AlephAlpha/rlifesrc/issues/51)
  - [ ] Support hexagonal symmetries and transformations.
  - [ ] Support custom symmetries and transformations. Maybe describe them using a DSL.
  - [ ] Separate the symmetries and transformations into another crate.
- [x] A seedable RNG. (https://github.com/AlephAlpha/rlifesrc/issues/183)
  - [ ] Use a RNG with `serde` support, so that we can save and load the random state.
- [ ] More user-friendly TUI and web UI.
  - [ ] Set cells to be known by clicking.
  - [ ] Automatically save the search state in the browser cache. (https://github.com/AlephAlpha/rlifesrc/issues/366)
- [ ] More. See rlifesrc's issues.

And finally:

- [ ] Merge factoriosrc into rlifesrc.
