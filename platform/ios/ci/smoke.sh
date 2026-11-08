#!/bin/sh
# Boots an iPhone simulator, installs and launches the app, waits, takes a screenshot and reports
# what happened as GitHub annotations (readable from the run's check annotations).
# Usage: smoke.sh <path to .app> <bundle id>
app="$1"; bundle="$2"
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
  xcrun simctl boot "$udid" 2>&1
  xcrun simctl bootstatus "$udid" -b 2>&1 | tail -3
  xcrun simctl install "$udid" "$app" 2>&1 && echo "installed"
} > "$out" 2>&1
xcrun simctl launch --console "$udid" "$bundle" > smoke.log 2>&1 &
sleep 30
{
  echo "--- process"
  xcrun simctl spawn "$udid" launchctl list 2>&1 | grep -i -E 'imc|HorizonIOS' || echo "no imc process in launchctl"
  echo "--- crash reports"
  ls ~/Library/Logs/DiagnosticReports 2>/dev/null | grep -i -E 'HorizonIOS|imc' || echo "none"
} >> "$out" 2>&1
xcrun simctl io "$udid" screenshot smoke.png >> "$out" 2>&1
python3 -m venv /tmp/pyvenv >> "$out" 2>&1 && /tmp/pyvenv/bin/pip install --quiet pillow >> "$out" 2>&1
/tmp/pyvenv/bin/python - >> "$out" 2>&1 <<'PY'
from PIL import Image
im = Image.open("smoke.png").convert("RGB")
w, h = im.size
px = im.getdata(); n = w * h
clear = sum(1 for r, g, b in px if abs(r-13) < 8 and abs(g-18) < 8 and abs(b-31) < 10)
orange = sum(1 for r, g, b in px if r > 120 and 70 < g < 190 and b < 90 and r > g > b)
colours = len(im.getcolors(maxcolors=n) or [])
print(f"--- screenshot {w}x{h}: distinct colours {colours}, clear-colour fraction {clear/n:.3f}, orange-texel fraction {orange/n:.3f}")
PY
note "smoke: simulator" "$(cat "$out")"
note "smoke: app output (stderr)" "$(head -n 60 smoke.log)"
xcrun simctl terminate "$udid" "$bundle" >/dev/null 2>&1
xcrun simctl shutdown "$udid" >/dev/null 2>&1
exit 0
