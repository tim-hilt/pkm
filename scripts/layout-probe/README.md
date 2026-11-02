# Layout probe

Measures excalidraw.com's UI with `playwright-cli` (see the `playwright-cli` skill). Built for issue 04; its results are in `docs/layout-reference/`. Re-use it for any later layout or styling measurement instead of writing new probes.

```bash
source scripts/layout-probe/lib.sh iph      # session name
open_site iphone-portrait                   # a device from devices/
outline                                     # DOM tree with rects
click 197 820; drag 120 250 270 400         # draw a rectangle
shot iphone-rect-selected                   # $OUT/shots/
measure selectors/mobile.json iph-sel       # rects + computed styles + CSS vars
python3 scripts/layout-probe/show.py $OUT/out/iph-sel.json
panel iph-panel; python3 scripts/layout-probe/show-panel.py $OUT/out/iph-panel.json
icons iph-icons                             # live <svg> per control
playwright-cli -s=iph close
```

- `devices/`: viewport, scale, touch and user agent of each target device form factor.
- `selectors/`: `[name, css selector, all?]` lists for `measure`; `all` = 1 measures every match instead of the first.
- `ancestors.js`: styles of a popover's ancestor chain (`run ancestors.js`), for finding which element draws the island.
- Coordinates are CSS pixels of the device viewport; screenshots are at device scale.
