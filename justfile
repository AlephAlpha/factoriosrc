bin := "./target/release/factoriosrc-tui"
gui := "./target/release/factoriosrc-egui"

# Install the dependencies
init:
    cargo install miniserve
    cargo install hyperfine
    rustup toolchain install nightly
    rustup +nightly component add miri
    cargo +nightly miri setup

# Build the release binary
build:
    cargo build --release

# Run the release binary
run *ARGS: build
    {{bin}} {{ARGS}}

# Run the GUI (WIP)
gui: build
    RUST_LOG=INFO {{gui}}

# Run the tests
test:
    cargo test --package factoriosrc-lib
    cargo test

# Run the tests with Miri
test-miri:
    cargo +nightly miri test --features serde test_miri

# Build and serve the documentation
doc:
    cargo doc
    cd target/doc && miniserve --index index.html

# Show the help message
help: build
    {{bin}} --help

# Run the benchmark
bench: build
    hyperfine --warmup 3 '{{bin}} -r B3/S23 26 8 4 -y 1 -n a --no-tui'

# Run the benchmark, comparing with rlifesrc
bench-compare: build
    hyperfine --warmup 3 '{{bin}} -r B3/S23 26 8 4 -y 1 -n a --no-tui' 'rlifesrc 26 8 4 0 1 --no-tui'
