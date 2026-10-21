# Vendored patches

Minimal, documented patches for transitive dependencies that fail to build with
current toolchains. Each one is scoped to a single targeted fix.

## `xattr` 0.2.3

`xattr` 0.2.3 referenced `libc::ENOATTR`, which newer `libc` releases no longer
expose on Linux (`ENODATA` is the correct value there). The only change is that
constant's definition in `src/sys/mod.rs`.

## `proc-macro-error2` 2.0.1

`proc-macro-error2` 2.0.1 triggered a future incompatibility warning (`E0365: extern crate proc_macro is private and cannot be re-exported`) on newer Rust compilers because `extern crate proc_macro;` on line 277 was private while line 494 did `pub use proc_macro;`. Marked as `pub extern crate proc_macro;` in `src/lib.rs`.
