//! The web build: the player compiled to wasm (Luau built against a wasi-sdk sysroot with
//! wasm exceptions, see docs/DESIGN.md risk 1), wasm-bindgen glue, a page and the game pack.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Game, Options, Result, escape, fetch, run, write};

const WASI_SDK: &str = "34";
const WASM_BINDGEN: &str = "0.2.129";

/// The wasi sysroot provides libc, libc++ (with wasm exceptions) and libunwind for Luau.
fn wasi_sdk(engine: &Path) -> Result<(PathBuf, PathBuf)> {
    // Outside `target/`: CI's cargo cache prunes target/ and could leave a partial sysroot.
    let dir = engine.join(".tools").join(format!("wasi-sdk-{WASI_SDK}"));
    let base = format!("https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-{WASI_SDK}");
    let sysroot = dir.join(format!("wasi-sysroot-{WASI_SDK}.0"));
    let rt = dir.join(format!("libclang_rt-{WASI_SDK}.0")).join("wasm32-unknown-wasip1");
    let libcxxabi = sysroot.join("lib/wasm32-wasip1/eh/libc++abi.a");
    fetch(&format!("{base}/wasi-sysroot-{WASI_SDK}.0.tar.gz"), &dir, &libcxxabi)?;
    fetch(&format!("{base}/libclang_rt-{WASI_SDK}.0.tar.gz"), &dir, &rt.join("libclang_rt.builtins.a"))?;
    Ok((sysroot, rt))
}

/// Builds the web player for `game` into `out` (page, wasm, glue, `game.kpak`).
pub fn build(game: &Game, opts: &Options, out: &Path) -> Result {
    let version = Command::new("wasm-bindgen").arg("--version").output();
    match version {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains(WASM_BINDGEN) => {}
        _ => {
            return Err(format!(
                "wasm-bindgen {WASM_BINDGEN} is required: cargo install wasm-bindgen-cli --version {WASM_BINDGEN} --locked"
            ));
        }
    }

    let (sysroot, rt) = wasi_sdk(&opts.engine)?;
    let s = sysroot.display().to_string().replace('\\', "/");
    let cxxflags = [
        "--target=wasm32-wasip1".to_string(),
        format!("--sysroot={s}"),
        "-nostdinc++".into(),
        format!("-isystem {s}/include/wasm32-wasip1/eh/c++/v1"),
        format!("-isystem {s}/include/c++/v1"),
        "-fexceptions -fcxx-exceptions -fwasm-exceptions".into(),
        "-mllvm -wasm-use-legacy-eh=false".into(),
    ]
    .join(" ");
    run(opts
        .cargo_player(&["--lib", "--target", "wasm32-unknown-unknown"])
        .env("WASI_SYSROOT", &sysroot)
        .env("WASI_CLANG_RT", &rt)
        .env("LUAU_CXXFLAGS", cxxflags))?;

    let wasm = opts.engine.join("target/wasm32-unknown-unknown").join(opts.profile()).join("kulisa.wasm");
    run(Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(out)
        .arg(&wasm))?;
    write(&out.join("index.html"), page(game))?;
    write(&out.join("game.kpak"), game.pack()?)?;
    let size = fs::metadata(out.join("kulisa_bg.wasm")).map(|m| m.len()).unwrap_or(0);
    eprintln!("web build ready: {} ({:.1} MB wasm)", out.display(), size as f64 / 1e6);
    Ok(())
}

/// The page: the game's title and background, and a clear message for browsers without
/// WebAssembly exception handling (exnref), which Luau needs.
pub fn page(game: &Game) -> String {
    let background =
        game.config.background.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '#').collect::<String>();
    INDEX_HTML.replace("{title}", &escape(&game.config.title)).replace("{background}", &background)
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>{title}</title>
<style>
  html, body { margin: 0; height: 100%; background: {background}; color: #f4f1ea; overflow: hidden; font: 16px system-ui, sans-serif; }
  canvas { display: block; width: 100vw !important; height: 100vh !important; touch-action: none; outline: none; }
  #error { max-width: 36em; margin: 20vh auto; padding: 0 16px; line-height: 1.5; }
</style>
</head>
<body>
<script type="module">
  function fail(text) {
    const p = document.createElement("p");
    p.id = "error";
    p.textContent = text;
    document.body.append(p);
  }
  // A module with one `try_table`: browsers without exnref cannot validate it.
  const exnref = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0,
    10, 8, 1, 6, 0, 31, 64, 0, 11, 11]);
  if (typeof WebAssembly !== "object" || !WebAssembly.validate(exnref)) {
    fail("This game needs a newer browser: Chrome or Edge 137, Firefox 131, Safari 18.4 or later.");
  } else {
    // ?webgl hides WebGPU to exercise the WebGL2 fallback.
    if (new URLSearchParams(location.search).has("webgl")) {
      Object.defineProperty(navigator, "gpu", { value: undefined });
    }
    const { default: init } = await import("./kulisa.js");
    init().catch((e) => fail("The game could not start in this browser: " + e));
  }
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use kulisa_platform::GameConfig;

    #[test]
    fn page_has_title_and_background() {
        let config =
            GameConfig::parse(Some("[game]\ntitle = \"Cats <3\"\n[window]\nbackground = \"#102030\""))
                .unwrap();
        let html = page(&Game { dir: PathBuf::new(), config });
        assert!(html.contains("<title>Cats &lt;3</title>"));
        assert!(html.contains("background: #102030;"));
    }
}
