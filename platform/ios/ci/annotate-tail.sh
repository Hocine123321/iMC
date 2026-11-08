#!/bin/sh
# Turns the end of a build log into GitHub annotations, so a failure's cause is readable from the
# run's check annotations without downloading logs. Usage: annotate-tail.sh <log> <title>
log="$1"; title="$2"
[ -f "$log" ] || { echo "::error title=$title::no log at $log"; exit 0; }
enc() { sed -e 's/%/%25/g' -e 's/\r/%0D/g' | awk '{printf "%s%%0A", $0}'; }
errs=$(grep -E '(^|[^a-z])(error|ld: |clang: error|fatal error|Undefined symbols|FAILED|\*\* BUILD FAILED)' "$log" | grep -v 'warning' | head -n 25 | enc)
[ -n "$errs" ] && echo "::error title=$title: errors::$errs"
tail=$(tail -n 40 "$log" | cut -c1-300 | enc)
echo "::error title=$title: last lines::$tail"
