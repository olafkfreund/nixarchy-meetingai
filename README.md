# lennette

A desktop assistant with live transcription, chat, and meeting notes — built in
pure Rust with [GPUI](https://gpui.rs).

## Layout

```
crates/lennette/src/
├── app.rs                 bootstrap: window + event loop
├── features/
│   ├── domain.rs          pure domain types (mode, size, messages)
│   ├── chat.rs            conversation state
│   ├── overlay.rs         overlay view
│   └── theme.rs           visual tokens
└── lib.rs / main.rs
```

Feature modules own their state and rendering. Anything with rules worth testing
lives in a module with unit tests and no UI dependency.

## Development

The environment is managed by [devenv](https://devenv.sh) and pins Rust 1.99.1
via rust-overlay.

```sh
direnv allow      # once
just build
just run
just check        # fmt + clippy + tests
```

Without direnv: `devenv shell -- just check`.

## Notes

- `ref/` holds the reverse-engineering material used for behavioural reference.
  It is intentionally untracked.
- `vendor/xattr` is a one-line portability patch for a transitive dependency;
  see `vendor/README.md`.
- Development uses a local paywall-free build: no purchase UI, and every local
  capability check passes. Hosted features still require a real, server-verified
  license.
