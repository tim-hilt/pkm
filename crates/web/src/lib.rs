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
        mirror_enum!($(#[$meta])* $name in excalidraw_native_core::$name { $($variant),* });
    };
    ($(#[$meta:meta])* $name:ident in $core:path { $($variant:ident),* $(,)? }) => {
        $(#[$meta])*
        #[wasm_bindgen]
        #[derive(Clone, Copy)]
        pub enum $name { $($variant),* }

        impl From<$name> for $core {
            fn from(value: $name) -> Self {
                match value { $($name::$variant => Self::$variant),* }
            }
        }

        impl From<$core> for $name {
            fn from(value: $core) -> Self {
                match value { $(<$core>::$variant => Self::$variant),* }
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

// The options of the properties panel.
mirror_enum!(FillPattern in excalidraw_native_core::scene::FillPattern { Hachure, CrossHatch, Zigzag, Solid });
mirror_enum!(LineStyle in excalidraw_native_core::scene::LineStyle { Solid, Dashed, Dotted });
mirror_enum!(StrokeWidth { Thin, Medium, Bold });
mirror_enum!(Sloppiness {
    Architect,
    Artist,
    Cartoonist
});
mirror_enum!(Edges { Sharp, Round });
mirror_enum!(FontSize {
    Small,
    Medium,
    Large,
    ExtraLarge
});
mirror_enum!(TextAlign in excalidraw_native_core::TextAlign { Left, Center, Right });
mirror_enum!(
    /// Where a layer command moves the Selection in the z-order.
    ZOrder { Forward, Backward, ToFront, ToBack }
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

    /// The text view to show over the Canvas, `undefined` if no text is being
    /// edited; see [`excalidraw_native_core::Editor::text_editor`]. Read it
    /// with the viewport after every input or command.
    #[wasm_bindgen(js_name = textEditor)]
    pub fn text_editor(&self) -> Option<TextEditor> {
        let e = self.inner.borrow().text_editor()?;
        Some(TextEditor {
            text: e.text,
            x: e.x,
            y: e.y,
            width: e.width,
            height: e.height,
            font_size: e.font_size,
            line_height: e.line_height,
            font_family: e.font_family,
            align: e.align.into(),
            color: e.color,
            opacity: e.opacity,
            angle: e.angle,
            zoom: e.zoom,
            fixed_width: e.fixed_width,
        })
    }

    /// Tells the core what the text view says now.
    #[wasm_bindgen(js_name = setEditingText)]
    pub fn set_editing_text(&self, text: &str) {
        self.inner.borrow_mut().set_editing_text(text);
    }

    /// Ends the editing of the text and keeps it.
    #[wasm_bindgen(js_name = commitText)]
    pub fn commit_text(&self) {
        self.inner.borrow_mut().commit_text();
    }

    /// Ends the editing of the text and keeps it as it was.
    #[wasm_bindgen(js_name = cancelText)]
    pub fn cancel_text(&self) {
        self.inner.borrow_mut().cancel_text();
    }

    /// Opens the text view on the text or label of the Selection (Enter).
    #[wasm_bindgen(js_name = editSelectedText)]
    pub fn edit_selected_text(&self) {
        self.inner.borrow_mut().edit_selected_text();
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

    /// Whether the Tool lock is on: the Tool then stays after drawing a shape.
    #[wasm_bindgen(js_name = toolLock)]
    pub fn tool_lock(&self) -> bool {
        self.inner.borrow().tool_lock()
    }

    #[wasm_bindgen(js_name = setToolLock)]
    pub fn set_tool_lock(&self, locked: bool) {
        self.inner.borrow_mut().set_tool_lock(locked);
    }

    /// Whether there is something to undo; changes after any input or command.
    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.inner.borrow().can_undo()
    }

    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.inner.borrow().can_redo()
    }

    /// Takes back the latest change to the Scene.
    pub fn undo(&self) {
        self.inner.borrow_mut().undo();
    }

    /// Does again what was last undone.
    pub fn redo(&self) {
        self.inner.borrow_mut().redo();
    }

    /// Differs from before whenever [`Editor::overlay`] may: read it after
    /// forwarding input and ask for the overlay again if it moved.
    #[wasm_bindgen(js_name = overlayRevision)]
    pub fn overlay_revision(&self) -> f64 {
        self.inner.borrow().overlay_revision() as f64
    }

    /// What to draw over the Scene (selection frames, handles, the box-select
    /// rectangle, binding highlights), in Scene coordinates and for the
    /// current zoom.
    pub fn overlay(&self) -> Geometry {
        Geometry {
            inner: self.inner.borrow().overlay(),
        }
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

    /// Performs the Command the core binds to the key `key` (see
    /// [`excalidraw_native_core::Shortcut::key`]) with the modifier bits
    /// `modifiers`; `false` if no Command is.
    #[wasm_bindgen(js_name = pressKey)]
    pub fn press_key(&self, key: &str, modifiers_bits: u8) -> bool {
        let held = modifiers(modifiers_bits);
        let Some(shortcut) = excalidraw_native_core::shortcuts()
            .into_iter()
            .find(|s| s.key == key && s.modifiers == held)
        else {
            return false;
        };
        self.inner.borrow_mut().perform(shortcut.command);
        true
    }

    /// The clipboard text for copying the Selection; `undefined` with nothing
    /// selected. See [`excalidraw_native_core::Editor::copy_selection`].
    #[wasm_bindgen(js_name = copySelection)]
    pub fn copy_selection(&self) -> Option<String> {
        self.inner.borrow().copy_selection()
    }

    /// Copies the Selection and deletes it. See
    /// [`excalidraw_native_core::Editor::cut_selection`].
    #[wasm_bindgen(js_name = cutSelection)]
    pub fn cut_selection(&self) -> Option<String> {
        self.inner.borrow_mut().cut_selection()
    }

    /// Pastes Excalidraw clipboard JSON in the middle of the view; `false`
    /// for any other text.
    pub fn paste(&self, text: &str) -> bool {
        self.inner.borrow_mut().paste(text)
    }

    /// Pastes Excalidraw clipboard JSON around the point `(x, y)` of the view.
    #[wasm_bindgen(js_name = pasteAt)]
    pub fn paste_at(&self, text: &str, x: f64, y: f64) -> bool {
        self.inner.borrow_mut().paste_at(text, x, y)
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

    /// The ids of the selected Elements.
    pub fn selection(&self) -> Vec<String> {
        self.inner.borrow().selection().to_vec()
    }

    /// Puts pasted images in the middle of the view; see
    /// [`excalidraw_native_core::Editor::insert_images`].
    #[wasm_bindgen(js_name = insertImages)]
    pub fn insert_images(&self, images: Vec<ImageFile>) {
        self.inner
            .borrow_mut()
            .insert_images(images.into_iter().map(Into::into).collect());
    }

    /// Puts dropped images around the point `(x, y)` of the view; see
    /// [`excalidraw_native_core::Editor::insert_images_at`].
    #[wasm_bindgen(js_name = insertImagesAt)]
    pub fn insert_images_at(&self, images: Vec<ImageFile>, x: f64, y: f64) {
        self.inner.borrow_mut().insert_images_at(
            images.into_iter().map(Into::into).collect(),
            x,
            y,
        );
    }

    /// The bytes of the image file `id` as stored in the Drawing, for the
    /// client to decode; `undefined` if the Drawing has no such file.
    #[wasm_bindgen(js_name = fileBytes)]
    pub fn file_bytes(&self, id: &str) -> Option<Vec<u8>> {
        self.inner.borrow().file_bytes(id)
    }

    /// What a PNG export shows; `undefined` if there is nothing to export.
    pub fn export(&self, scope: ExportScope) -> Option<Export> {
        let export = self.inner.borrow().export(scope.into())?;
        Some(Export {
            geometry: Some(Geometry {
                inner: export.geometry,
            }),
            x: export.x,
            y: export.y,
            width: export.width,
            height: export.height,
        })
    }

    /// What the properties panel shows; see
    /// [`excalidraw_native_core::Editor::properties`]. Read it after every
    /// input or command.
    pub fn properties(&self) -> Properties {
        let shown = self.inner.borrow().properties();
        let s = shown.sections;
        Properties {
            stroke_color: shown.stroke_color,
            background_color: shown.background_color,
            fill_style: shown.fill_style.map(Into::into),
            stroke_width: shown.stroke_width.map(Into::into),
            stroke_style: shown.stroke_style.map(Into::into),
            sloppiness: shown.sloppiness.map(Into::into),
            edges: shown.edges.map(Into::into),
            opacity: shown.opacity,
            font_size: shown.font_size.map(Into::into),
            text_align: shown.text_align.map(Into::into),
            sections: Sections {
                stroke_color: s.stroke_color,
                background_color: s.background_color,
                fill_style: s.fill_style,
                stroke_width: s.stroke_width,
                stroke_style: s.stroke_style,
                sloppiness: s.sloppiness,
                edges: s.edges,
                opacity: s.opacity,
                text: s.text,
            },
        }
    }

    /// Whether the chrome shows the properties panel.
    #[wasm_bindgen(js_name = showsProperties)]
    pub fn shows_properties(&self) -> bool {
        self.inner.borrow().shows_properties()
    }

    /// Sets a property on the Selection and as the style of the next Element.
    #[wasm_bindgen(js_name = setStrokeColor)]
    pub fn set_stroke_color(&self, color: String) {
        self.set_property(excalidraw_native_core::Property::StrokeColor(color));
    }

    #[wasm_bindgen(js_name = setBackgroundColor)]
    pub fn set_background_color(&self, color: String) {
        self.set_property(excalidraw_native_core::Property::BackgroundColor(color));
    }

    #[wasm_bindgen(js_name = setFillStyle)]
    pub fn set_fill_style(&self, value: FillPattern) {
        self.set_property(excalidraw_native_core::Property::FillStyle(value.into()));
    }

    #[wasm_bindgen(js_name = setStrokeWidth)]
    pub fn set_stroke_width(&self, value: StrokeWidth) {
        self.set_property(excalidraw_native_core::Property::StrokeWidth(value.into()));
    }

    #[wasm_bindgen(js_name = setStrokeStyle)]
    pub fn set_stroke_style(&self, value: LineStyle) {
        self.set_property(excalidraw_native_core::Property::StrokeStyle(value.into()));
    }

    #[wasm_bindgen(js_name = setSloppiness)]
    pub fn set_sloppiness(&self, value: Sloppiness) {
        self.set_property(excalidraw_native_core::Property::Sloppiness(value.into()));
    }

    #[wasm_bindgen(js_name = setEdges)]
    pub fn set_edges(&self, value: Edges) {
        self.set_property(excalidraw_native_core::Property::Edges(value.into()));
    }

    /// `0` to `100`.
    #[wasm_bindgen(js_name = setOpacity)]
    pub fn set_opacity(&self, value: f64) {
        self.set_property(excalidraw_native_core::Property::Opacity(value));
    }

    #[wasm_bindgen(js_name = setFontSize)]
    pub fn set_font_size(&self, value: FontSize) {
        self.set_property(excalidraw_native_core::Property::FontSize(value.into()));
    }

    #[wasm_bindgen(js_name = setTextAlign)]
    pub fn set_text_align(&self, value: TextAlign) {
        self.set_property(excalidraw_native_core::Property::TextAlign(value.into()));
    }

    /// Duplicates the Selection.
    pub fn duplicate(&self) {
        self.inner.borrow_mut().duplicate();
    }

    #[wasm_bindgen(js_name = deleteSelection)]
    pub fn delete_selection(&self) {
        self.inner.borrow_mut().delete_selection();
    }

    #[wasm_bindgen(js_name = moveInZOrder)]
    pub fn move_in_z_order(&self, how: ZOrder) {
        self.inner.borrow_mut().move_in_z_order(how.into());
    }

    /// Groups the Selection (see [`excalidraw_native_core::Editor::group`]).
    pub fn group(&self) {
        self.inner.borrow_mut().group();
    }

    /// Dissolves the groups in the Selection.
    pub fn ungroup(&self) {
        self.inner.borrow_mut().ungroup();
    }

    /// Selects every Element.
    #[wasm_bindgen(js_name = selectAll)]
    pub fn select_all(&self) {
        self.inner
            .borrow_mut()
            .perform(excalidraw_native_core::Command::SelectAll);
    }

    /// A finger or mouse held at the view point `(x, y)`: the Context menu to
    /// show, `undefined` if the press means nothing. See
    /// [`excalidraw_native_core::Editor::long_press`].
    #[wasm_bindgen(js_name = longPress)]
    pub fn long_press(&self, x: f64, y: f64, kind: PointerKind) -> Option<ContextMenu> {
        let menu = self.inner.borrow_mut().long_press(x, y, kind.into())?;
        Some(match menu {
            excalidraw_native_core::ContextMenu::Element {
                can_group,
                can_ungroup,
            } => ContextMenu {
                element: true,
                can_group,
                can_ungroup,
            },
            excalidraw_native_core::ContextMenu::Canvas => ContextMenu {
                element: false,
                can_group: false,
                can_ungroup: false,
            },
        })
    }

    /// Whether Pencil-only drawing is on; read it after every input, as the
    /// first Pencil input may turn it on.
    #[wasm_bindgen(js_name = pencilOnlyDrawing)]
    pub fn pencil_only_drawing(&self) -> bool {
        self.inner.borrow().pencil_only_drawing()
    }

    /// The user's choice (or the stored one) of Pencil-only drawing.
    #[wasm_bindgen(js_name = setPencilOnlyDrawing)]
    pub fn set_pencil_only_drawing(&self, on: bool) {
        self.inner.borrow_mut().set_pencil_only_drawing(on);
    }
}

/// The Context menu to show; see [`excalidraw_native_core::ContextMenu`].
#[wasm_bindgen]
pub struct ContextMenu {
    /// The Element menu (else the Canvas's).
    #[wasm_bindgen(readonly)]
    pub element: bool,
    #[wasm_bindgen(readonly, js_name = canGroup)]
    pub can_group: bool,
    #[wasm_bindgen(readonly, js_name = canUngroup)]
    pub can_ungroup: bool,
}

mirror_enum! {
    /// What to export as an image.
    ExportScope { Scene, Selection }
}

/// An image export for the client to rasterise; see
/// [`excalidraw_native_core::Export`].
#[wasm_bindgen]
pub struct Export {
    geometry: Option<Geometry>,
    /// The Scene rectangle the image shows, in Scene units.
    #[wasm_bindgen(readonly)]
    pub x: f64,
    #[wasm_bindgen(readonly)]
    pub y: f64,
    #[wasm_bindgen(readonly)]
    pub width: f64,
    #[wasm_bindgen(readonly)]
    pub height: f64,
}

#[wasm_bindgen]
impl Export {
    /// The geometry of what is exported; it can be taken once.
    #[wasm_bindgen(js_name = takeGeometry)]
    pub fn take_geometry(&mut self) -> Option<Geometry> {
        self.geometry.take()
    }
}

impl Editor {
    fn set_property(&self, property: excalidraw_native_core::Property) {
        self.inner.borrow_mut().set_property(property);
    }
}

/// An image to paste or drop; see [`excalidraw_native_core::ImageFile`].
#[wasm_bindgen(getter_with_clone)]
pub struct ImageFile {
    #[wasm_bindgen(js_name = fileId)]
    pub file_id: String,
    #[wasm_bindgen(js_name = mimeType)]
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[wasm_bindgen]
impl ImageFile {
    #[wasm_bindgen(constructor)]
    pub fn new(
        file_id: String,
        mime_type: String,
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            file_id,
            mime_type,
            bytes,
            width,
            height,
        }
    }
}

impl From<ImageFile> for excalidraw_native_core::ImageFile {
    fn from(file: ImageFile) -> Self {
        Self {
            file_id: file.file_id,
            mime_type: file.mime_type,
            bytes: file.bytes,
            width: file.width,
            height: file.height,
        }
    }
}

/// The text view to show over the Canvas while a text is edited, in view
/// coordinates; see [`excalidraw_native_core::TextEditor`].
#[wasm_bindgen(getter_with_clone)]
pub struct TextEditor {
    /// The text as typed: what the text view starts with.
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[wasm_bindgen(js_name = fontSize)]
    pub font_size: f64,
    #[wasm_bindgen(js_name = lineHeight)]
    pub line_height: f64,
    #[wasm_bindgen(js_name = fontFamily)]
    pub font_family: u32,
    pub align: TextAlign,
    /// `0xRRGGBBAA`.
    pub color: u32,
    /// `0..1`.
    pub opacity: f64,
    /// Clockwise, in radians.
    pub angle: f64,
    pub zoom: f64,
    #[wasm_bindgen(js_name = fixedWidth)]
    pub fixed_width: bool,
}

/// What the properties panel shows: `undefined` where the Selection's
/// Elements differ. See [`excalidraw_native_core::Properties`].
#[wasm_bindgen(getter_with_clone)]
pub struct Properties {
    #[wasm_bindgen(js_name = strokeColor)]
    pub stroke_color: Option<String>,
    #[wasm_bindgen(js_name = backgroundColor)]
    pub background_color: Option<String>,
    #[wasm_bindgen(js_name = fillStyle)]
    pub fill_style: Option<FillPattern>,
    #[wasm_bindgen(js_name = strokeWidth)]
    pub stroke_width: Option<StrokeWidth>,
    #[wasm_bindgen(js_name = strokeStyle)]
    pub stroke_style: Option<LineStyle>,
    pub sloppiness: Option<Sloppiness>,
    pub edges: Option<Edges>,
    /// `0` to `100`.
    pub opacity: Option<f64>,
    #[wasm_bindgen(js_name = fontSize)]
    pub font_size: Option<FontSize>,
    #[wasm_bindgen(js_name = textAlign)]
    pub text_align: Option<TextAlign>,
    /// Which sections the panel shows.
    pub sections: Sections,
}

/// The sections of the panel that apply.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Sections {
    #[wasm_bindgen(js_name = strokeColor)]
    pub stroke_color: bool,
    #[wasm_bindgen(js_name = backgroundColor)]
    pub background_color: bool,
    #[wasm_bindgen(js_name = fillStyle)]
    pub fill_style: bool,
    #[wasm_bindgen(js_name = strokeWidth)]
    pub stroke_width: bool,
    #[wasm_bindgen(js_name = strokeStyle)]
    pub stroke_style: bool,
    pub sloppiness: bool,
    pub edges: bool,
    pub opacity: bool,
    pub text: bool,
}

/// How Excalidraw shows a CSS colour in its dark theme, for swatches.
#[wasm_bindgen(js_name = darkModeColor)]
pub fn dark_mode_color(css: &str) -> String {
    excalidraw_native_core::dark_mode_color(css)
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
