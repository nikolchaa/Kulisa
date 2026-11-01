#!/usr/bin/env bash
# Checks a release's downloads the way someone without Rust uses them.
#
#   release-check.sh DOWNLOADS           install the tool, make games for every target with it
#   release-check.sh --start-from DIR    start the games another system packed in DIR
#   release-check.sh --published TAG     the same from a release on GitHub
#
# The game is demos/smoke, whose script logs a line that says the game itself came up.
set -euo pipefail

case "$(uname -s)" in
    Linux*) host=linux ;;
    Darwin*) host=macos ;;
    *) host=windows ;;
esac
export RUST_LOG=info

# Starts a program from another folder, so it must find the game on its own, and checks it is
# still running after a while and that the game came up.
start_game() {
    local program=$1 status=0 log pid
    log=$(mktemp)
    if [ "$host" = linux ]; then
        (cd /tmp && exec xvfb-run -a "$program") > "$log" 2>&1 &
    else
        (cd "${TMPDIR:-/tmp}" && exec "$program") > "$log" 2>&1 &
    fi
    pid=$!
    sleep 15
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "error: $program is not running after 15 s"
        status=1
    fi
    pkill -f "$(basename "$program")" 2>/dev/null || kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    echo "--- log of $program ---"
    tail -n 30 "$log"
    if ! grep -q "hello from Luau" "$log"; then
        echo "error: \"hello from Luau\" is not in the log of $program"
        status=1
    fi
    return "$status"
}

if [ "$1" = --start-from ]; then
    tar xzf "$2/Smoke Test-$host.tar.gz" -C "$2"
    if [ "$host" = macos ]; then
        start_game "$PWD/$2/Smoke Test.app/Contents/MacOS/Smoke Test"
    else
        start_game "$PWD/$2/Smoke Test/Smoke Test"
    fi
    exit 0
fi

work=$(mktemp -d)

# No compiler to fall back on.
PATH=$(printf '%s' "$PATH" | tr ':' '\n' | grep -v -E 'cargo|rust' | paste -sd: -)
export PATH
if command -v cargo > /dev/null; then
    echo "error: cargo is still on the PATH"
    exit 1
fi

# Installs the tool from the release at $1, the way the download page says to.
install_tool() {
    local home=$work/installed
    if [ "$host" = windows ]; then
        KULISA_RELEASES=$1 KULISA_HOME=$(cygpath -w "$home") KULISA_NO_MODIFY_PATH=1 \
            powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "irm $1/install.ps1 | iex"
    else
        curl -fsSL "$1/install.sh" | KULISA_RELEASES=$1 KULISA_HOME=$home KULISA_NO_MODIFY_PATH=1 sh
    fi
    kulisa=$home/kulisa
    "$kulisa" templates
}

smoke_game() {
    cp -R demos/smoke "$work/smoke"
    sed -i.bak 's/^title = .*/title = "Smoke Test"/' "$work/smoke/kulisa.toml"
    rm "$work/smoke/kulisa.toml.bak"
}

# Starts what this system built.
start_here() {
    case "$host" in
        windows) start_game "$work/smoke/dist/windows/Smoke Test.exe" ;;
        linux) start_game "$work/smoke/dist/linux/Smoke Test" ;;
        macos) .github/scripts/macos-app-smoke.sh "$work/smoke/dist/macos/Smoke Test.app" ;;
    esac
}

if [ "$1" = --published ]; then
    install_tool "https://github.com/$GITHUB_REPOSITORY/releases/download/$2"
    smoke_game
    # The tool finds its players in the release of its own version.
    for target in windows linux macos web; do
        "$kulisa" build "$target" "$work/smoke"
    done
    start_here
    exit 0
fi

downloads=$(cd "$1" && pwd)
cp install/install.sh install/install.ps1 "$downloads/"
(cd "$downloads" && if command -v sha256sum > /dev/null; then sha256sum -- *; else shasum -a 256 -- *; fi) > "$work/SHA256SUMS"
mv "$work/SHA256SUMS" "$downloads/SHA256SUMS"

# Windows can have a `python3` that only offers to install Python.
python=python3
python3 --version > /dev/null 2>&1 || python=python
"$python" -m http.server 8765 --directory "$downloads" > "$work/server.log" 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT
for _ in 1 2 3 4 5 6 7 8 9 10; do
    curl -fs -o /dev/null http://127.0.0.1:8765/ && break
    sleep 1
done
export KULISA_RELEASES=http://127.0.0.1:8765

install_tool "$KULISA_RELEASES"
"$kulisa" new "Fresh Game" --dir "$work/fresh"

# A player that is not the release's is refused, and leaves nothing behind.
web=$downloads/kulisa-player-web.tar.gz
mv "$web" "$web.good"
head -c 100000 "$web.good" > "$web"
if "$kulisa" build web "$work/fresh" 2> "$work/refused.log"; then
    echo "error: the tool used a damaged player"
    exit 1
fi
grep -q "checksum" "$work/refused.log" || { cat "$work/refused.log"; exit 1; }
mv "$web.good" "$web"

"$kulisa" build web "$work/fresh"
test -s "$work/fresh/dist/web/kulisa_bg.wasm"
test -s "$work/fresh/dist/web/game.kpak"
test -s "$work/fresh/dist/web/THIRD-PARTY-LICENSES.txt"
grep -q "<title>Fresh Game</title>" "$work/fresh/dist/web/index.html"

smoke_game
for target in windows linux macos web; do
    "$kulisa" build "$target" "$work/smoke"
done
"$kulisa" build android "$work/smoke" --abi arm64-v8a 2> "$work/android.log" || \
    grep -q "Android SDK\|aapt\|apksigner" "$work/android.log" || { cat "$work/android.log"; exit 1; }
test -s "$work/smoke/dist/windows/Smoke Test.exe"
test -s "$work/smoke/dist/windows/game.kpak"
test -s "$work/smoke/dist/windows/THIRD-PARTY-LICENSES.txt"
test -s "$work/smoke/dist/linux/game.kpak"
test -s "$work/smoke/dist/linux/THIRD-PARTY-LICENSES.txt"
test -s "$work/smoke/dist/macos/Smoke Test.app/Contents/Resources/game.kpak"
test -s "$work/smoke/dist/macos/Smoke Test.app/Contents/Resources/THIRD-PARTY-LICENSES.txt"

case "$host" in
    windows)
        mkdir -p check-out
        cp "$work/smoke/dist/Smoke Test-linux.tar.gz" "$work/smoke/dist/Smoke Test-macos.tar.gz" check-out/
        ;;
    macos)
        # The downloaded iOS player, in a simulator, for the only system that can.
        "$kulisa" build ios "$work/smoke" --sim
        mkdir -p dist/ios
        cp -R "$work/smoke/dist/ios/Smoke Test.app" dist/ios/Kulisa.app
        .github/scripts/ios-sim-smoke.sh
        ;;
esac
start_here
