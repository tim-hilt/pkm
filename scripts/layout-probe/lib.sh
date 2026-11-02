# Usage: source scripts/layout-probe/lib.sh <session> [<device>]
# Drives excalidraw.com in one playwright-cli session; see README.md.
# OUT (default /tmp/layout-probe) receives screenshots and measurements.

PROBE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
S=$1
P="playwright-cli -s=$S"
OUT=${OUT:-/tmp/layout-probe}
mkdir -p "$OUT/shots" "$OUT/out"

# Opens excalidraw.com with a device from devices/ (e.g. iphone-portrait).
open_site() { $P open https://excalidraw.com --config="$PROBE/devices/$1.json" >/dev/null; sleep 3; }
click() { $P mousemove "$1" "$2" >/dev/null; $P mousedown >/dev/null; $P mouseup >/dev/null; sleep "${3:-0.5}"; }
drag() {
  $P mousemove "$1" "$2" >/dev/null; $P mousedown >/dev/null
  $P mousemove $((($1 + $3) / 2)) $((($2 + $4) / 2)) >/dev/null; $P mousemove "$3" "$4" >/dev/null
  $P mouseup >/dev/null; sleep 0.5
}
# shot <name> [--full-page]: screenshot to $OUT/shots/<name>.png
shot() { $P screenshot --filename="$OUT/shots/$1.png" ${2:+"$2"} >/dev/null; }
clearall() { $P press Escape >/dev/null; $P press Meta+a >/dev/null; $P press Delete >/dev/null; $P press Escape >/dev/null; }

# run <script.js>: runs one of the probe scripts and prints its raw result.
run() { $P --raw run-code --filename="$PROBE/$1"; }
# outline: indented DOM tree of the UI with rects (x,y wxh) and titles.
outline() { run outline.js | python3 -c 'import json, sys; print(json.load(sys.stdin))'; }
# measure <selectors/name.json> <outname>: rects + computed styles + CSS
# variables into $OUT/out/<outname>.json; print with show.py.
measure() {
  local js="$OUT/out/.styles-run.js"
  python3 -c 'import sys; print(open(sys.argv[1]).read().replace("__SELS__", open(sys.argv[2]).read()))' \
    "$PROBE/styles.js" "$PROBE/$1" >"$js"
  $P --raw run-code --filename="$js" >"$OUT/out/$2.json"
}
# panel <outname>: every fieldset of the properties panel into $OUT/out/<outname>.json; print with show-panel.py.
panel() { run panel.js >"$OUT/out/$1.json"; }
# icons <outname>: the live <svg> of every visible control into $OUT/out/<outname>.json.
icons() { run icons.js >"$OUT/out/$1.json"; }
