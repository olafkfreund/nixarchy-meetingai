# nixarchy-meetingai — development tasks

devenv := if env_var_or_default("DEVENV_PROFILE", "") != "" { "" } else { "devenv shell -- " }

default:
    @just --list

# Format all code.
fmt:
    {{devenv}}cargo fmt --all

# Check formatting without modifying files.
fmt-check:
    {{devenv}}cargo fmt --all -- --check

# Lint with clippy (warnings denied).
lint:
    {{devenv}}cargo clippy --workspace --all-targets -- -D warnings

# Format and lint together in a single command.
style: fmt lint

# Fast type & borrow checking across all targets.
typecheck:
    {{devenv}}cargo check --workspace --all-targets

# Fast type & borrow checking alias.
check: typecheck

# Build binaries.
build:
    {{devenv}}cargo build

# Run the daemon.
run:
    {{devenv}}cargo run

# Run the unit test suite.
test:
    {{devenv}}cargo nextest run --workspace

# Full pre-commit check: style + typecheck.
pre-commit: fmt-check lint typecheck

# Full pre-push check: pre-commit + all tests.
pre-push: pre-commit test

# Install repository git hooks into .git/hooks/
install-hooks:
    lefthook install --force

# Validate lints, format, types, compile optimized release binary and install locally.
install: fmt-check lint typecheck
    {{devenv}}cargo build --release
    mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
    install -m755 target/release/nixarchy-meetingai ~/.local/bin/nixarchy-meetingai
    cp packaging/nixarchy-meetingai.svg ~/.local/share/icons/hicolor/scalable/apps/dev.nixarchy.meetingai.svg
    rm -f ~/.local/share/applications/nixarchy-meetingai.desktop
    cp -f packaging/dev.nixarchy.meetingai.desktop ~/.local/share/applications/dev.nixarchy.meetingai.desktop
    @command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database ~/.local/share/applications || true
    @command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor || true
    @echo "nixarchy-meetingai successfully built and installed to ~/.local/bin/nixarchy-meetingai"
