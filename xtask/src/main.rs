//! Engine development tasks.
//!
//!   cargo xtask web [--release] [--serve [PORT]]   build the web player into dist/web
//!   cargo xtask apk [--release]                    build an Android APK into dist/android

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

mod android;

const WASI_SDK: &str = "34";
const WASM_BINDGEN: &str = "0.2.129";

type Result<T = ()> = std::result::Result<T, String>;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("web") => web(&args[1..]),
        Some("apk") => android::apk(&args[1..]),
        _ => Err("usage: cargo xtask web [--release] [--serve [PORT]] | apk [--release]".into()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(cmd: &mut Command) -> Result {
    eprintln!("> {cmd:?}");
    let status = cmd.status().map_err(|e| format!("failed to run {:?}: {e}", cmd.get_program()))?;
    if status.success() { Ok(()) } else { Err(format!("{:?} failed with {status}", cmd.get_program())) }
}

/// Downloads and unpacks `url` into `dir` unless `marker` already exists.
fn fetch(url: &str, dir: &Path, marker: &Path) -> Result {
    if marker.exists() {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let archive = dir.join("download.tar.gz");
    run(Command::new("curl").args(["-fsSL", "-o"]).arg(&archive).arg(url))?;
    run(Command::new("tar").arg("xzf").arg(&archive).arg("-C").arg(dir))?;
    fs::remove_file(&archive).map_err(|e| e.to_string())
}

/// The wasi sysroot provides libc, libc++ (with wasm exceptions) and libunwind for Luau.
fn wasi_sdk() -> Result<(PathBuf, PathBuf)> {
    let dir = root().join("target").join(format!("wasi-sdk-{WASI_SDK}"));
    let base = format!("https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-{WASI_SDK}");
    let sysroot = dir.join(format!("wasi-sysroot-{WASI_SDK}.0"));
    let rt = dir.join(format!("libclang_rt-{WASI_SDK}.0")).join("wasm32-unknown-wasip1");
    fetch(&format!("{base}/wasi-sysroot-{WASI_SDK}.0.tar.gz"), &dir, &sysroot)?;
    fetch(&format!("{base}/libclang_rt-{WASI_SDK}.0.tar.gz"), &dir, &rt)?;
    Ok((sysroot, rt))
}

fn web(args: &[String]) -> Result {
    let release = args.iter().any(|a| a == "--release");
    let serve = args
        .iter()
        .position(|a| a == "--serve")
        .map(|i| args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(8080u16));

    let version = Command::new("wasm-bindgen").arg("--version").output();
    match version {
        Ok(out) if String::from_utf8_lossy(&out.stdout).contains(WASM_BINDGEN) => {}
        _ => {
            return Err(format!(
                "wasm-bindgen {WASM_BINDGEN} is required: cargo install wasm-bindgen-cli --version {WASM_BINDGEN} --locked"
            ));
        }
    }

    let (sysroot, rt) = wasi_sdk()?;
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

    let root = root();
    let mut cargo = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo
        .current_dir(&root)
        .args(["build", "-p", "kulisa-player", "--lib", "--target", "wasm32-unknown-unknown"])
        .env("WASI_SYSROOT", &sysroot)
        .env("WASI_CLANG_RT", &rt)
        .env("LUAU_CXXFLAGS", cxxflags);
    if release {
        cargo.arg("--release");
    }
    run(&mut cargo)?;

    let profile = if release { "release" } else { "debug" };
    let wasm = root.join("target/wasm32-unknown-unknown").join(profile).join("kulisa.wasm");
    let out = root.join("dist/web");
    run(Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(&out)
        .arg(&wasm))?;
    fs::write(out.join("index.html"), INDEX_HTML).map_err(|e| e.to_string())?;
    let size = fs::metadata(out.join("kulisa_bg.wasm")).map(|m| m.len()).unwrap_or(0);
    eprintln!("web build ready: {} ({:.1} MB wasm)", out.display(), size as f64 / 1e6);

    match serve {
        Some(port) => serve_dir(&out, port),
        None => Ok(()),
    }
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Kulisa</title>
<style>
  html, body { margin: 0; height: 100%; background: #18181f; color: #f4f1ea; overflow: hidden; font: 14px system-ui, sans-serif; }
  canvas { display: block; width: 100vw !important; height: 100vh !important; touch-action: none; outline: none; }
  #error { padding: 16px; }
</style>
</head>
<body>
<script type="module">
  import init from "./kulisa.js";
  // ?webgl hides WebGPU to exercise the WebGL2 fallback.
  if (new URLSearchParams(location.search).has("webgl")) {
    Object.defineProperty(navigator, "gpu", { value: undefined });
  }
  init().catch((e) => {
    const p = document.createElement("p");
    p.id = "error";
    p.textContent = "Kulisa could not start in this browser: " + e;
    document.body.append(p);
  });
</script>
</body>
</html>
"#;

/// Minimal static file server for local testing (wasm needs the right MIME type).
fn serve_dir(dir: &Path, port: u16) -> Result {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    eprintln!("serving {} at http://127.0.0.1:{port}/", dir.display());
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }
        let path = line.split_whitespace().nth(1).unwrap_or("/");
        let path = path.split('?').next().unwrap_or("/").trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        let file = dir.join(path);
        let response = match (path.contains(".."), fs::read(&file)) {
            (false, Ok(body)) => {
                let mime = match file.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("js") => "text/javascript",
                    Some("wasm") => "application/wasm",
                    Some("png") => "image/png",
                    Some("json") => "application/json",
                    _ => "application/octet-stream",
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                [head.into_bytes(), body].concat()
            }
            _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        };
        let _ = stream.write_all(&response);
    }
    Ok(())
}
