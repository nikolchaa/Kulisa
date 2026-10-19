#!/usr/bin/env bash
# Runs dist/ios/Kulisa.app (from `cargo xtask ios-app --sim`) in an iOS simulator and checks
# that the Luau smoke-test line reached the unified log. Leaves a screenshot and logs in dist/ios.
set -euo pipefail
app=dist/ios/Kulisa.app
id=dev.kulisa.player
out="$PWD/dist/ios"

# Newest iOS runtime, first iPhone in it.
udid=$(xcrun simctl list devices available -j | python3 -c '
import json, sys
devices = json.load(sys.stdin)["devices"]
ios = sorted((r for r in devices if ".iOS-" in r), key=lambda r: [int(x) for x in r.split(".iOS-")[1].split("-")])
for runtime in reversed(ios):
    for d in devices[runtime]:
        if d["name"].startswith("iPhone"):
            print(d["udid"]); sys.exit()
sys.exit("no iPhone simulator available")
')
echo "simulator: $(xcrun simctl list devices | grep "$udid")"
xcrun simctl bootstatus "$udid" -b
xcrun simctl install "$udid" "$app"

# Info messages are not persisted by default, so stream the log live instead of `log show`.
xcrun simctl spawn "$udid" log stream --level debug --style compact \
    --predicate 'process == "kulisa-player"' > "$out/app.log" 2>&1 &
stream=$!
sleep 3

launch=$(xcrun simctl launch --terminate-running-process \
    --stdout="$out/stdout.log" --stderr="$out/stderr.log" "$udid" "$id")
echo "$launch"
pid=${launch##*: }
sleep 10

xcrun simctl io "$udid" screenshot "$out/screenshot.png"
alive=0
kill -0 "$pid" 2>/dev/null && alive=1
kill "$stream" 2>/dev/null || true
xcrun simctl spawn "$udid" log show --last 5m --info --debug --style compact \
    --predicate 'process == "kulisa-player"' >> "$out/app.log" 2>&1 || true

echo "--- unified log (kulisa-player) ---"
tail -n 60 "$out/app.log"
echo "--- stderr ---"
tail -n 40 "$out/stderr.log" 2>/dev/null || true

status=0
if [ "$alive" != 1 ]; then
    echo "error: kulisa-player (pid $pid) is not running after 10 s; it crashed or exited"
    report=$(ls -t ~/Library/Logs/DiagnosticReports/kulisa-player* 2>/dev/null | head -n 1 || true)
    [ -n "$report" ] && head -n 80 "$report"
    status=1
fi
if ! grep -q "hello from Luau" "$out/app.log"; then
    echo "error: \"hello from Luau\" missing from the simulator log"
    status=1
fi
if [ "$status" = 0 ]; then
    # As an annotation so the result is readable without signing in to GitHub.
    line=$(grep -m1 "hello from Luau" "$out/app.log" | cut -c1-240)
    errors=$(grep -E "renderer:|panicked""$out/app.log" | sort -u | head -n 5 | cut -c1-240 || true)
    msg=$line
    [ -n "$errors" ] && msg+=$'\n'$errors
    msg=${msg//%/%25}
    msg=${msg//$'\n'/%0A}
    echo "::notice title=iOS simulator run::$msg"
fi
exit "$status"
