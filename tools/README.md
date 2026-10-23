# Repository tools

Run from the repository root with Rust 1.99.1 (or inside `devenv shell`). The
`lennette-tools` Cargo workspace member has no GUI/audio build dependencies.

```sh
cargo xtask --help
cargo test --locked -p lennette-tools
cargo xtask bump patch
cargo xtask bump auto "feat(profiles): add interview background"
cargo xtask hyprland-smoke --help
cargo xtask hyprland-overview --help
```

`bump` updates the app version in both Cargo files, preserves formatting and
stages those files. Existing Git hooks and `just bump` use the same Rust command.

`release` is for the main-branch GitHub Actions publish job. It checks both
platforms' files/checksums before any GitHub call, creates/resumes a draft, then
publishes only after all expected assets are uploaded. It uses the authenticated
`gh release view` command to inspect published releases and drafts: the REST
lookup by tag does not return drafts. Its tests use a fake client without network
access. Every release keeps the supplied banner. CI tests this tool on Linux and
Windows.

CI caches Cargo downloads and compiled dependencies for both debug checks and
release builds, separately for each platform and the publishing tool. Compiler
versions, Cargo manifests/lockfiles and Rust flags are included in the cache
keys. A first run still has to populate the caches; cache restoration does not
skip formatting, lint, tests, or the Windows smoke check.

The Hyprland commands are Linux-only. They launch their own nested compositor,
store captures/logs/results in the artifact directory, and stop only their own
process groups. They require a parent Wayland session and the native fixtures;
see [the integration guide](../packaging/hyprland/README.md#test-and-inspect).
Their unit tests cover pixel/PPM validation, argument handling, bounded command
execution and protected binding checks. Running those tests does not claim that
real capture protocols or plugin builds passed; run the native QA command for
that evidence.
