#!/bin/sh
# Boots an iPhone simulator once, then for each app: installs it, launches it, waits, takes a
# screenshot and reports what happened as GitHub annotations (readable from the run's check
# annotations). Every command has a time limit and every phase reports at once, so a hang still
# leaves a trail.
# Usage: smoke.sh LABEL APP BUNDLE WAITFOR MAXWAIT [LABEL APP BUNDLE WAITFOR MAXWAIT ...]
#   WAITFOR: text to wait for in the app's stderr (empty: just wait MAXWAIT seconds)
lim() { s="$1"; shift; perl -e 'alarm shift; exec @ARGV' "$s" "$@"; }
note() { # title, text
  msg=$(printf '%s' "$2" | sed -e 's/%/%25/g' -e 's/\r//g' | awk '{printf "%s%%0A", $0}')
  echo "::notice title=$1::$msg"
}
out=$(mktemp)
line=$(xcrun simctl list devices available | grep -E 'iPhone' | head -1)
udid=$(echo "$line" | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')
{
  echo "device: $line"
  lim 120 xcrun simctl boot "$udid" 2>&1
  lim 300 xcrun simctl bootstatus "$udid" -b 2>&1 | tail -3
} > "$out" 2>&1
note "smoke: boot" "$(cat "$out")"
{
  lim 120 python3 -m venv /tmp/pyvenv 2>&1 && lim 120 /tmp/pyvenv/bin/pip install --quiet pillow 2>&1
} > "$out" 2>&1
sleep 15

while [ $# -ge 5 ]; do
  label="$1"; app="$2"; bundle="$3"; waitfor="$4"; maxwait="$5"; shift 5
  rm -f "smoke-$label.log" "smoke-$label.out" "smoke-$label.png"
  {
    lim 120 xcrun simctl install "$udid" "$app" 2>&1 && echo "installed $bundle"
    lim 150 xcrun simctl launch --stdout="$PWD/smoke-$label.out" --stderr="$PWD/smoke-$label.log" "$udid" "$bundle" 2>&1
  } > "$out" 2>&1
  note "smoke $label 1: install and launch" "$(cat "$out")"

  i=0
  while [ $i -lt $maxwait ]; do
    [ -n "$waitfor" ] && grep -q "$waitfor" "smoke-$label.log" 2>/dev/null && break
    i=$((i+2)); sleep 2
  done
  sleep 5
  note "smoke $label 2: app stderr (waited $i s)" "$( (head -n 30 "smoke-$label.log"; echo '...'; tail -n 12 "smoke-$label.log") 2>&1 | cut -c1-300)"

  data=$(lim 30 xcrun simctl get_app_container "$udid" "$bundle" data 2>/dev/null)
  {
    echo "--- process"
    lim 30 xcrun simctl spawn "$udid" launchctl list 2>&1 | grep -i "$bundle" || echo "app not running"
    echo "--- crash reports"
    crash=$(ls -t ~/Library/Logs/DiagnosticReports/*Horizon* 2>/dev/null | head -1)
    if [ -n "$crash" ]; then echo "$crash"; head -c 3500 "$crash"; else echo "none"; fi
    echo "--- container files"
    [ -n "$data" ] && find "$data" -maxdepth 5 -type f \( -name '*.log' -o -name '*.json' -o -name '*.csv' \) 2>/dev/null | sed "s#$data/##" | head -20
  } > "$out" 2>&1
  note "smoke $label 3: process, crashes, files" "$(cat "$out")"

  if [ -n "$data" ]; then
    {
      echo "--- Documents/telemetry/latest.json"
      cat "$data/Documents/telemetry/latest.json" 2>&1
      lg=$(find "$data/Library" -name '*.log' 2>/dev/null | head -1)
      if [ -n "$lg" ]; then echo "--- ${lg#$data/} (first 25, last 25)"; head -n 25 "$lg" | cut -c1-260; echo ...; tail -n 25 "$lg" | cut -c1-260; fi
    } > "$out" 2>&1
    note "smoke $label 4: telemetry report and Horizon log" "$(cat "$out")"
  fi

  lim 60 xcrun simctl io "$udid" screenshot "smoke-$label.png" > "$out" 2>&1
  cols=60; [ "$label" = game ] && cols=100
  {
    lim 120 /tmp/pyvenv/bin/python -W ignore - "smoke-$label.png" "$cols" 2>&1 <<'PY'
import sys
from collections import Counter
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
cols = int(sys.argv[2])
W, H = im.size
quarter = list(im.resize((W // 4, H // 4)).getdata())
bg, bg_n = Counter(quarter).most_common(1)[0]
print(f"screenshot {W}x{H}: {len(set(quarter))} distinct colours at quarter size, dominant colour {bg} covers {bg_n/len(quarter):.3f}")
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
  } >> "$out" 2>&1
  note "smoke $label 5: screenshot as text" "$(cat "$out")"
  lim 30 xcrun simctl terminate "$udid" "$bundle" >/dev/null 2>&1
done
lim 60 xcrun simctl shutdown "$udid" >/dev/null 2>&1
exit 0
