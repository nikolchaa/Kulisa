#!/usr/bin/env bash
# Turns compiler errors from a CI log into a workflow annotation, which is
# readable without signing in to GitHub (unlike raw job logs).
log="${1:-ci.log}"
[ -f "$log" ] || exit 0
msg=$(grep -E -A14 '^error' "$log" | head -n 160)
[ -z "$msg" ] && msg=$(tail -n 60 "$log")
msg="${msg//'%'/'%25'}"
msg="${msg//$'\r'/}"
msg="${msg//$'\n'/'%0A'}"
echo "::error title=Build failed::$msg"
