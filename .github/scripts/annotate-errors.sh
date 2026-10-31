#!/usr/bin/env bash
# Turns compiler errors from a CI log into a workflow annotation, which is
# readable without signing in to GitHub (unlike raw job logs).
log="${1:-ci.log}"
[ -f "$log" ] || exit 0
# Annotations are capped at 4 KB, so clip huge lines (linker command lines).
clip() { cut -c1-240; }
# What tests said when they failed comes first: `error:` lines only name the test binary.
panics=$(grep -E -A6 'panicked at' "$log" | clip | head -n 30)
msg=$(grep -E -A30 '^error' "$log" | grep -v '^ *= note: "' | clip | head -n 30)
msg=$(printf '%s\n%s' "$panics" "$msg")
[ -z "$msg" ] && msg=$(tail -n 40 "$log" | clip)
msg="${msg//'%'/'%25'}"
msg="${msg//$'\r'/}"
msg="${msg//$'\n'/'%0A'}"
echo "::error title=Build failed::$msg"
