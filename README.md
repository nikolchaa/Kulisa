<p align="center">
  <img src="docs/brand/logo.svg" width="96" alt="Kulisa logo">
</p>

<h1 align="center">Kulisa</h1>

<p align="center">A fast, fully customizable visual novel and narrative game engine, written in Rust.</p>

---

*Kulisa* is Serbian for stage scenery. The engine provides rendering, text, input, audio,
scripting, saves and rollback. It has no built-in look: the classic visual novel interface
ships as an editable template written in Kulisa's own scripting, so games built with it can
look like anything.

- **Two scripting layers.** `.kulisa` story files for writers, Luau for programmers.
- **Everything is public API.** Templates and demos use only what games can use.
- **Six platforms.** Windows, Linux, macOS, Web, Android, iOS.
- **Fast and lean.** Batched GPU rendering via wgpu, instant startup, small builds.

> Status: early development. Nothing is usable yet.

See [docs/DESIGN.md](docs/DESIGN.md) for the architecture.

## License

[MIT](LICENSE)
