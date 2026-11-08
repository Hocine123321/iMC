#!/bin/sh
# Boots an iPhone simulator, installs and launches the app, waits, takes a screenshot and reports
# what happened as GitHub annotations (readable from the run's check annotations). Every command
# has a time limit and every phase reports at once, so a hang still leaves a trail.
# Usage: smoke.sh <path to .app> <bundle id>
app="$1"; bundle="$2"
lim() { s="$1"; shift; perl -e 'alarm shift; exec @ARGV' "$s" "$@"; }
note() { # title, text
  msg=$(printf '%s' "$2" | sed -e 's/%/%25/g' -e 's/\r//g' | awk '{printf "%s%%0A", $0}')
  echo "::notice title=$1::$msg"
}
out=$(mktemp)
line=$(xcrun simctl list devices available | grep -E 'iPhone' | head -1)
udid=$(echo "$line" | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')
echo "device: $line"
{
  echo "device: $line"
  lim 120 xcrun simctl boot "$udid" 2>&1
  lim 300 xcrun simctl bootstatus "$udid" -b 2>&1 | tail -3
  lim 120 xcrun simctl install "$udid" "$app" 2>&1 && echo "installed"
} > "$out" 2>&1
note "smoke 1: boot and install" "$(cat "$out")"

( lim 90 xcrun simctl launch --console "$udid" "$bundle" > smoke.log 2>&1 ) &
sleep 25
note "smoke 2: app output (stderr) after 25 s" "$(head -n 40 smoke.log | cut -c1-400)"

{
  echo "--- process"
  lim 30 xcrun simctl spawn "$udid" launchctl list 2>&1 | grep -i -E 'imc|HorizonIOS' || echo "no imc process in launchctl"
  echo "--- crash reports"
  ls ~/Library/Logs/DiagnosticReports 2>/dev/null | grep -i -E 'HorizonIOS|imc' || echo "none"
  lim 60 xcrun simctl io "$udid" screenshot smoke.png 2>&1
} > "$out" 2>&1
note "smoke 3: process, crashes, screenshot" "$(cat "$out")"

{
  lim 120 python3 -m venv /tmp/pyvenv 2>&1 && lim 120 /tmp/pyvenv/bin/pip install --quiet pillow 2>&1
  lim 120 /tmp/pyvenv/bin/python - 2>&1 <<'PY'
from PIL import Image
im = Image.open("smoke.png").convert("RGB")
w, h = im.size
im = im.resize((w // 4, h // 4))
w, h = im.size; n = w * h
px = list(im.getdata())
clear = sum(1 for r, g, b in px if abs(r-13) < 8 and abs(g-18) < 8 and abs(b-31) < 10)
orange = sum(1 for r, g, b in px if r > 120 and 70 < g < 190 and b < 90 and r > g > b)
blue = sum(1 for r, g, b in px if b > 70 and b > r + 25 and r < 90)
print(f"screenshot (quarter size) {w}x{h}: distinct colours {len(set(px))}, clear-colour fraction {clear/n:.3f}, orange fraction {orange/n:.3f}, blue fraction {blue/n:.3f}")
PY
} > "$out" 2>&1
note "smoke 4: screenshot analysis" "$(cat "$out")"
lim 30 xcrun simctl terminate "$udid" "$bundle" >/dev/null 2>&1
lim 60 xcrun simctl shutdown "$udid" >/dev/null 2>&1
exit 0
