#!/usr/bin/env bash
# Checks the app bundle from `kulisa build macos --universal`: its layout, signature and
# architectures, then starts it from another folder, so the game must come from the bundle.
set -euo pipefail
app=$(cd "$1" && pwd)
name=$(basename "$app" .app)
program="$app/Contents/MacOS/$name"

plutil -lint "$app/Contents/Info.plist"
test -f "$app/Contents/Resources/game.kpak"
archs=$(lipo -archs "$program")
echo "architectures: $archs"
case "$archs" in *arm64*x86_64* | *x86_64*arm64*) ;; *) echo "error: not universal"; exit 1 ;; esac
codesign --verify --deep --strict --verbose=2 "$app"

log=$(mktemp)
cd /tmp
RUST_LOG=info "$program" > "$log" 2>&1 &
pid=$!
# A busy runner can take a while to start it. Then it must stay up.
for _ in $(seq 60); do
    grep -q "hello from Luau" "$log" && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 1
done
sleep 5
status=0
if ! kill -0 "$pid" 2>/dev/null; then
    echo "error: the app is not running"
    status=1
fi
kill "$pid" 2>/dev/null || true
echo "--- app log ---"
tail -n 40 "$log"
if ! grep -q "hello from Luau" "$log"; then
    echo "error: \"hello from Luau\" missing from the log"
    status=1
fi
# As an annotation, since job logs need a GitHub sign-in to read.
if [ "$status" != 0 ]; then
    echo "::notice title=macOS app log::$(tail -n 25 "$log" | cut -c1-200 | awk '{ printf "%s%%0A", $0 }')"
fi
exit "$status"
