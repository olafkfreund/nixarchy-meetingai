# lennette — development tasks

devenv := if env_var_or_default("DEVENV_PROFILE", "") != "" { "" } else { "devenv shell -- " }
web := "cd web && bun " 

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

# Run the desktop application.
run:
    {{devenv}}cargo run

# Run the unit test suite.
test:
    {{devenv}}cargo nextest run --workspace

# Full pre-commit check: style + typecheck.
pre-commit: fmt-check lint typecheck

# Full pre-push check: pre-commit + all tests.
pre-push: pre-commit test

# Bump version (patch, minor, major, or auto).
bump type="patch":
    {{devenv}}./scripts/bump-version.sh {{type}}

# Install repository git hooks into .git/hooks/
install-hooks:
    lefthook install --force

# Run the landing site dev server.
web-dev:
    cd web && bun run dev

# Format, lint and type check the landing site.
web-check:
    cd web && bun run format
    cd web && bun run lint
    cd web && bun run typecheck

# Run the landing site test suite.
web-check-types:
    cd web && bun run test

# Validate lints, format, types, compile optimized release binary and install locally.
install: fmt-check lint typecheck
    {{devenv}}cargo build --release
    mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/512x512/apps ~/.local/share/icons/hicolor/scalable/apps
    {{devenv}}./scripts/install-binary.sh target/release/lennette ~/.local/bin/lennette
    cp src/assets/lennette_app_icon.png ~/.local/share/icons/hicolor/512x512/apps/dev.lennette.overlay.png
    cp src/assets/lennette_app_icon.png ~/.local/share/icons/hicolor/512x512/apps/lennette.png
    cp src/assets/lennette_app_icon.png ~/.local/share/icons/hicolor/scalable/apps/dev.lennette.overlay.png
    cp src/assets/lennette_app_icon.png ~/.local/share/icons/hicolor/scalable/apps/lennette.png
    rm -f ~/.local/share/applications/lennette.desktop
    cp -f packaging/dev.lennette.overlay.desktop ~/.local/share/applications/dev.lennette.overlay.desktop
    @command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database ~/.local/share/applications || true
    @command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor || true
    @echo "Lennette successfully built and installed to ~/.local/bin/lennette"
