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

# Bump version (patch, minor, major, or auto).
bump type="patch":
    ./scripts/bump-version.sh {{type}}

# Install repository git hooks into .git/hooks/
install-hooks:
    cp .githooks/pre-push .git/hooks/pre-push
    cp .githooks/prepare-commit-msg .git/hooks/prepare-commit-msg
    chmod +x .git/hooks/pre-push .git/hooks/prepare-commit-msg

# Validate lints, format, types, compile optimized release binary and install locally.
install: fmt-check lint typecheck
    cargo build --release
    mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/512x512/apps ~/.local/share/icons/hicolor/scalable/apps
    cp target/release/lennette ~/.local/bin/lennette
    chmod +x ~/.local/bin/lennette
    cp src/assets/lennette_app_icon.png ~/.local/share/icons/hicolor/512x512/apps/dev.lennette.overlay.png
    cp src/assets/lennette_app_icon.png ~/.local/share/icons/hicolor/512x512/apps/lennette.png
    cp src/assets/lennette_app_icon.png ~/.local/share/icons/hicolor/scalable/apps/lennette.png
    rm -f ~/.local/share/applications/lennette.desktop
    cp packaging/dev.lennette.overlay.desktop ~/.local/share/applications/dev.lennette.overlay.desktop
    @command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database ~/.local/share/applications || true
    @command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor || true
    @echo "Lennette successfully built and installed to ~/.local/bin/lennette"
