# Dark mode is display-only, using Excalidraw's colour transform

Dark mode follows the system appearance and only changes what is shown. The `.excalidraw` file stores no theme and is never changed by it, and PNG exports always use the Drawing's own (light) colours. On the Canvas, every Element colour, the Canvas background (`viewBackgroundColor`) and the colour picker's swatches go through excalidraw.com's dark-mode transform (`applyDarkModeFilter`: CSS `invert(93%) hue-rotate(180deg)`). The core owns this transform as a pure function, tested against Excalidraw's values, so every client gets the same result. In dark mode the core reports display colours (geometry, overlay, Text view, Canvas background), so clients render without knowing about the transform. Raster images are not transformed. As a result, a Drawing looks the same here as on excalidraw.com in either theme, and a file made in dark mode opens correctly everywhere.

## Considered Options

- **Inverting the whole rendered frame**: one post-pass that also covers cached thumbnails, but it inverts photos, which Excalidraw does not.
- **Storing the theme in the file, or writing dark colours**: other devices and excalidraw.com would show the Drawing with the wrong colours.
- **A colour matrix in the client's shaders** (the transform is affine in RGB): every client would need its own copy, and the overlay and Text view colours would still need the transform elsewhere.
- **Each client implementing the transform itself** (a strict reading of ADR-0008): every platform would need its own copy of an Excalidraw compatibility rule.

## Consequences

- Each colour is transformed once, when its geometry is generated, so frames cost nothing extra. Switching the appearance regenerates the geometry once.
- The Gallery caches a separate thumbnail for each appearance.
- The Text view shows the transformed colour while editing but commits the original one.
