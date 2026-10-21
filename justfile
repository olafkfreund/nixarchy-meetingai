# lennette — development tasks

default:
    @just --list

# Build the workspace.
build:
    cargo build --workspace

# Run the application.
run:
    cargo run -p lennette

# Run the test suite.
test:
    cargo nextest run --workspace

# Format the workspace.
fmt:
    cargo fmt --all

# Lint with warnings denied.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Full local gate: format, lint, test.
check: fmt lint test
