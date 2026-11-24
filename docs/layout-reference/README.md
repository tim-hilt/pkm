# excalidraw.com layout reference

Measurements of excalidraw.com's UI at the three target display sizes. The UI tickets ([45 desktop layout](../../.scratch/v1/issues/45-desktop-layout.md), [46 mobile layout](../../.scratch/v1/issues/46-mobile-layout.md), [27 properties panel](../../.scratch/v1/issues/27-properties-panel.md), [09 zoom controls](../../.scratch/v1/issues/09-hand-tool-and-zoom-controls.md), [28 Context menu](../../.scratch/v1/issues/28-action-bar-duplicate-delete.md)) reproduce these layouts; ticket 50 reviews visual parity against them.

All lengths are CSS px, which map 1:1 to SwiftUI points. Coordinates are `x,y` of the top-left corner inside the editor container (= the window/viewport; excalidraw.com has no outer chrome). Colours are light theme unless stated.

## Method and provenance

- Live site https://excalidraw.com, driven with `playwright-cli` (Chrome channel, English locale, 2026-11-02). Geometry and computed styles were read from the DOM (`getBoundingClientRect`, `getComputedStyle`, CSS variables); icons are the live `<svg>` elements. The probe scripts are in `scripts/layout-probe/`; re-measure with them.
- Breakpoints, option sets, palettes and constants were cross-checked against the upstream source checkout at `../../excalidraw/excalidraw` (commit `8bcdca4`): `packages/common/src/editorInterface.ts`, `colors.ts`, `constants.ts`, `packages/excalidraw/components/{LayerUI,Actions,MobileToolbar,ToolPopover}.tsx`, `css/styles.scss`, `components/Actions.scss`. Items verified only in source are marked _(source)_.
- Device emulation: Chrome with a custom context per size (`hasTouch`, `isMobile`, iOS user agent, DPR 1 for Mac/iPad, DPR 2 for iPhone). The layout is chosen purely by container size (below), not by user agent, so this is equivalent to Safari on the real devices. Real Safari/iOS additionally applies safe-area insets; the native app must do this itself.
- Mouse events were used for interaction; excalidraw.com's chrome does not change with pointer type, but its **canvas** selection handles do (see [Canvas overlay](#canvas-overlay-not-chrome-but-touch-relevant)).
- Screenshots live next to this file (`*.png`); SVGs of every in-scope icon live in [`icons/`](icons/).

## 1. Which layout is used at which size

excalidraw.com has **three** form factors, not two. They are decided from the **editor container's** width × height (`getFormFactor` in `editorInterface.ts`):

```
phone   : width <= 599  ||  (height < 500 && width < 1000)
tablet  : min(width,height) >= 600  &&  max(width,height) <= 1180      (and not phone)
desktop : everything else
```

| Form factor | Styles panel mode | Toolbar | Properties |
|---|---|---|---|
| `phone` | `mobile` | bottom, 9 slots, max 450 wide | popovers anchored above a bottom action row |
| `tablet` | `compact` | top, 514 wide, no key hints | narrow 48 px side bar on the left, popovers to its right |
| `desktop` | `full` (`desktopUIMode` default; a stored preference can switch it to `compact`, no UI for that) | top, 542 wide, key hints | 195 px panel on the left |

Target sizes and what excalidraw.com does there (all verified live):

| Target | Container (CSS px) | Form factor | Layout | Screenshots |
|---|---|---|---|---|
| MacBook Air M1, typical window | 1470×832 | desktop | full | `mac-desktop-*.png` |
| MacBook Air M1, small window | 1024×768 | tablet | compact | `mac-1024x768-compact-*.png` |
| iPhone 15 Pro portrait | 393×852 | phone | mobile | `iphone-portrait-*.png` (2×) |
| iPhone 15 Pro landscape | 852×393 | phone (height < 500) | mobile | `iphone-landscape-*.png` (2×) |
| iPad Pro 12.9" portrait | 1024×1366 | **desktop** (max side 1366 > 1180) | full | `ipad-portrait-*.png` |
| iPad Pro 12.9" landscape | 1366×1024 | **desktop** | full | `ipad-landscape-*.png` |
| iPad Pro 12.9" Split View / Stage Manager window, e.g. 678×1024 | 678×1024 | tablet | compact | `ipad-splitview-678x1024-*.png` |

Further rules of thumb derived from the formula (not all screenshotted): iPad Air landscape 1180×820 → tablet; iPad Pro 11" landscape 1194×834 → desktop; any width ≤ 599 (e.g. a 1/3 Split View) → phone, whatever the height.

> **Decision for tickets 45/46.** The spec only names desktop and mobile layouts (stories 69–71). Excalidraw's third `tablet/compact` layout is what a Mac window narrower than ~1180 px or an iPad Split View half would show. Either reproduce it (section 3) or fall back to desktop/mobile at a chosen width; this reference records all three so that the choice is deliberate. The live-resize story (71) means the form factor must be re-evaluated whenever the container size changes.

## 2. Desktop layout (full) — Mac 1470×832, iPad Pro 12.9" both orientations

Screenshots: `mac-desktop-welcome.png`, `mac-desktop-selection-tool-empty.png`, `mac-desktop-rect-tool-active.png`, `mac-desktop-rect-selected.png`, `mac-desktop-rect-filled-selected.png`, `mac-desktop-text-selected.png`, `mac-desktop-arrow-selected.png`, `mac-desktop-line-selected.png`, `mac-desktop-freedraw-selected.png`, `mac-desktop-multi-selected.png`, `mac-desktop-dark-selected.png`, `ipad-landscape-*.png`, `ipad-portrait-*.png`. (`mac-desktop-welcome.png` and `*-empty.png` also show first-run decorations that are omitted, see section 7.)

Every floating control sits on a 16 px margin from the container edge and is an "island": white (`#ffffff`), radius 8, shadow `0 0 1px rgba(0,0,0,.17), 0 0 3px rgba(0,0,0,.08), 0 7px 14px rgba(0,0,0,.05)`.

### 2.1 Placement

| Control | Position | Size | Notes |
|---|---|---|---|
| Main-menu button (hamburger) | `16,16` | 36×36 | fill `#ececf4`, radius 8, 1 px white ring (`0 0 0 1px #fff`), icon 16×16 (`control-menu-trigger.svg`) |
| Toolbar island | top, horizontally centred in the container: Mac `464,16`, iPad landscape `412,16` | 542×44 | padding 4, tool gap 4. If the (omitted) top-right cluster would collide, the toolbar is pushed left: iPad portrait `211,16`. Without that cluster treat it as centred |
| Hint line | under the toolbar, `y = 68` (toolbar bottom + 8) | toolbar width × 24 | 12 px text, `#b8b8b8`, context-sensitive ("Enter to add text…"); optional |
| Properties panel | `16,76` (below menu button) | 195 wide (171 content + 2×12 padding); height = content, `max-height = containerHeight − 166`, scrolls beyond | white island, radius 8, padding 12. Shown while a drawing Tool is active or a Selection exists (§2.4) |
| Zoom group | `16, H−52` | 132×36 | three buttons 36 / 60 / 36 wide, fill `#ececf4`, radius 8 on the outer corners, 1 px white ring |
| Undo/Redo group | `157.6, H−52` (9.6 right of zoom group) | 72×36 | two 36×36 buttons, same fill/ring; disabled glyph colour `rgba(16,16,16,.3)` |
| Help button | `W−52, H−52` | 36×36 | same fill/ring; opens shortcuts (ticket 47) – optional |

Zoom group content: `−` (`control-zoom-out.svg`), current zoom as text (`100%`, 14 px, also the "reset zoom" button), `+` (`control-zoom-in.svg`). Undo/redo icons: `control-undo.svg`, `control-redo.svg`. Excalidraw.com has no "zoom to fit" button (it is a shortcut: Shift+1); ticket 09 adds one.

### 2.2 Toolbar

Island 542×44; inside, a 534×36 row with 4 px gap. Buttons are 36×36, radius 8, icons 16×16 (viewBox sizes differ per icon, see `icons/toolbar-*.svg`). Order and shortcuts:

| # | Slot | Icon file | Shortcut (label shown bottom-right) |
|---|---|---|---|
| 1 | Keep tool active after drawing (lock) | `toolbar-lock.svg` | Q (no label) |
| – | divider 1×24, `#f1f0ff`, 4 px margins | | |
| 2 | Hand | `toolbar-hand.svg` | H (no label) |
| 3 | Selection | `toolbar-selection.svg` | V or 1 |
| 4 | Rectangle | `toolbar-rectangle.svg` | R or 2 |
| 5 | Diamond | `toolbar-diamond.svg` | D or 3 |
| 6 | Ellipse | `toolbar-ellipse.svg` | O or 4 |
| 7 | Arrow | `toolbar-arrow.svg` | A or 5 |
| 8 | Line | `toolbar-line.svg` | L or 6 |
| 9 | Draw (freedraw) | `toolbar-freedraw.svg` | P or 7 |
| 10 | Text | `toolbar-text.svg` | T or 8 |
| 11 | Sticky note | – (omitted) | N |
| 12 | Eraser | `toolbar-eraser.svg` | E or 0 |
| – | divider | | |
| 13 | More tools (⋮) | `toolbar-more-tools.svg` | – |

"More tools" opens a dropdown (§2.6): Insert image (9, `toolbar-image.svg`), Frame tool (F), Web Embed, Draw to shape (Shift+X), Laser pointer (K), Bucket fill (B), Lasso selection, then "Generate": Text to diagram, Mermaid, Wireframe to code. Only **Insert image** is in scope.

States (all radius 8): normal – transparent, glyph `#1b1b1f`; hover – `#f1f0ff`; **checked/active** – fill `#e0dfff`, glyph `#030064`. Key-hint labels are 10 px, `#b8b8b8` (`#030064` on the checked tool), anchored bottom-right of the button (≈ +28, +21 inside the 36 box). In `tablet`/compact mode the key hints are hidden and the toolbar is 514 wide.

### 2.3 Zoom/undo on touch hardware

**This app has no on-screen Undo/Redo buttons** (neither the desktop group nor the phone action row's): undo and redo are the system gestures on iOS/iPadOS (three-finger swipe left/right, three-finger double-tap, shake) and ⌘Z / ⇧⌘Z on a keyboard or on macOS.

Unchanged on iPad Pro (same form factor). Target sizes are 36 pt (buttons) and 32 pt (panel buttons), i.e. **below Apple's 44 pt minimum**; excalidraw.com does not enlarge them for touch. For this app, keep the visual size but extend hit areas to ≥ 44 pt (project goal: touch-first).

### 2.4 Properties panel ("styles panel")

Island at `16,76`, 195 wide, padding 12, vertical gap 8 between fieldsets.

- **Fieldset** = 12 px legend (`#1b1b1f`, margin-bottom 4, 15 px line) + button list (padding 4 0, gap 8) → 59 px tall; pitch between fieldsets 67 px.
- **Option button**: 32×32, radius 8, icon 16×16, gap 8 (pitch 40). Unselected fill `#f6f6f9`; **selected** fill `#e0dfff`, glyph `#030064`; hover `#f1f0ff`.
- **Colour rows** (Stroke, Background): legend, then (swatches at `y = 113` for Stroke, i.e. 37 below the panel top) a 22 px high row: five **top-pick** swatches 22×22, radius 4, gap 5 (pitch 27), then at the right edge a 26×26, radius-5 **current colour** button (opens the picker popover). Transparent swatches show a checkerboard with a 1 px `#ebebeb` ring. The active swatch has a 1 px primary ring. Row pitch 61.
- **Opacity**: legend, then a range control 171×43 (padding-top 10, bottom 25): track 4 px high, radius 2, filled part `hsl(240 100% 90%)` (`#ccccff`), rest `#f6f6f9`; thumb 16 px circle `#3d3d3d`; labels `0` (left) and the current value (e.g. `100`, centred under the thumb), 12 px.
- The panel's **visible fieldsets depend on the Tool/Selection** (verified live by activating every tool):

| Tool / selected Element | Fieldsets, top to bottom |
|---|---|
| Rectangle, Diamond | Stroke · Background · _Fill_ · Stroke width · Stroke style · Sloppiness · Edges · Opacity · Layers · _Actions_ |
| Ellipse | as rectangle without Edges |
| Line | Stroke · Background · _Fill_ · Stroke width · Stroke style · Sloppiness · Edges · Opacity · Layers · _Actions_ |
| Arrow | Stroke · Stroke width · Stroke style · Sloppiness · Arrow type · Arrowheads · Opacity · Layers · _Actions_ (incl. "Edit arrow") |
| Freedraw | Stroke · Background · _Fill_ · Stroke width · Pressure · Opacity · Layers · _Actions_ |
| Text | Stroke · Font family · Font size · Text align · (vertical align when text is bound in a shape _(source)_) · Opacity · Layers · _Actions_ |
| Multiple Elements | common fieldsets above · Layers · **Align** (8 buttons, 2 rows) · Actions (+ Group / Ungroup) |
| Hand, Eraser, Selection with empty Selection | **no panel** |

  _Fill_ appears only when the background is not transparent. _Actions_ appears only when a Selection exists (not for an armed Tool). Layers/Align/Actions are what ticket 28–30's Context menu takes over.
- Stroke/Background defaults and "last used" behaviour: new Elements take the last chosen values (story 56).

### 2.5 Property controls and their option sets

Option labels are excalidraw.com's tooltips. Stored values are the `.excalidraw` JSON values.

| Control | Options (left → right) | Value | Default | Icons |
|---|---|---|---|---|
| Stroke colour | 5 top picks + picker popover | hex | `#1e1e1e` | – |
| Background colour | 5 top picks + picker popover | hex / `transparent` | `transparent` | – |
| Fill | Hachure, Cross-hatch, Solid; Option-click on the first button toggles Hachure ↔ **Zigzag** (the button then reads "Zigzag") _(source)_ | `hachure` (or `zigzag`) / `cross-hatch` / `solid` | **Solid** on this build (`DEFAULT_ELEMENT_PROPS.fillStyle`) | `panel-fill-*.svg` |
| Stroke width | Thin, Medium, Bold | 1 / 2 / 4 (freedraw: 0.5 / 1 / 2) | Medium | `panel-stroke-width-*.svg` |
| Stroke style | Solid, Dashed, Dotted | `solid` / `dashed` / `dotted` | Solid | `panel-stroke-style-*.svg` |
| Sloppiness | Architect, Artist, Cartoonist | `roughness` 0 / 1 / 2 | Artist | `panel-sloppiness-*.svg` |
| Edges | Sharp, Round | `roundness` null / `{type}` | Round for rectangle & diamond | `panel-edges-*.svg` |
| Arrow type | Sharp, Curved, Elbow | `sharp` / `round` / `elbow` | **Curved** | `panel-arrow-type-*.svg` |
| Arrowheads | two pickers (start, end); see below | `startArrowhead` / `endArrowhead` | start none, end arrow | `panel-arrowheads-*.svg` |
| Pressure (freedraw) | Constant, Variable | `simulatePressure` | Constant | `panel-pressure-*.svg` |
| Font family | Hand-drawn (Excalifont), Normal (Nunito), Code (Comic Shanns) + font picker | `fontFamily` 5 / 6 / 8 | Hand-drawn | `panel-font-family-*.svg`, `fontlist-*.svg` |
| Font size | S, M, L, XL | 16 / 20 / 28 / 36 | M (20) | `panel-font-size-*.svg` |
| Text align | Left, Center, Right | `textAlign` | Left | `panel-text-align-*.svg` |
| Opacity | slider 0–100 | `opacity` | 100 | – |
| Layers | Send to back, Send backward, Bring forward, Bring to front | – | – | `panel-layers-*.svg` |
| Align (multi) | Align left, Center horizontally, Align right, Distribute horizontally / Align top, Center vertically, Align bottom, Distribute vertically | – | – | `panel-align-*.svg` |
| Actions | Duplicate, Delete, Group / Ungroup, Link (omitted), Edit arrow (arrows) | – | – | `panel-actions-*.svg` |

**Arrowhead picker** (`mac-desktop-arrowhead-picker*.png`): a 4-button row — None (q), Arrow (w), Triangle (e), Triangle outline (r) — each button 32×32 with its shortcut letter bottom-right; a collapsible "More options" shows Circle (a), Circle outline (s), Diamond (d), Diamond outline (f), Bar (z), and a "Cardinality" group: One (x), Many (c), One or many (v), Exactly one, Zero or one, Zero or many. The popover is a white island, padding ≈ 8, floating just below the picker button. Heads for the start picker are mirrored. Full list (14 heads): `panel-arrowheads-*.svg`.

**Font picker** (`mac-desktop-font-picker.png`): popover with a quick-search field, "In this scene" (fonts used) and "Available fonts": Excalifont, Comic Shanns, Lilita One, Nunito (more are loadable but not listed: Virgil, Helvetica, Cascadia, Liberation Sans, Assistant). A fourth, separate button next to the three top picks opens it.

**Colour picker popover** (`mac-desktop-stroke-picker.png`, `mac-desktop-background-picker.png`): white island 208×346 (padding 12, content 184, vertical gap 12) to the right of the panel (x = 229, 18 px from the panel), with a pointer tip toward the trigger. Contents: heading "Colors" (12 px) + 5×3 grid of 30×30 swatches, radius 6, gap 4 (pitch 34), each with its shortcut letter bottom-right (q w e r t / a s d f g / z x c v b); heading "Shades" + 5 swatches 30×30 (only for palette colours; otherwise the text "No shades available for this color"); heading "Hex code" + 32 px input with `#` prefix and an eyedropper button (28×28, radius 8; omit or implement natively); tip text "drag any color onto your top picks to pin it" (12 px, gray). Drag-to-pin top picks is an excalidraw feature this app may drop.

**Palette** (`COLOR_PALETTE`, open-color; shade index 0…4 = 50/200/400/600/800):

| Name | 0 | 1 | 2 | 3 | 4 |
|---|---|---|---|---|---|
| gray | `#f8f9fa` | `#e9ecef` | `#ced4da` | `#868e96` | `#343a40` |
| red | `#fff5f5` | `#ffc9c9` | `#ff8787` | `#fa5252` | `#e03131` |
| pink | `#fff0f6` | `#fcc2d7` | `#f783ac` | `#e64980` | `#c2255c` |
| grape | `#f8f0fc` | `#eebefa` | `#da77f2` | `#be4bdb` | `#9c36b5` |
| violet | `#f3f0ff` | `#d0bfff` | `#9775fa` | `#7950f2` | `#6741d9` |
| blue | `#e7f5ff` | `#a5d8ff` | `#4dabf7` | `#228be6` | `#1971c2` |
| cyan | `#e3fafc` | `#99e9f2` | `#3bc9db` | `#15aabf` | `#0c8599` |
| teal | `#e6fcf5` | `#96f2d7` | `#38d9a9` | `#12b886` | `#099268` |
| green | `#ebfbee` | `#b2f2bb` | `#69db7c` | `#40c057` | `#2f9e44` |
| yellow | `#fff9db` | `#ffec99` | `#ffd43b` | `#fab005` | `#f08c00` |
| orange | `#fff4e6` | `#ffd8a8` | `#ffa94d` | `#fd7e14` | `#e8590c` |
| bronze | `#f8f1ee` | `#eaddd7` | `#d2bab0` | `#a18072` | `#846358` |

Plus `transparent`, black `#1e1e1e`, white `#ffffff`.

- **Top picks**: stroke `#1e1e1e`, `#e03131`, `#2f9e44`, `#1971c2`, `#f08c00`; background `transparent`, `#ffc9c9`, `#b2f2bb`, `#a5d8ff`, `#ffec99`.
- **Picker grid**: row 1 = transparent, white, gray, black, bronze; row 2 = cyan, blue, violet, grape, pink; row 3 = green, teal, yellow, orange, red. Stroke grid uses shade 4 for the coloured rows (gray 4, bronze 4); background grid uses shade 1.
- **Canvas background picks** (main menu): `#ffffff`, `#f8f9fa`, `#f5faff`, `#fffce8`, `#fdf8f6` + custom.
- **Dark theme** swaps UI variables (below) and renders canvas colours through a filter (invert 93 % + hue-rotate 180°); stored element colours stay the light-theme hex values. The dark top picks therefore look different (`mac-desktop-dark-selected.png`).

### 2.6 Menus (dropdown style)

Used by "More tools" and the main menu (`mac-desktop-more-tools.png`, `mac-desktop-main-menu.png`). Container white, radius 8, same shadow, padding 8, item gap 1. Item: 32 high, radius 6, padding 0 8, icon 16 + gap 10 + 14 px label, optional right-aligned gray shortcut. Group titles are bold ("Generate"). Width follows content (More tools 194, main menu 230); anchored under its trigger (main menu: 8 below the 36-px button, left-aligned at x = 16; More tools: 14 below the trigger, right-aligned to it, so `x = 808` at 1470 wide).

Main-menu items on excalidraw.com: Open, Save to…, Export image…, Live collaboration…, Command palette, Find on canvas, Help, Reset the canvas · Excalidraw+, GitHub, Follow us, Discord chat, Sign up · Preferences ▸, Theme (light / dark / system segmented control), Language select, Canvas background swatches. Icons for the in-scope ones: `menu-*.svg`.

## 3. Tablet layout (compact) — Mac 1024×768, iPad Split View 678×1024

Screenshots: `mac-1024x768-compact-selected.png`, `mac-1024x768-compact-properties-popover.png`, `mac-1024x768-compact-more-popover.png`, `ipad-splitview-678x1024-*.png`.

Differences from §2 (everything not listed is identical: menu button, zoom/undo group at the bottom-left, help button, island styling):

| Control | Position | Size | Notes |
|---|---|---|---|
| Toolbar island | top, centred (1024 wide: `255,16`; 678 wide: `80,16`, i.e. constrained between menu button and right cluster) | **514**×44 | no key-hint labels |
| **Compact style bar** | `16,68` (below the menu button, 8 gap) | 48 × (8 + 48·n), e.g. 296 for 6 items | white island, radius 8, padding 8; each item 32×40, pitch 48 |

Compact style bar items (top → bottom), each 32×32 inside its 32×40 cell:

1. **Stroke colour** – a square swatch filled with the current stroke colour carrying a hatch pattern glyph (`compact-stroke-swatch-pattern.svg`), radius 5; opens the colour popover.
2. **Background colour** – swatch (checkerboard if transparent), radius 5.
3. **Properties** (sliders glyph, `compact-properties-sliders.svg`) – opens a popover with Fill / Stroke width / Stroke style / Sloppiness / Edges / Opacity (for arrows: arrow type & heads; for text: font fields).
4. **Duplicate** and 5. **Delete** (shown directly).
6. **More** (`…`, `compact-actions-more.svg`) – popover with Layers and Actions.

Icon buttons here: radius 8, fill `rgba(255,255,255,.35)` over the island (effectively white), active (popover open) `#e0dfff`.

**Popovers** (`…compact-properties-popover.png`, `…compact-more-popover.png`): white island, radius 8, padding 12, 22 px to the right of the bar (x = 86), vertically offset so that its pointer tip lines up with the triggering item, with a small pointer tip toward it. Content width 129 (Properties popover) or 152 (popovers whose rows hold 4 buttons: Layers, Font size); fieldset structure identical to §2.4 (12 px legend + 32 px buttons). Properties popover ≈ 153×~380 for a filled rectangle; "More" popover 176×134.

The compact style bar is only present while a Tool is armed or a Selection exists (same rule as §2.4); `max-height = containerHeight − 166`.

## 4. Mobile layout (phone) — iPhone 15 Pro 393×852 and 852×393

Screenshots: `iphone-portrait-*.png`, `iphone-landscape-*.png` (taken at 2×).

### 4.1 Placement

| Control | Position | Size | Notes |
|---|---|---|---|
| Main-menu button | `12,12` | **32×32** | fill `#ececf4`, radius 8, white ring, icon 16 |
| (Library button) | `W−44,12` | 32×32 | omitted |
| **Bottom bar** (action row + toolbar) | horizontally centred, `width = min(100% − 28, 450)`, bottom aligned with a **10 px** gap: portrait `14,750.8`/365 wide; landscape `201,291.8`/450 wide | 365×91.2 | column: action row, 4 gap, toolbar |
| Action row | top of bottom bar | width × 43.2 | transparent background, no shadow, item gap 6 |
| Toolbar island | bottom of bottom bar | 365×44 (portrait), 450×44 (landscape) | white, radius 8, shadow as §2, padding 4; inner row `x+4`, 357×36, `justify-content: space-between`, min gap 4, horizontally scrollable when too narrow |
| Zoom controls | left part of the main-menu island, top right | | excalidraw.com has none (pinch only); this app shows the zoom in percent, opening a menu: Zoom In, Zoom Out, Reset, Zoom to Content. The zoom group does not fit the bottom bar |

### 4.2 Toolbar (portrait, 365 wide, 36×36 buttons, radius 4 highlight, icons 16×16)

Hand · Selection · Draw · Eraser · Rectangle · Arrow · Text · Insert image · ⋮ More. Landscape (450 wide) additionally inserts **Frame tool** before ⋮ (out of scope). There is no lock button and no Diamond/Ellipse/Line slot: those live in **tool popovers** that open when the already-active tool button is tapped again (`ToolPopover`): Rectangle → Rectangle / Diamond / Ellipse (`iphone-portrait-rect-tool-popover.png`); Arrow → Arrow / Line; Selection → Selection / Lasso; Draw → Draw / Draw-to-shape (`iphone-portrait-draw-tool-popover.png`). The popover is a white island (132×52 for three options: padding 8, three 36×36 buttons, gap 4, radius 8, shadow as §2) whose bottom edge sits 12 px above the toolbar island, centred on the tapped tool; it closes on the next canvas pointer-down. Checked state: `#e0dfff` fill, `#030064` glyph; hover is suppressed on touch. The ⋮ dropdown (`iphone-portrait-more-tools.png`) lists Sticky note, Frame tool, Web Embed, Draw to shape, Laser pointer, Bucket fill, Generate… (all out of scope).

### 4.3 Action row (above the toolbar)

Items are 32×40 cells (button 32×32, gap 6, so pitch 38), left-aligned; **Undo** and **Redo** (32×32, fill `rgba(255,255,255,.35)`, radius 8, disabled glyph `rgba(16,16,16,.3)`) are right-aligned. The row is empty on the left when nothing is armed/selected. What appears on the left depends on the armed Tool / Selection:

| State | Left items |
|---|---|
| Hand, Selection (nothing selected), Eraser | none (only Undo/Redo) |
| Rectangle (also Diamond, Ellipse, Line _(source)_) armed, nothing selected | Stroke swatch · Background swatch · Properties (sliders) |
| Rectangle selected | Stroke swatch · Background swatch · Properties · Actions (…) |
| Arrow | Stroke swatch · Properties · Arrow type (`compact-arrow-type.svg`) · _Edit arrow_ and _Actions_ (selection only) |
| Text | Stroke swatch · Properties (opacity) · Font picker (`A` glyph) · Text size/align (`Tт`) · _Actions_ (selection only) |
| Freedraw armed | Stroke swatch · Background swatch · Properties |

Duplicate and Delete live inside the **Actions** popover on narrow bars; once the row is ≥ 374 wide Delete moves out next to Redo, and at ≥ 412 Duplicate does as well (`iphone-landscape-rect-selected.png` — 450 wide — shows both outside; portrait 365 shows neither). _(source: `MobileShapeActions`, button 32, gap 6, 9 minimum actions)_

Stroke/Background swatch buttons: 32×32, radius 5, 1 px `#f1f0ff` border; the stroke swatch is the current colour with the hatch pattern glyph; the background swatch shows a checkerboard when transparent.

### 4.4 Popovers instead of a sheet

excalidraw.com's phone UI does **not** use a bottom sheet: every property group opens an anchored **popover** above the tapped item, with a pointer tip toward it, white island, radius 8, padding 12, shadow as §2:

| Popover | Size | Content | Screenshot |
|---|---|---|---|
| Stroke / Background colour | 208×333 at `x = 8`, bottom edge ≈ 24 above the action row (incl. the pointer tip) | title ("Stroke"/"Background"), then the same grid/shades/hex field as §2.5 (12 px section headings, no eyedropper, no drag-to-pin tip) | `…-stroke-picker`, `…-background-picker` |
| Properties (sliders) | 153×322 (rect) | Stroke width · Stroke style · Sloppiness · Edges · Opacity (and Fill if background set) | `…-properties-popover` |
| Actions (…) | 176×134 | Layers (4) · Actions (Link, Duplicate, Delete, Group/Ungroup…) | `…-actions-popover` |
| Text size/align (`Tт`) | 176×134 at `x = 60` (anchored to its trigger) | Font size (4) · Text align (3) | `…-text-props-popover` |
| Font picker | 240×231 at `x = 8` | "In this scene" / "Available fonts" list (no search field on phone) | `…-font-picker` |
| Main menu | 232 wide, from `12,52` | as §2.6, full list | `…-main-menu` |

This app follows these popovers rather than user story 69's "properties in a sheet": the Action row's triggers open custom popovers with the same groups and option sets. A Canvas touch while one is open only closes it (ADR-0013).

Phone `rect-selected` also shows touch-sized selection handles (§6).

## 5. Colours, typography, shape (all layouts)

Light theme CSS variables (from the live page):

| Token | Value | Used for |
|---|---|---|
| `--island-bg-color` | `#ffffff` | islands, popovers, menus |
| `--color-surface-low` | `#ececf4` | menu/zoom/undo/help button fill |
| `--color-surface-mid` | `#f6f6f9` | unselected option buttons |
| `--color-surface-high` / `--button-hover-bg` | `#f1f0ff` | hover, dividers (`--default-border-color`) |
| `--color-surface-primary-container` | `#e0dfff` | checked tool, selected option |
| `--color-on-primary-container` | `#030064` | glyph on checked/selected |
| `--color-primary` | `#6965db` | selection colour, rings, accents (`-darker #5b57d1`, `-darkest #4a47b1`, `-light #e3e2fe`) |
| `--text-primary-color` / `--icon-fill-color` | `#1b1b1f` | text, icons |
| `--keybinding-color` / hints | `#b8b8b8` | key labels, hint line |
| `--color-gray-*` | `#f5f5f5 #ebebeb #d6d6d6 #b8b8b8 #999 #7a7a7a #5c5c5c #3d3d3d #1e1e1e #121212` | grays 10–100 |
| slider track / thumb | `hsl(240 100% 90%)` / `#3d3d3d` | opacity slider |

Dark theme: island `#232329`, icon/text `#e3e3e8`, hover/high `#2e2d39`, primary container `#403e6a`, on-primary-container `#e0dfff`, primary `#a8a5ff`, surface-low `hsl(240 8% 15%)`, surface-mid `hsl(240 6% 10%)`, canvas `#121212`.

Shape/spacing tokens: `--space-factor` 4 px; radius `--border-radius-lg` 8 (`-md` 6; swatches 4–6; mobile tool highlight 4); default button 32, large button 36, icon 16. Typography: UI font **Assistant** (replace with SF), 12 px for legends/hints/labels, 14 px for menu items and zoom text, 10 px for key hints, 16 px base; Excalifont only on canvas text and welcome decorations.

Icons: Excalidraw's own set (`packages/excalidraw/components/icons.tsx`), stroke-style monoline glyphs, `stroke="currentColor"`, round caps/joins, stroke width 1.25–2 depending on glyph, viewBoxes 20/22/24 (arrowheads 40×20), displayed at 16×16. The SVG sources are in [`icons/`](icons/) and are directly reusable (MIT, see ticket 51); SF Symbols may replace them where they "match closely" (spec).

## 6. Canvas overlay (not chrome, but touch-relevant)

- Selection overlay colour `#6965db`: outline around the box, square corner/edge handles, one circular rotation handle above the box.
- **Transform handle size depends on the pointer**: mouse 8 px, Pencil 16 px, touch 28 px (`transformHandleSizes` _(source)_; compare `iphone-portrait-rect-selected.png` with `mac-desktop-rect-selected.png`). Handle spacing from the box: 2 px.
- Hint line, tooltips and the toast stack are separate overlays and are not needed for v1.

## 7. Out-of-scope UI (omitted in the native app)

Seen on excalidraw.com and intentionally **not** reproduced (spec "Out of scope" and project goal):

- **Libraries**: Library sidebar button (top-right on every layout), library dock/sidebars.
- **Share / collaboration**: Share button, Live collaboration entries, Excalidraw+ banner / "Try Excalidraw+", "Sign up", user list, laser pointer button, end-to-end-encryption badge (bottom right), social links (GitHub, X, Discord).
- **AI / generate**: Text to diagram, Wireframe to code, Mermaid to Excalidraw, Draw to shape.
- **Tools not in v1**: Sticky note, Frame tool, Web Embed, Laser pointer, Bucket fill, Lasso selection, Link action, crop.
- **Welcome screen** and its hand-drawn arrows/hints (the Gallery replaces it), command palette, "Find on canvas", Stats panel, Help dialog contents, reset-the-canvas, theme/language menu entries (system-provided), eyedropper, tool-lock hint tooltips, zen/view mode, grid/snapping toggles.
- **Context menu** (long-press / right-click): a long-press with a finger opens the system's own Context menu (glossary); its Layers/Actions content is available in §2.4/§4.3 as reference for the menu's options. A right-click context menu on the Mac is out of scope.
- **excalidraw.com quirk**: at 678×1024 the top-right cluster (Share + Library) overflows the container (`ipad-splitview-678x1024-*.png`); not to be copied.

## 8. Quick reference — numbers a UI ticket needs

| | Desktop (full) | Tablet (compact) | Phone |
|---|---|---|---|
| Edge margin | 16 | 16 | 12 (top bar) / 14 (bottom bar) |
| Menu button | 36, `16,16` | 36, `16,16` | 32, `12,12` |
| Toolbar | 542×44 top centre | 514×44 top centre | ≤450×44, bottom, 10 above edge |
| Toolbar button | 36 (radius 8) | 36 (radius 8) | 36 (radius 4 highlight) |
| Properties | 195-wide panel at `16,76` | 48-wide bar at `16,68` + popovers | action row 32-px items + popovers |
| Option button | 32 (radius 8) | 32 | 32 |
| Zoom group | 132×36 at `16,H−52` | same | none |
| Undo/Redo | 72×36 at `157.6,H−52` | same | 2×32 at the right end of the action row |
| Island | `#fff`, radius 8, shadow §2 | same | same |
