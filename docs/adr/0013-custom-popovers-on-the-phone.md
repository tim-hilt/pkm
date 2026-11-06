# Custom popovers on the phone, and they swallow Canvas touches

On a phone, the Properties panel is split by concern into popovers opened from the Action row: Stroke, Background, Properties (sliders), Text size/align and Actions (`…`), the same groups excalidraw.com shows. The tool family pickers (Rectangle/Diamond/Ellipse, Arrow/Line) use the same popovers. They are our own component rather than SwiftUI's `.popover`, so that a tap on another Action row trigger switches popovers in one tap, as on excalidraw.com. Any touch on the Canvas while a popover is open only closes it and never reaches the Canvas, so that dismissing a popover cannot change or lose the Selection, which is the excalidraw.com problem this app exists to fix. This replaces the single properties sheet that `docs/layout-reference` §4.4 used to describe as the native adaptation of story 69.

## Considered Options

- **One sheet with every property group** (the earlier design): it never fit at the medium detent, needed scrolling, covered the Canvas and stretched the colour rows across the full width.
- **SwiftUI's `.popover` with `.presentationCompactAdaptation(.popover)`**: provides accessibility, dismissal and the pointer tip with no extra code, but the first tap outside only closes it, so switching between two popovers takes two taps.
- **Excalidraw's behaviour, where a Canvas touch both closes the popover and acts on the Canvas**: a mistimed tap would clear the Selection.

## Consequences

- The component has to provide what the system popover gave for free: VoiceOver treats it as a modal and the escape gesture closes it; it fades in and out; it is capped to the height above the Action row and scrolls beyond that; it moves up with the bottom bar above the keyboard.
- A popover stays open while its options are tapped, and closes itself when its trigger leaves the Action row (e.g. undo removes the Selection).
