# Excalifont

Excalifont is the only bundled font (`apple/ExcalidrawNative/Resources/Fonts`,
SIL Open Font License 1.1; the licence text is next to it). Every text is
drawn with it, whatever `fontFamily` the Element has; the family stays in the
file.

## How the file is made

Excalidraw ships Excalifont as seven `woff2` files, each covering a range of
Unicode (`packages/excalidraw/fonts/Excalifont`). CoreText cannot read `woff2`
and would have to know which file to ask for, so they are merged into one
TrueType file with fontTools (162 kB):

```sh
python3 -m venv /tmp/fvenv && /tmp/fvenv/bin/pip install fonttools brotli
cd <excalidraw checkout>/packages/excalidraw/fonts/Excalifont
for f in *.woff2; do /tmp/fvenv/bin/python -c "
from fontTools.ttLib import TTFont; f = TTFont('$f'); f.flavor = None; f.save('/tmp/${f%.woff2}.ttf')"; done
/tmp/fvenv/bin/python -c "
from fontTools.merge import Merger; import glob
Merger().merge(sorted(glob.glob('/tmp/Excalifont-Regular-*.ttf'))).save('Excalifont-Regular.ttf')"
```

The metrics the core uses for the baseline (`unitsPerEm` 1000, ascender 886,
descender -374) are Excalidraw's own `FONT_METADATA` for Excalifont and match
the file's `hhea` table.

## Where it is used

- `Excalifont` (`Rendering/Excalifont.swift`) implements the core's
  `TextMeasurer` with CoreText and turns lines of text into glyph outlines,
  cached per glyph (ADR-0008).
- `Editor.withExcalifont()` creates an editor that measures with it.
- Text is drawn as filled paths through the stencil fill, so it is sharp at
  every zoom; characters the font lacks come from the system's fallback fonts.
