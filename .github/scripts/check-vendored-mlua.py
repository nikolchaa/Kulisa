#!/usr/bin/env python3
"""Keeps third_party/mlua equal to a crates.io release plus our patches.

    check-vendored-mlua.py                    verify (CI): fail on any unexpected difference
    check-vendored-mlua.py --update VERSION   replace the vendored copy with VERSION + patches

Only src/, LICENSE, README.md and the manifest's non-dev sections are vendored.
"""

import io
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VENDORED = ROOT / "third_party" / "mlua"
PATCHES = VENDORED / "patches"
USER_AGENT = "kulisa-vendoring (https://github.com/nikolchaa/Kulisa)"


def vendored_version() -> str:
    m = re.search(r'^version = "([^"]+)"', (VENDORED / "Cargo.toml").read_text(), re.M)
    return m.group(1)


def strip_dev_sections(manifest: str) -> str:
    """Drops test/example/bench targets and dev-dependencies, which we do not vendor."""
    parts = re.split(r"(?m)^(?=\[)", manifest.replace("\r\n", "\n"))
    dev = re.compile(r"\[\[(example|test|bench)\]\]|\[dev-dependencies|\[target\.[^\]]*\.dev-dependencies")
    return "".join(p for p in parts if not dev.match(p)).strip()


def read(path: Path) -> bytes:
    return path.read_bytes().replace(b"\r\n", b"\n")


def tree(base: Path) -> dict[str, bytes]:
    return {p.relative_to(base).as_posix(): read(p) for p in sorted(base.rglob("*")) if p.is_file()}


def fetch(version: str, dest: Path) -> Path:
    """Downloads mlua `version` from crates.io, applies our patches, returns its directory."""
    url = f"https://static.crates.io/crates/mlua/mlua-{version}.crate"
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with tarfile.open(fileobj=io.BytesIO(urllib.request.urlopen(req).read())) as tar:
        tar.extractall(dest, filter="data")
    upstream = dest / f"mlua-{version}"
    for patch in sorted(PATCHES.glob("*.patch")):
        result = subprocess.run(["patch", "-p1", "--forward", "-i", str(patch)], cwd=upstream,
                                capture_output=True, text=True)
        if result.returncode != 0:
            sys.exit(f"error: {patch.name} does not apply to mlua {version}; if upstream fixed it, "
                     f"delete the patch, otherwise fix and regenerate it.\n{result.stdout}{result.stderr}")
    return upstream


def check() -> int:
    version = vendored_version()
    with tempfile.TemporaryDirectory() as tmp:
        upstream = fetch(version, Path(tmp))
        problems = []
        ours, theirs = tree(VENDORED / "src"), tree(upstream / "src")
        for path in sorted(set(ours) | set(theirs)):
            if ours.get(path) != theirs.get(path):
                problems.append(f"src/{path} differs from upstream {version} + patches")
        for name in ["LICENSE", "README.md"]:
            if read(VENDORED / name) != read(upstream / name):
                problems.append(f"{name} differs from upstream")
        if strip_dev_sections((VENDORED / "Cargo.toml").read_text()) != strip_dev_sections(
            (upstream / "Cargo.toml").read_text()
        ):
            problems.append("Cargo.toml differs from upstream beyond removed dev sections")

    if problems:
        print("\n".join(f"error: third_party/mlua: {p}" for p in problems))
        return 1
    print(f"third_party/mlua matches mlua {version} + {len(list(PATCHES.glob('*.patch')))} patch(es)")
    return 0


def update(version: str) -> int:
    old = vendored_version()
    with tempfile.TemporaryDirectory() as tmp:
        upstream = fetch(version, Path(tmp))
        shutil.rmtree(VENDORED / "src")
        shutil.copytree(upstream / "src", VENDORED / "src")
        for name in ["LICENSE", "README.md"]:
            shutil.copyfile(upstream / name, VENDORED / name)
        manifest = strip_dev_sections((upstream / "Cargo.toml").read_text())
        (VENDORED / "Cargo.toml").write_text(manifest + "\n", newline="\n")
    (VENDORED / "PATCHES.md").write_text(
        (VENDORED / "PATCHES.md").read_text().replace(f"mlua {old}", f"mlua {version}"), newline="\n"
    )
    print(f"third_party/mlua updated {old} -> {version}. Next:")
    print("  1. bump `mlua` in the root Cargo.toml if the minor version changed")
    print("  2. cargo test --workspace && cargo xtask web && cargo xtask apk")
    return 0


if __name__ == "__main__":
    args = sys.argv[1:]
    if args[:1] == ["--update"] and len(args) == 2:
        sys.exit(update(args[1]))
    if args:
        sys.exit(__doc__)
    sys.exit(check())
