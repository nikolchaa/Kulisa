#!/usr/bin/env python3
"""Checks that third_party/mlua is exactly the crates.io release plus our patches.

Downloads the published crate, applies third_party/mlua/patches/*.patch, and fails
if any source file, the license, the readme or the manifest's non-dev sections differ.
"""

import io
import re
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VENDORED = ROOT / "third_party" / "mlua"


def version() -> str:
    m = re.search(r'^version = "([^"]+)"', (VENDORED / "Cargo.toml").read_text(), re.M)
    return m.group(1)


def strip_dev_sections(manifest: str) -> str:
    """Drops test/example/bench targets and dev-dependencies, which we do not vendor."""
    parts = re.split(r"(?m)^(?=\[)", manifest.replace("\r\n", "\n"))
    dev = re.compile(r"\[\[(example|test|bench)\]\]|\[dev-dependencies|\[target\.[^\]]*\.dev-dependencies")
    return "".join(p for p in parts if not dev.match(p)).strip()


def files(base: Path, rel: str) -> dict[str, bytes]:
    root = base / rel
    return {str(p.relative_to(base)).replace("\\", "/"): p.read_bytes().replace(b"\r\n", b"\n")
            for p in sorted(root.rglob("*")) if p.is_file()}


def main() -> int:
    ver = version()
    url = f"https://static.crates.io/crates/mlua/mlua-{ver}.crate"
    with tempfile.TemporaryDirectory() as tmp:
        data = urllib.request.urlopen(url).read()
        with tarfile.open(fileobj=io.BytesIO(data)) as tar:
            tar.extractall(tmp, filter="data")
        upstream = Path(tmp) / f"mlua-{ver}"
        for patch in sorted((VENDORED / "patches").glob("*.patch")):
            subprocess.run(["patch", "-p1", "--quiet", "-i", str(patch)], cwd=upstream, check=True)

        problems = []
        ours, theirs = files(VENDORED, "src"), files(upstream, "src")
        for path in sorted(set(ours) | set(theirs)):
            if ours.get(path) != theirs.get(path):
                problems.append(f"{path} differs from upstream {ver} + patches")
        for name in ["LICENSE", "README.md"]:
            if (VENDORED / name).read_bytes().replace(b"\r\n", b"\n") != (upstream / name).read_bytes().replace(b"\r\n", b"\n"):
                problems.append(f"{name} differs from upstream")
        if strip_dev_sections((VENDORED / "Cargo.toml").read_text()) != strip_dev_sections((upstream / "Cargo.toml").read_text()):
            problems.append("Cargo.toml differs from upstream beyond removed dev sections")

    if problems:
        print("\n".join(f"error: third_party/mlua: {p}" for p in problems))
        return 1
    print(f"third_party/mlua matches mlua {ver} + {len(list((VENDORED / 'patches').glob('*.patch')))} patch(es)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
