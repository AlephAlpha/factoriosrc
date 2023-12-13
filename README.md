# factoriosrc

Search for patterns in [Factorio (R3,C2,S2,B3,N+)](https://conwaylife.com/forums/viewtopic.php?f=11&t=6166) cellular automata, using an algorithm similar to [rlifesrc](https://github.com/AlephAlpha/rlifesrc).

This program is still in development. Currently it is still very slow, and the only supported rule is Factorio. There is only an extremely simple command line interface.

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

Currently it simply prints the intermediate results every 100000 steps to the standard output. So there may be a lot of output.

If the search takes too long, you can press `Ctrl+C` to stop.

The program is still work in progress, so the usage may change in the future.
