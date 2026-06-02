bin := "./target/release/factoriosrc-tui"
gui := "./target/release/factoriosrc-egui"

# Install the dependencies
init:
    if [ ! -x "$(command -v miniserve)" ]; then cargo install miniserve; fi
    if [ ! -x "$(command -v hyperfine)" ]; then cargo install hyperfine; fi
    rustup toolchain install nightly
    rustup +nightly component add miri
    cargo +nightly miri setup

# Build the release binary
build:
    cargo build --release

# Run the release binary (TUI)
run *ARGS: build
    {{ bin }} {{ ARGS }}

# Run the GUI (WIP)
gui: build
    RUST_LOG=factoriosrc_egui=DEBUG {{ gui }}

# Run linting and formatting checks
lint:
    cargo fmt --check
    cargo clippy -- -D warnings

# Run the local validation baseline
test:
    just lint
    cargo test --package factoriosrc-lib --no-default-features
    cargo test --all-features

# Run the tests with Miri
test-miri:
    cargo +nightly miri test test_miri

# Build and serve the documentation
doc:
    cargo doc
    cd target/doc && miniserve --index index.html

# Show the help message
help: build
    {{ bin }} --help

# Run the benchmark
bench: build
    hyperfine --warmup 3 '{{ bin }} --no-tui new -r B3/S23 26 8 4 -y 1 -n a'

# Run the benchmark, comparing with rlifesrc
bench-compare: build
    hyperfine --warmup 3 '{{ bin }} --no-tui new -r B3/S23 26 8 4 -y 1 -n a' 'rlifesrc 26 8 4 0 1 --no-tui'
