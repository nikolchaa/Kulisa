# Vendored mlua 0.12.2

Upstream: https://github.com/mlua-rs/mlua (MIT). Vendored only because of the patch
below; drop this directory once an upstream release includes an equivalent fix.

Tests, examples and benches are stripped to keep the repository small.

## Patches

1. `src/luau/heap_dump.rs`: skip the `tmpfile`-based heap dump on
   `wasm32-unknown-unknown`, where the `libc` crate has no stdio bindings.
   `Lua::heap_dump()` returns an error on that target instead of failing to compile.
