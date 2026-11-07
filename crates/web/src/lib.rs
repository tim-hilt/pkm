//! Wasm exports of the core for the web app (ADR-0014). Names mirror the
//! UniFFI surface in `crates/bindings` (in camelCase, as the TS client writes
//! them); a change that surface gets and the web app needs is made here too.
//!
//! Wasm is single-threaded, so unlike in the bindings the editor needs no lock.

use std::cell::RefCell;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `Date.now()`: milliseconds since 1970.
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now() -> f64;
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
