# Vendored patches

Minimal, documented patches for transitive dependencies that fail to build with
current toolchains. Each one is scoped to a single targeted fix.

## `xattr` 0.2.3

`xattr` 0.2.3 referenced `libc::ENOATTR`, which newer `libc` releases no longer
expose on Linux (`ENODATA` is the correct value there). The only change is that
constant's definition in `src/sys/mod.rs`.
