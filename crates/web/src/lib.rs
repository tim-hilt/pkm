//! Wasm exports of the core for the web app (ADR-0014). Names mirror the
//! UniFFI surface in `crates/bindings` (in camelCase, as the TS client writes
//! them); a change that surface gets and the web app needs is made here too.
//!
//! Wasm is single-threaded, so unlike in the bindings the editor needs no lock.

use std::cell::RefCell;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// A JS object that measures text for the core: the client's font
    /// machinery. See [`excalidraw_native_core::TextMeasurer`].
    pub type TextMeasurer;

    /// The advance width in Scene units of `text`, a single line, at `font_size`.
    #[wasm_bindgen(method, js_name = lineWidth)]
    fn line_width(this: &TextMeasurer, text: &str, font_size: f64) -> f64;

    /// `Date.now()`: milliseconds since 1970.
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now() -> f64;
}

/// Lets the core call a client's measurer.
struct Measurer(TextMeasurer);

// SAFETY: wasm is single-threaded, so the JS object is never touched from
// another thread.
unsafe impl Send for Measurer {}
unsafe impl Sync for Measurer {}

impl excalidraw_native_core::TextMeasurer for Measurer {
    fn line_width(&self, text: &str, font_size: f64) -> f64 {
        self.0.line_width(text, font_size)
    }
}

/// Mirrors of the core's input and Tool enums: `wasm-bindgen` exports its own
/// C-like enums, so each core enum has one here with the same variants, as
/// UniFFI's remote enums do for the bindings.
macro_rules! mirror_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),* $(,)? }) => {
        $(#[$meta])*
        #[wasm_bindgen]
        #[derive(Clone, Copy)]
        pub enum $name { $($variant),* }

        impl From<$name> for excalidraw_native_core::$name {
            fn from(value: $name) -> Self {
                match value { $($name::$variant => Self::$variant),* }
            }
        }

        impl From<excalidraw_native_core::$name> for $name {
            fn from(value: excalidraw_native_core::$name) -> Self {
                match value { $(excalidraw_native_core::$name::$variant => Self::$variant),* }
            }
        }
    };
}

mirror_enum!(
    /// What produced a pointer.
    PointerKind { Finger, Pencil, Mouse }
);
mirror_enum!(
    /// Which button of a mouse is down.
    PointerButton { Primary, Secondary, Middle }
);
mirror_enum!(PointerPhase {
    Down,
    Move,
    Up,
    Cancel
});
mirror_enum!(
    /// The active Tool.
    Tool { Selection, Hand, Rectangle, Diamond, Ellipse, Line, Arrow, Freedraw, Eraser, Text }
);
mirror_enum!(
    /// The mouse cursor the core wants over the Canvas.
    Cursor {
        Arrow, Crosshair, Text, Move, Grab, Grabbing, Pointer, Eraser,
        ResizeNorthSouth, ResizeEastWest, ResizeNorthWestSouthEast, ResizeNorthEastSouthWest,
    }
);

/// Modifier keys as a bit set: shift 1, ctrl 2, alt 4, meta 8.
fn modifiers(bits: u8) -> excalidraw_native_core::input::Modifiers {
    excalidraw_native_core::input::Modifiers {
        shift: bits & 1 != 0,
        ctrl: bits & 2 != 0,
        alt: bits & 4 != 0,
        meta: bits & 8 != 0,
    }
}

/// What the Canvas shows; see [`excalidraw_native_core::Viewport`].
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct ViewportState {
    /// Scene point at the Canvas's top-left corner.
    #[wasm_bindgen(js_name = originX)]
    pub origin_x: f64,
    #[wasm_bindgen(js_name = originY)]
    pub origin_y: f64,
    /// View points per Scene unit.
    pub zoom: f64,
}

#[wasm_bindgen]
impl ViewportState {
    #[wasm_bindgen(constructor)]
    pub fn new(origin_x: f64, origin_y: f64, zoom: f64) -> Self {
        Self {
            origin_x,
            origin_y,
            zoom,
        }
    }
}

/// Handle to the core's editor; see [`excalidraw_native_core::Editor`].
#[wasm_bindgen]
pub struct Editor {
    inner: RefCell<excalidraw_native_core::Editor>,
}

#[wasm_bindgen]
impl Editor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        // The browser has no system time for the core to ask for.
        excalidraw_native_core::set_clock(date_now);
        Self {
            inner: RefCell::new(excalidraw_native_core::Editor::new()),
        }
    }

    /// Replaces the Scene with the Drawing in `bytes` (a `.excalidraw` file's
    /// content); throws if it is not one.
    pub fn load(&self, bytes: &[u8]) -> Result<(), JsError> {
        self.inner
            .borrow_mut()
            .load(bytes)
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// The Scene as `.excalidraw` file content.
    pub fn save(&self) -> Vec<u8> {
        self.inner.borrow().save()
    }

    /// Hands the core the client's way to measure text; see
    /// [`excalidraw_native_core::Editor::set_text_measurer`]. Call it before
    /// loading a Drawing.
    #[wasm_bindgen(js_name = setTextMeasurer)]
    pub fn set_text_measurer(&self, measurer: TextMeasurer) {
        self.inner
            .borrow_mut()
            .set_text_measurer(Arc::new(Measurer(measurer)));
    }

    /// Number of Elements shown in the Scene.
    #[wasm_bindgen(js_name = elementCount)]
    pub fn element_count(&self) -> u32 {
        self.inner.borrow().element_count() as u32
    }

    /// The part of the Scene the Canvas shows.
    pub fn viewport(&self) -> ViewportState {
        let viewport = self.inner.borrow().viewport();
        let (origin_x, origin_y) = viewport.origin();
        ViewportState::new(origin_x, origin_y, viewport.zoom())
    }

    /// Shows the Scene from the given viewport; not a change of the Scene.
    #[wasm_bindgen(js_name = setViewport)]
    pub fn set_viewport(&self, viewport: &ViewportState) {
        self.inner
            .borrow_mut()
            .set_viewport((viewport.origin_x, viewport.origin_y), viewport.zoom);
    }

    /// Reports the Canvas size in points; call on start and on every resize.
    #[wasm_bindgen(js_name = setViewSize)]
    pub fn set_view_size(&self, width: f64, height: f64) {
        self.inner.borrow_mut().set_view_size(width, height);
    }

    #[wasm_bindgen(js_name = setDarkMode)]
    pub fn set_dark_mode(&self, on: bool) {
        self.inner.borrow_mut().set_dark_mode(on);
    }

    /// The Canvas colour as `0xRRGGBBAA`.
    #[wasm_bindgen(js_name = canvasColor)]
    pub fn canvas_color(&self) -> u32 {
        self.inner.borrow().canvas_color()
    }

    /// Differs from before whenever the Scene changed; read it after
    /// forwarding input and ask for [`Editor::geometry`] again if it moved.
    pub fn revision(&self) -> f64 {
        self.inner.borrow().revision() as f64
    }

    /// One zoom step in around the middle of the view.
    #[wasm_bindgen(js_name = zoomIn)]
    pub fn zoom_in(&self) {
        self.inner.borrow_mut().zoom_in();
    }

    /// One zoom step out around the middle of the view.
    #[wasm_bindgen(js_name = zoomOut)]
    pub fn zoom_out(&self) {
        self.inner.borrow_mut().zoom_out();
    }

    /// Back to 100 % around the middle of the view.
    #[wasm_bindgen(js_name = resetZoom)]
    pub fn reset_zoom(&self) {
        self.inner.borrow_mut().reset_zoom();
    }

    /// Frames all shown Elements.
    #[wasm_bindgen(js_name = zoomToFit)]
    pub fn zoom_to_fit(&self) {
        self.inner.borrow_mut().zoom_to_fit();
    }

    /// The active Tool.
    pub fn tool(&self) -> Tool {
        self.inner.borrow().tool().into()
    }

    #[wasm_bindgen(js_name = setTool)]
    pub fn set_tool(&self, tool: Tool) {
        self.inner.borrow_mut().set_tool(tool.into());
    }

    /// A step of a finger, Pencil or mouse; see
    /// [`excalidraw_native_core::Editor::pointer`]. `modifiers` is a bit set:
    /// shift 1, ctrl 2, alt 4, meta 8. The fields are the core's
    /// `PointerEvent`, flat, so that no object is made per event.
    #[wasm_bindgen]
    #[allow(clippy::too_many_arguments)]
    pub fn pointer(
        &self,
        id: u32,
        phase: PointerPhase,
        x: f64,
        y: f64,
        kind: PointerKind,
        button: PointerButton,
        pressure: f32,
        timestamp: f64,
        modifier_bits: u8,
    ) {
        self.inner
            .borrow_mut()
            .pointer(excalidraw_native_core::input::PointerEvent {
                id: id.into(),
                phase: phase.into(),
                x,
                y,
                kind: kind.into(),
                button: button.into(),
                pressure,
                timestamp,
                modifiers: modifiers(modifier_bits),
            });
    }

    /// A scroll wheel or trackpad scroll; `dx`, `dy` are how far the content
    /// should move, in view points. `modifiers` as in [`Editor::pointer`].
    pub fn scroll(&self, x: f64, y: f64, dx: f64, dy: f64, modifier_bits: u8) {
        self.inner
            .borrow_mut()
            .scroll(excalidraw_native_core::input::ScrollEvent {
                x,
                y,
                dx,
                dy,
                modifiers: modifiers(modifier_bits),
            });
    }

    /// A trackpad pinch: the relative change of zoom since the last event.
    pub fn magnify(&self, x: f64, y: f64, magnification: f64) {
        self.inner
            .borrow_mut()
            .magnify(excalidraw_native_core::input::MagnifyEvent {
                x,
                y,
                magnification,
            });
    }

    /// The space bar went down (`true`) or up.
    #[wasm_bindgen(js_name = setPanKey)]
    pub fn set_pan_key(&self, held: bool) {
        self.inner.borrow_mut().set_pan_key(held);
    }

    /// The mouse cursor at the view point `(x, y)`.
    pub fn cursor(&self, x: f64, y: f64) -> Cursor {
        self.inner.borrow().cursor(x, y).into()
    }

    /// The paths to draw for the Scene.
    pub fn geometry(&self) -> Geometry {
        Geometry {
            inner: self.inner.borrow_mut().geometry(),
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero-copy geometry (ADR-0007): only sizes and buffer addresses cross the
/// boundary, the client reads the data in place from wasm memory. An address
/// is valid until this object is freed, and the buffers never change; but
/// memory may grow in between, so clients make their views of it afresh.
#[wasm_bindgen]
pub struct Geometry {
    inner: excalidraw_native_core::Geometry,
}

#[wasm_bindgen]
impl Geometry {
    /// Tells whether cached per-Element results are still valid.
    pub fn generation(&self) -> f64 {
        self.inner.generation() as f64
    }

    #[wasm_bindgen(js_name = partCount)]
    pub fn part_count(&self) -> u32 {
        self.inner.parts().len() as u32
    }

    /// Size in bytes of one part record, so the client can check that its
    /// mirror of the record has the same layout.
    #[wasm_bindgen(js_name = partStride)]
    pub fn part_stride(&self) -> u32 {
        size_of::<excalidraw_native_core::Part>() as u32
    }

    #[wasm_bindgen(js_name = partsAddress)]
    pub fn parts_address(&self) -> u32 {
        self.inner.parts().as_ptr() as u32
    }

    #[wasm_bindgen(js_name = verbCount)]
    pub fn verb_count(&self) -> u32 {
        self.inner.verbs().len() as u32
    }

    #[wasm_bindgen(js_name = verbsAddress)]
    pub fn verbs_address(&self) -> u32 {
        self.inner.verbs().as_ptr() as u32
    }

    #[wasm_bindgen(js_name = coordCount)]
    pub fn coord_count(&self) -> u32 {
        self.inner.coords().len() as u32
    }

    #[wasm_bindgen(js_name = coordsAddress)]
    pub fn coords_address(&self) -> u32 {
        self.inner.coords().as_ptr() as u32
    }

    #[wasm_bindgen(js_name = textLineCount)]
    pub fn text_line_count(&self) -> u32 {
        self.inner.text_lines().len() as u32
    }

    #[wasm_bindgen(js_name = textLineStride)]
    pub fn text_line_stride(&self) -> u32 {
        size_of::<excalidraw_native_core::TextLine>() as u32
    }

    #[wasm_bindgen(js_name = textLinesAddress)]
    pub fn text_lines_address(&self) -> u32 {
        self.inner.text_lines().as_ptr() as u32
    }

    #[wasm_bindgen(js_name = textByteCount)]
    pub fn text_byte_count(&self) -> u32 {
        self.inner.text().len() as u32
    }

    #[wasm_bindgen(js_name = textAddress)]
    pub fn text_address(&self) -> u32 {
        self.inner.text().as_ptr() as u32
    }

    #[wasm_bindgen(js_name = imageCount)]
    pub fn image_count(&self) -> u32 {
        self.inner.images().len() as u32
    }

    #[wasm_bindgen(js_name = imageStride)]
    pub fn image_stride(&self) -> u32 {
        size_of::<excalidraw_native_core::Image>() as u32
    }

    #[wasm_bindgen(js_name = imagesAddress)]
    pub fn images_address(&self) -> u32 {
        self.inner.images().as_ptr() as u32
    }

    #[wasm_bindgen(js_name = fileIdByteCount)]
    pub fn file_id_byte_count(&self) -> u32 {
        self.inner.file_ids().len() as u32
    }

    #[wasm_bindgen(js_name = fileIdsAddress)]
    pub fn file_ids_address(&self) -> u32 {
        self.inner.file_ids().as_ptr() as u32
    }
}
