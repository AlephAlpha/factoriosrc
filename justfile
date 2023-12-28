bin := "./target/release/factoriosrc-tui"

# Build the release binary
build:
    cargo build --release

# Run the release binary
run *ARGS: build
    {{bin}} {{ARGS}}

# Run the tests
test:
    cargo test

# Build and serve the documentation
doc:
    cargo doc
    cd target/doc && miniserve --index index.html

# Run the benchmark
bench: build
    hyperfine --warmup 3 '{{bin}} -r life 26 8 4 -y 1 -n a --no-tui'

# Run the benchmark, comparing with rlifesrc
bench-compare: build
    hyperfine --warmup 3 '{{bin}} -r life 26 8 4 -y 1 -n a --no-tui' 'rlifesrc 26 8 4 0 1 --no-tui'
