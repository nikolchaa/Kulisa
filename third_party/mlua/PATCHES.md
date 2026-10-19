# Vendored mlua 0.12.2

Upstream: https://github.com/mlua-rs/mlua (MIT). Vendored only because of the patch
below; drop this directory once an upstream release includes an equivalent fix.

Tests, examples and benches are stripped to keep the repository small.

CI (`.github/scripts/check-vendored-mlua.py`, lint job) downloads the published
crate, applies `patches/*.patch` and fails if this directory differs in any other
way. To change the vendored code, edit the files here *and* regenerate the patch.

To upgrade: `python3 .github/scripts/check-vendored-mlua.py --update <version>`. It
downloads that release, re-applies `patches/`, and says what to run next. If a patch
no longer applies because upstream fixed the issue, delete the patch (and this
directory plus the `[patch.crates-io]` entry once no patches remain).

## Patches

1. `patches/0001-heap-dump-wasm32-unknown.patch`, `src/luau/heap_dump.rs`: skip the `tmpfile`-based heap dump on
   `wasm32-unknown-unknown`, where the `libc` crate has no stdio bindings.
   `Lua::heap_dump()` returns an error on that target instead of failing to compile.
   Every other target compiles the unmodified code (the change is behind
   `cfg(all(target_arch = "wasm32", target_os = "unknown"))`); upstream's own test
   suite passes with the patch applied. Kulisa does not call `heap_dump`.
