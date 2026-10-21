# lennette — development tasks

default:
    @just --list

# Format all code.
fmt:
    cargo fmt --all

# Check formatting without modifying files.
fmt-check:
    cargo fmt --all -- --check

# Lint with clippy (warnings denied).
lint:
    cargo clippy --all-targets -- -D warnings

# Format and lint together in a single command.
style: fmt lint

# Fast type & borrow checking across all targets.
typecheck:
    cargo check --all-targets

# Fast type & borrow checking alias.
check: typecheck

# Build binaries.
build:
    cargo build

# Run the desktop application.
run:
    cargo run

# Run the unit test suite.
test:
    cargo nextest run

# Full pre-commit check: style + typecheck.
pre-commit: fmt-check lint typecheck

# Full pre-push check: pre-commit + all tests.
pre-push: pre-commit test

# Install repository git hooks into .git/hooks/
install-hooks:
    cp .githooks/pre-push .git/hooks/pre-push
    chmod +x .git/hooks/pre-push
