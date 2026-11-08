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
  lim 120 /tmp/pyvenv/bin/python -W ignore - 2>&1 <<'PY'
from collections import Counter
from PIL import Image
im = Image.open("smoke.png").convert("RGB")
W, H = im.size
quarter = list(im.resize((W // 4, H // 4)).getdata())
bg, bg_n = Counter(quarter).most_common(1)[0]
print(f"screenshot {W}x{H}: {len(set(quarter))} distinct colours at quarter size, dominant colour {bg} covers {bg_n/len(quarter):.3f}")
cols = 56
rows = max(8, int(cols * H / W / 2))
small = list(im.resize((cols, rows)).getdata())
ramp = " .:-=+*#%@"
for y in range(rows):
    line = ""
    for x in range(cols):
        r, g, b = small[y * cols + x]
        if abs(r - bg[0]) + abs(g - bg[1]) + abs(b - bg[2]) < 40:
            line += " "
        else:
            line += ramp[1 + min(8, (r * 299 + g * 587 + b * 114) // 1000 * 9 // 256)]
    print(line.rstrip() or ".")
PY
} > "$out" 2>&1
note "smoke 4: screenshot analysis" "$(cat "$out")"
lim 30 xcrun simctl terminate "$udid" "$bundle" >/dev/null 2>&1
lim 60 xcrun simctl shutdown "$udid" >/dev/null 2>&1
exit 0
