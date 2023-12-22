build:
    cargo build --release

test:
    cargo test

bench: build
    hyperfine --warmup 3 './target/release/factoriosrc-tui -r life 26 8 4 -y 1 -n a --no-tui'