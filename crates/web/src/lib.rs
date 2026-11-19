//! Wasm exports of the core for the web app (ADR-0014). Names mirror the
//! UniFFI surface in `crates/bindings` (in camelCase, as the TS client writes
//! them); a change that surface gets and the web app needs is made here too.
//!
//! Wasm is single-threaded, so unlike in the bindings the editor needs no lock.

use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use std::cell::RefCell;
use std::io;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// A JS object that measures text for the core: the client's font
    /// machinery. See [`pkm_drawing::TextMeasurer`].
    pub type TextMeasurer;

    /// The advance width in Scene units of `text`, a single line, at `font_size`.
    #[wasm_bindgen(method, js_name = lineWidth)]
    fn line_width(this: &TextMeasurer, text: &str, font_size: f64) -> f64;

    /// `Date.now()`: milliseconds since 1970.
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now() -> f64;
}

/// Runs when the module loads: the browser has no system time for the core to
/// ask for, and anything that makes random ids needs it.
#[wasm_bindgen(start)]
fn start() {
    pkm_drawing::set_clock(date_now);
}

/// Lets the core call a client's measurer.
struct Measurer(TextMeasurer);

// SAFETY: wasm is single-threaded, so the JS object is never touched from
// another thread.
unsafe impl Send for Measurer {}
unsafe impl Sync for Measurer {}

impl pkm_drawing::TextMeasurer for Measurer {
    fn line_width(&self, text: &str, font_size: f64) -> f64 {
        self.0.line_width(text, font_size)
    }
}

/// Mirrors of the core's input and Tool enums: `wasm-bindgen` exports its own
/// C-like enums, so each core enum has one here with the same variants, as
/// UniFFI's remote enums do for the bindings.
macro_rules! mirror_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),* $(,)? }) => {
        mirror_enum!($(#[$meta])* $name in pkm_drawing::$name { $($variant),* });
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
mirror_enum!(FillPattern in pkm_drawing::scene::FillPattern { Hachure, CrossHatch, Zigzag, Solid });
mirror_enum!(LineStyle in pkm_drawing::scene::LineStyle { Solid, Dashed, Dotted });
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
mirror_enum!(TextAlign in pkm_drawing::TextAlign { Left, Center, Right });
mirror_enum!(
    /// Where a layer command moves the Selection in the z-order.
    ZOrder { Forward, Backward, ToFront, ToBack }
);
mirror_enum!(
    /// Which edges or middles an align command lines up.
    Alignment { Left, CenterHorizontally, Right, Top, CenterVertically, Bottom }
);
mirror_enum!(
    /// Along which axis a distribute command spaces the Selection.
    Distribution { Horizontally, Vertically }
);

mod markdown;
mod notes;
mod tasks;

/// Modifier keys as a bit set: shift 1, ctrl 2, alt 4, meta 8.
fn modifiers(bits: u8) -> pkm_drawing::input::Modifiers {
    pkm_drawing::input::Modifiers {
        shift: bits & 1 != 0,
        ctrl: bits & 2 != 0,
        alt: bits & 4 != 0,
        meta: bits & 8 != 0,
    }
}

/// What the Canvas shows; see [`pkm_drawing::Viewport`].
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

/// Handle to the core's editor; see [`pkm_drawing::Editor`].
#[wasm_bindgen]
pub struct Editor {
    inner: RefCell<pkm_drawing::Editor>,
}

#[wasm_bindgen]
impl Editor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(pkm_drawing::Editor::new()),
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
    /// [`pkm_drawing::Editor::set_text_measurer`]. Call it before
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
    /// edited; see [`pkm_drawing::Editor::text_editor`]. Read it
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

    /// Takes in `bytes`, the Drawing as Sync rewrote its file, without
    /// reloading and without an undo step; see
    /// [`pkm_drawing::Editor::merge_synced`]. Throws if `bytes`
    /// are not a Drawing; nothing changes then.
    #[wasm_bindgen(js_name = mergeSynced)]
    pub fn merge_synced(&self, bytes: &[u8]) -> Result<(), JsError> {
        self.inner
            .borrow_mut()
            .merge_synced(bytes)
            .map_err(|error| JsError::new(&error.to_string()))
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

    /// Frames all shown Elements if none is in view; once after opening.
    #[wasm_bindgen(js_name = revealContent)]
    pub fn reveal_content(&self) {
        self.inner.borrow_mut().reveal_content();
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
    /// [`pkm_drawing::Editor::pointer`]. `modifiers` is a bit set:
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
            .pointer(pkm_drawing::input::PointerEvent {
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
            .scroll(pkm_drawing::input::ScrollEvent {
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
            .magnify(pkm_drawing::input::MagnifyEvent {
                x,
                y,
                magnification,
            });
    }

    /// Performs the Command the core binds to the key `key` (see
    /// [`pkm_drawing::Shortcut::key`]) with the modifier bits
    /// `modifiers`; `false` if no Command is.
    #[wasm_bindgen(js_name = pressKey)]
    pub fn press_key(&self, key: &str, modifiers_bits: u8) -> bool {
        let held = modifiers(modifiers_bits);
        let Some(shortcut) = pkm_drawing::shortcuts()
            .into_iter()
            .find(|s| s.key == key && s.modifiers == held)
        else {
            return false;
        };
        self.inner.borrow_mut().perform(shortcut.command);
        true
    }

    /// The clipboard text for copying the Selection; `undefined` with nothing
    /// selected. See [`pkm_drawing::Editor::copy_selection`].
    #[wasm_bindgen(js_name = copySelection)]
    pub fn copy_selection(&self) -> Option<String> {
        self.inner.borrow().copy_selection()
    }

    /// Copies the Selection and deletes it. See
    /// [`pkm_drawing::Editor::cut_selection`].
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

    /// A mouse or Pencil hovers at the view point `(x, y)`; see
    /// [`pkm_drawing::Editor::hover`]. `modifier_bits` as in [`Editor::pointer`].
    pub fn hover(&self, x: f64, y: f64, kind: PointerKind, modifier_bits: u8) {
        self.inner
            .borrow_mut()
            .hover(x, y, kind.into(), modifiers(modifier_bits));
    }

    /// Nothing hovers over the Canvas any more.
    #[wasm_bindgen(js_name = endHover)]
    pub fn end_hover(&self) {
        self.inner.borrow_mut().end_hover();
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
    /// [`pkm_drawing::Editor::insert_images`].
    #[wasm_bindgen(js_name = insertImages)]
    pub fn insert_images(&self, images: Vec<ImageFile>) {
        self.inner
            .borrow_mut()
            .insert_images(images.into_iter().map(Into::into).collect());
    }

    /// Puts dropped images around the point `(x, y)` of the view; see
    /// [`pkm_drawing::Editor::insert_images_at`].
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
    /// [`pkm_drawing::Editor::properties`]. Read it after every
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
                align: s.align,
                distribute: s.distribute,
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
        self.set_property(pkm_drawing::Property::StrokeColor(color));
    }

    #[wasm_bindgen(js_name = setBackgroundColor)]
    pub fn set_background_color(&self, color: String) {
        self.set_property(pkm_drawing::Property::BackgroundColor(color));
    }

    #[wasm_bindgen(js_name = setFillStyle)]
    pub fn set_fill_style(&self, value: FillPattern) {
        self.set_property(pkm_drawing::Property::FillStyle(value.into()));
    }

    #[wasm_bindgen(js_name = setStrokeWidth)]
    pub fn set_stroke_width(&self, value: StrokeWidth) {
        self.set_property(pkm_drawing::Property::StrokeWidth(value.into()));
    }

    #[wasm_bindgen(js_name = setStrokeStyle)]
    pub fn set_stroke_style(&self, value: LineStyle) {
        self.set_property(pkm_drawing::Property::StrokeStyle(value.into()));
    }

    #[wasm_bindgen(js_name = setSloppiness)]
    pub fn set_sloppiness(&self, value: Sloppiness) {
        self.set_property(pkm_drawing::Property::Sloppiness(value.into()));
    }

    #[wasm_bindgen(js_name = setEdges)]
    pub fn set_edges(&self, value: Edges) {
        self.set_property(pkm_drawing::Property::Edges(value.into()));
    }

    /// `0` to `100`.
    #[wasm_bindgen(js_name = setOpacity)]
    pub fn set_opacity(&self, value: f64) {
        self.set_property(pkm_drawing::Property::Opacity(value));
    }

    #[wasm_bindgen(js_name = setFontSize)]
    pub fn set_font_size(&self, value: FontSize) {
        self.set_property(pkm_drawing::Property::FontSize(value.into()));
    }

    #[wasm_bindgen(js_name = setTextAlign)]
    pub fn set_text_align(&self, value: TextAlign) {
        self.set_property(pkm_drawing::Property::TextAlign(value.into()));
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

    /// Lines up the Selection (see [`pkm_drawing::Editor::align`]).
    pub fn align(&self, how: Alignment) {
        self.inner.borrow_mut().align(how.into());
    }

    /// Spaces the Selection evenly (see [`pkm_drawing::Editor::distribute`]).
    pub fn distribute(&self, how: Distribution) {
        self.inner.borrow_mut().distribute(how.into());
    }

    /// Groups the Selection (see [`pkm_drawing::Editor::group`]).
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
            .perform(pkm_drawing::Command::SelectAll);
    }

    /// A finger or mouse held at the view point `(x, y)`: the Context menu to
    /// show, `undefined` if the press means nothing. See
    /// [`pkm_drawing::Editor::long_press`].
    #[wasm_bindgen(js_name = longPress)]
    pub fn long_press(&self, x: f64, y: f64, kind: PointerKind) -> Option<ContextMenu> {
        let menu = self.inner.borrow_mut().long_press(x, y, kind.into())?;
        Some(match menu {
            pkm_drawing::ContextMenu::Element {
                can_group,
                can_ungroup,
            } => ContextMenu {
                element: true,
                can_group,
                can_ungroup,
            },
            pkm_drawing::ContextMenu::Canvas => ContextMenu {
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

/// The Context menu to show; see [`pkm_drawing::ContextMenu`].
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
/// [`pkm_drawing::Export`].
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
    fn set_property(&self, property: pkm_drawing::Property) {
        self.inner.borrow_mut().set_property(property);
    }
}

/// An image to paste or drop; see [`pkm_drawing::ImageFile`].
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

impl From<ImageFile> for pkm_drawing::ImageFile {
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
/// coordinates; see [`pkm_drawing::TextEditor`].
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
/// Elements differ. See [`pkm_drawing::Properties`].
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
    pub align: bool,
    pub distribute: bool,
}

/// Whether two Drawings' files are the same Drawing (they share Elements, or
/// are both empty); see [`pkm_drawing::are_same_drawing`].
#[wasm_bindgen(js_name = areSameDrawing)]
pub fn are_same_drawing(a: &[u8], b: &[u8]) -> bool {
    matches!(pkm_drawing::are_same_drawing(a, b), Ok(true))
}

/// The Item ID of the Drawing `bytes`, `undefined` if they hold none (or are
/// no Drawing).
#[wasm_bindgen(js_name = drawingItemId)]
pub fn drawing_item_id(bytes: &[u8]) -> Option<String> {
    pkm_drawing::item_id::of(bytes).map(|id| id.to_string())
}

/// The Drawing `bytes` with `item_id` as its Item ID, everything else as it
/// was. Throws if `bytes` are no Drawing or `item_id` is no Item ID.
#[wasm_bindgen(js_name = drawingWithItemId)]
pub fn drawing_with_item_id(bytes: &[u8], item_id: &str) -> Result<Vec<u8>, JsError> {
    let id = ItemId::parse(item_id).ok_or_else(|| JsError::new("not an Item ID"))?;
    pkm_drawing::item_id::with(bytes, &id).map_err(|e| JsError::new(&e.to_string()))
}

/// A new Item ID.
#[wasm_bindgen(js_name = newItemId)]
pub fn new_item_id() -> String {
    ItemId::new(&mut Entropy::from_system()).to_string()
}

/// The new Item IDs that Items need, given the ones they hold now, oldest
/// first (`""`: holds none): each entry is `""` where the Item keeps its own,
/// and the new Item ID where it has none or the same as an older Item.
#[wasm_bindgen(js_name = claimItemIds)]
pub fn claim_item_ids(oldest_first: Vec<String>) -> Vec<String> {
    pkm_base::item_id::claim(&oldest_first)
        .into_iter()
        .map(|id| id.map(|id| id.to_string()).unwrap_or_default())
        .collect()
}

/// How Excalidraw shows a CSS colour in its dark theme, for swatches.
#[wasm_bindgen(js_name = darkModeColor)]
pub fn dark_mode_color(css: &str) -> String {
    pkm_drawing::dark_mode_color(css)
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
    inner: pkm_drawing::Geometry,
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
        size_of::<pkm_drawing::Part>() as u32
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
        size_of::<pkm_drawing::TextLine>() as u32
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
        size_of::<pkm_drawing::Image>() as u32
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

// ---- The WebDAV protocol (ADR-0016) ----

use pkm_cloud::ConnectError;
use pkm_cloud::webdav_protocol::{Exchange, HttpAnswer, HttpRequest, Turn, WebDav};

/// A request for the client to send with `fetch`.
#[wasm_bindgen]
pub struct DavRequest(HttpRequest);

#[wasm_bindgen]
impl DavRequest {
    /// `PROPFIND`, `GET`, `PUT`, `MKCOL`, `MOVE` or `DELETE`.
    #[wasm_bindgen(getter)]
    pub fn method(&self) -> String {
        self.0.method.to_owned()
    }

    #[wasm_bindgen(getter)]
    pub fn url(&self) -> String {
        self.0.url.clone()
    }

    /// Name, value, name, value, …
    #[wasm_bindgen(getter)]
    pub fn headers(&self) -> Vec<String> {
        self.0
            .headers
            .iter()
            .flat_map(|(name, value)| [(*name).to_owned(), value.clone()])
            .collect()
    }

    #[wasm_bindgen(getter)]
    pub fn body(&self) -> Vec<u8> {
        self.0.body.clone()
    }
}

/// How an operation ended.
#[wasm_bindgen]
#[derive(Default)]
pub struct DavOutcome {
    error: Option<&'static str>,
    message: String,
    data: Option<Vec<u8>>,
    names: Vec<String>,
    markers: Vec<String>,
    empty_dirs: Vec<String>,
}

#[wasm_bindgen]
impl DavOutcome {
    /// Nothing if it worked; else one of `wrongCredentials`, `unreachable`,
    /// `notHttps`, `notWebDav` (connecting), `accessLost`, `full`, `offline`
    /// (the request could not be sent) or `failed`.
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.map(str::to_owned)
    }

    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// The content of a `get`; nothing if there is no such blob.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Option<Vec<u8>> {
        self.data.clone()
    }

    /// The paths of the blobs a `scan` found.
    #[wasm_bindgen(getter)]
    pub fn names(&self) -> Vec<String> {
        self.names.clone()
    }

    /// Their markers, in the order of `names`.
    #[wasm_bindgen(getter)]
    pub fn markers(&self) -> Vec<String> {
        self.markers.clone()
    }

    /// The paths of the empty directories a `scan` found.
    #[wasm_bindgen(getter, js_name = emptyDirs)]
    pub fn empty_dirs(&self) -> Vec<String> {
        self.empty_dirs.clone()
    }
}

impl DavOutcome {
    fn failure(error: &io::Error) -> Self {
        use io::ErrorKind::*;
        Self {
            error: Some(match error.kind() {
                PermissionDenied => "accessLost",
                StorageFull => "full",
                NotConnected => "offline",
                _ => "failed",
            }),
            message: error.to_string(),
            ..Self::default()
        }
    }

    fn of<T>(result: io::Result<T>, success: impl FnOnce(T, &mut Self)) -> Self {
        match result {
            Ok(value) => {
                let mut outcome = Self::default();
                success(value, &mut outcome);
                outcome
            }
            Err(error) => Self::failure(&error),
        }
    }

    fn connect(result: Result<(), ConnectError>) -> Self {
        match result {
            Ok(()) => Self::default(),
            Err(error) => Self {
                error: Some(match error {
                    ConnectError::WrongCredentials => "wrongCredentials",
                    ConnectError::NotHttps => "notHttps",
                    ConnectError::NotWebDav => "notWebDav",
                    _ => "unreachable",
                }),
                message: error.to_string(),
                ..Self::default()
            },
        }
    }
}

/// One operation of the protocol, answered request by request.
trait Operation {
    fn start(&mut self) -> HttpRequest;
    fn answer(&mut self, answer: HttpAnswer) -> Turn<DavOutcome>;
    fn failed(&mut self) -> DavOutcome;
}

struct Mapped<E: Exchange> {
    exchange: E,
    outcome: fn(E::Output) -> DavOutcome,
}

impl<E: Exchange> Operation for Mapped<E> {
    fn start(&mut self) -> HttpRequest {
        self.exchange.start()
    }

    fn answer(&mut self, answer: HttpAnswer) -> Turn<DavOutcome> {
        match self.exchange.answer(answer) {
            Turn::Send(request) => Turn::Send(request),
            Turn::Done(result) => Turn::Done((self.outcome)(result)),
        }
    }

    fn failed(&mut self) -> DavOutcome {
        let error = io::Error::new(io::ErrorKind::NotConnected, "the request was not sent");
        (self.outcome)(self.exchange.failed(error))
    }
}

/// One operation against the server: `start` gives the first request,
/// `answer` the next one until it gives none, `failed` ends it when a request
/// could not be sent, and `outcome` says how it ended.
#[wasm_bindgen]
pub struct DavExchange {
    operation: Box<dyn Operation>,
    outcome: Option<DavOutcome>,
}

#[wasm_bindgen]
impl DavExchange {
    pub fn start(&mut self) -> DavRequest {
        DavRequest(self.operation.start())
    }

    /// The next request, or nothing when the operation is over.
    pub fn answer(&mut self, status: u16, body: Vec<u8>) -> Option<DavRequest> {
        match self.operation.answer(HttpAnswer { status, body }) {
            Turn::Send(request) => Some(DavRequest(request)),
            Turn::Done(outcome) => {
                self.outcome = Some(outcome);
                None
            }
        }
    }

    pub fn failed(&mut self) {
        self.outcome = Some(self.operation.failed());
    }

    /// How the operation ended; call it once it is over.
    pub fn outcome(&mut self) -> DavOutcome {
        self.outcome.take().expect("the operation is over")
    }
}

impl DavExchange {
    fn new<E: Exchange + 'static>(exchange: E, outcome: fn(E::Output) -> DavOutcome) -> Self {
        Self {
            operation: Box::new(Mapped { exchange, outcome }),
            outcome: None,
        }
    }
}

/// The requests of one WebDAV server.
#[wasm_bindgen]
pub struct WebDavProtocol(WebDav);

#[wasm_bindgen]
impl WebDavProtocol {
    /// The requests for the folder of Drawings. Throws `notHttps` for a URL
    /// that is not https.
    #[wasm_bindgen(constructor)]
    pub fn new(url: &str, username: &str, password: &str) -> Result<WebDavProtocol, JsError> {
        WebDav::new(url, username, password, Drawings::FOLDER)
            .map(Self)
            .map_err(|_| JsError::new("notHttps"))
    }

    /// The requests for the folder of Books. Throws `notHttps` for a URL that
    /// is not https.
    #[wasm_bindgen(js_name = newForBooks)]
    pub fn new_for_books(
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<WebDavProtocol, JsError> {
        WebDav::new(url, username, password, Books::FOLDER)
            .map(Self)
            .map_err(|_| JsError::new("notHttps"))
    }

    /// The requests for the folder of Tasks. Throws
    /// `notHttps` for a URL that is not https.
    #[wasm_bindgen(js_name = newForTasks)]
    pub fn new_for_tasks(
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<WebDavProtocol, JsError> {
        WebDav::new(url, username, password, Tasks::FOLDER)
            .map(Self)
            .map_err(|_| JsError::new("notHttps"))
    }

    /// The requests for the folder of Notes. Throws `notHttps` for a URL that
    /// is not https.
    #[wasm_bindgen(js_name = newForNotes)]
    pub fn new_for_notes(
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<WebDavProtocol, JsError> {
        WebDav::new(url, username, password, NoteFiles::FOLDER)
            .map(Self)
            .map_err(|_| JsError::new("notHttps"))
    }

    /// The requests for the folder of the Tags file. Throws `notHttps` for a URL
    /// that is not https.
    #[wasm_bindgen(js_name = newForTags)]
    pub fn new_for_tags(
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<WebDavProtocol, JsError> {
        WebDav::new(url, username, password, TagsFile::FOLDER)
            .map(Self)
            .map_err(|_| JsError::new("notHttps"))
    }

    /// The requests for the folder of the Filters. Throws `notHttps` for a URL
    /// that is not https.
    #[wasm_bindgen(js_name = newForFilters)]
    pub fn new_for_filters(
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<WebDavProtocol, JsError> {
        WebDav::new(url, username, password, Filters::FOLDER)
            .map(Self)
            .map_err(|_| JsError::new("notHttps"))
    }

    /// Checks the server and makes its `pkm/Drawings` folder.
    pub fn connect(&self) -> DavExchange {
        DavExchange::new(self.0.connect(), DavOutcome::connect)
    }

    /// Everything in the folder and the directories below it.
    pub fn scan(&self) -> DavExchange {
        DavExchange::new(self.0.scan(), |result| {
            DavOutcome::of(result, |tree, outcome| {
                (outcome.names, outcome.markers) =
                    tree.blobs.into_iter().map(|b| (b.name, b.marker)).unzip();
                outcome.empty_dirs = tree.empty_dirs;
            })
        })
    }

    /// Makes the directory `path` and the ones above it.
    #[wasm_bindgen(js_name = createDir)]
    pub fn create_dir(&self, path: &str) -> DavExchange {
        DavExchange::new(self.0.create_dir(path), |result| {
            DavOutcome::of(result, |(), _| {})
        })
    }

    pub fn get(&self, name: &str) -> DavExchange {
        DavExchange::new(self.0.get(name), |result| {
            DavOutcome::of(result, |data, outcome| outcome.data = data)
        })
    }

    pub fn delete(&self, name: &str) -> DavExchange {
        DavExchange::new(self.0.delete(name), |result| {
            DavOutcome::of(result, |(), _| {})
        })
    }

    /// `temp` is a name no other upload uses; `incomingExists` says the
    /// folder for uploads is known to be there.
    pub fn put(&self, name: &str, temp: &str, data: Vec<u8>, incoming_exists: bool) -> DavExchange {
        DavExchange::new(self.0.put(name, temp, data, incoming_exists), |result| {
            DavOutcome::of(result, |(), _| {})
        })
    }
}

// ---- The Sync step and its schedule (ADR-0016) ----

use pkm_cloud::step::{LocalFile, Marks, Synced};
use pkm_cloud::{
    Blob, BookStates, Books, Drawings, Filters, Next, Notes as NoteFiles, Request, Response,
    Schedule, Status, StepOutcome, SyncStep, TagsFile, Tasks, Timing,
};
use std::time::Duration;

/// What a Sync step asks the client to do; `kind` says which request it is
/// and the other fields are the ones it uses (the others stay empty).
///
/// Kinds: `listCloud`, `listLocal`, `readLocal` (file), `readMarks` (file),
/// `readSynced` (file), `listSynced`, `writeSynced` (file, bytes, marker),
/// `writeStamp` (file, stamp), `forgetSynced` (file), `download` (file),
/// `upload` (file, bytes), `deleteCloud` (file), `writeLocal` (file, bytes,
/// expected: none means only if absent), `renameLocal` (file, to, expected)
/// and `deleteLocal` (file, expected). Names are file names.
#[wasm_bindgen(getter_with_clone)]
pub struct SyncRequest {
    pub kind: String,
    pub file: String,
    pub to: String,
    pub bytes: Vec<u8>,
    pub expected: Option<Vec<u8>>,
    pub marker: String,
    pub stamp: String,
}

impl SyncRequest {
    fn of(kind: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            file: String::new(),
            to: String::new(),
            bytes: Vec::new(),
            expected: None,
            marker: String::new(),
            stamp: String::new(),
        }
    }

    fn from(request: Request) -> Self {
        match request {
            Request::ListCloud => Self::of("listCloud"),
            Request::ListLocal => Self::of("listLocal"),
            Request::ListSynced => Self::of("listSynced"),
            Request::ReadLocal { file } => Self {
                file,
                ..Self::of("readLocal")
            },
            Request::ReadMarks { file } => Self {
                file,
                ..Self::of("readMarks")
            },
            Request::ReadSynced { file } => Self {
                file,
                ..Self::of("readSynced")
            },
            Request::WriteSynced {
                file,
                bytes,
                marker,
            } => Self {
                file,
                bytes,
                marker,
                ..Self::of("writeSynced")
            },
            Request::WriteStamp { file, stamp } => Self {
                file,
                stamp,
                ..Self::of("writeStamp")
            },
            Request::ForgetSynced { file } => Self {
                file,
                ..Self::of("forgetSynced")
            },
            Request::Download { file } => Self {
                file,
                ..Self::of("download")
            },
            Request::Upload { file, bytes } => Self {
                file,
                bytes,
                ..Self::of("upload")
            },
            Request::DeleteCloud { file } => Self {
                file,
                ..Self::of("deleteCloud")
            },
            Request::WriteLocal {
                file,
                bytes,
                expected,
            } => Self {
                file,
                bytes,
                expected,
                ..Self::of("writeLocal")
            },
            Request::RenameLocal { from, to, expected } => Self {
                file: from,
                to,
                expected: Some(expected),
                ..Self::of("renameLocal")
            },
            Request::DeleteLocal { file, expected } => Self {
                file,
                expected: Some(expected),
                ..Self::of("deleteLocal")
            },
        }
    }
}

/// How far a step is through the Drawings it transfers.
#[wasm_bindgen]
pub struct SyncProgress {
    #[wasm_bindgen(readonly)]
    pub done: u32,
    #[wasm_bindgen(readonly)]
    pub total: u32,
}

/// How a step ended.
#[wasm_bindgen(getter_with_clone)]
pub struct SyncOutcome {
    /// The Drawings whose local file the step wrote or deleted (a rename as
    /// the old and the new name), as file names.
    pub changed: Vec<String>,
    /// The first thing that went wrong, if anything did.
    pub error: Option<String>,
    #[wasm_bindgen(js_name = accessLost)]
    pub access_lost: bool,
    #[wasm_bindgen(js_name = storageFull)]
    pub storage_full: bool,
    /// What the user is told: `upToDate`, `offline`, `accessLost` or `full`.
    pub status: String,
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::UpToDate => "upToDate",
        Status::Syncing => "syncing",
        Status::Offline => "offline",
        Status::AccessLost => "accessLost",
        Status::Full => "full",
    }
}

/// One Sync step: `start` gives its first request, each `answer…` hands back
/// the result of the last one and gives the next, until there is none and
/// `outcome` says what the step did. A failed request is told with `fail`.
#[wasm_bindgen]
pub struct SyncRun {
    step: SyncStep,
    outcome: Option<StepOutcome>,
}

#[wasm_bindgen]
impl SyncRun {
    /// `nowMs`: milliseconds since the epoch.
    #[wasm_bindgen(constructor)]
    pub fn new(now_ms: f64) -> SyncRun {
        SyncRun::with_merge(now_ms, Arc::new(Drawings))
    }

    /// A step over the Book Files: transferred, never merged.
    #[wasm_bindgen(js_name = newForBooks)]
    pub fn new_for_books(now_ms: f64) -> SyncRun {
        SyncRun::with_merge(now_ms, Arc::new(Books))
    }

    /// A step over the Books' state files, merged by the Books merge hook.
    #[wasm_bindgen(js_name = newForBookStates)]
    pub fn new_for_book_states(now_ms: f64) -> SyncRun {
        SyncRun::with_merge(now_ms, Arc::new(BookStates))
    }

    /// A step over the Task files, merged field by field (ADR-0021).
    #[wasm_bindgen(js_name = newForTasks)]
    pub fn new_for_tasks(now_ms: f64) -> SyncRun {
        SyncRun::with_merge(now_ms, Arc::new(Tasks))
    }

    /// A step over the Note files, merged field by field, the Body by line (ADR-0032).
    #[wasm_bindgen(js_name = newForNotes)]
    pub fn new_for_notes(now_ms: f64) -> SyncRun {
        SyncRun::with_merge(now_ms, Arc::new(NoteFiles))
    }

    /// A step over the Tags file, merged per Tag (ADR-0023).
    #[wasm_bindgen(js_name = newForTags)]
    pub fn new_for_tags(now_ms: f64) -> SyncRun {
        SyncRun::with_merge(now_ms, Arc::new(TagsFile))
    }

    /// A step over the Filter files, merged field by field, their order per
    /// Task (ADR-0024).
    #[wasm_bindgen(js_name = newForFilters)]
    pub fn new_for_filters(now_ms: f64) -> SyncRun {
        SyncRun::with_merge(now_ms, Arc::new(Filters))
    }

    pub fn start(&mut self) -> Option<SyncRequest> {
        self.advance(None)
    }

    /// The Drawings of the cloud folder: names and markers in the same order.
    #[wasm_bindgen(js_name = answerCloud)]
    pub fn answer_cloud(
        &mut self,
        names: Vec<String>,
        markers: Vec<String>,
    ) -> Option<SyncRequest> {
        let blobs = names
            .into_iter()
            .zip(markers)
            .map(|(name, marker)| Blob { name, marker })
            .collect();
        self.advance(Some(Ok(Response::Cloud(blobs))))
    }

    /// The local Drawings: file names, stamps and modification times (ms) in
    /// the same order.
    #[wasm_bindgen(js_name = answerLocal)]
    pub fn answer_local(
        &mut self,
        files: Vec<String>,
        stamps: Vec<String>,
        mtimes: Vec<f64>,
    ) -> Option<SyncRequest> {
        let local = files
            .into_iter()
            .zip(stamps)
            .zip(mtimes)
            .map(|((file, stamp), mtime)| LocalFile {
                file,
                stamp,
                mtime_ms: mtime as u64,
            })
            .collect();
        self.advance(Some(Ok(Response::Local(local))))
    }

    #[wasm_bindgen(js_name = answerNames)]
    pub fn answer_names(&mut self, names: Vec<String>) -> Option<SyncRequest> {
        self.advance(Some(Ok(Response::Names(names))))
    }

    /// For reads and downloads: none if there is nothing.
    #[wasm_bindgen(js_name = answerBytes)]
    pub fn answer_bytes(&mut self, bytes: Option<Vec<u8>>) -> Option<SyncRequest> {
        self.advance(Some(Ok(Response::Bytes(bytes))))
    }

    #[wasm_bindgen(js_name = answerMarks)]
    pub fn answer_marks(
        &mut self,
        marker: Option<String>,
        stamp: Option<String>,
    ) -> Option<SyncRequest> {
        self.advance(Some(Ok(Response::Marks(Marks { marker, stamp }))))
    }

    /// The remembered bytes with their marker; none unless both are there.
    #[wasm_bindgen(js_name = answerSynced)]
    pub fn answer_synced(
        &mut self,
        bytes: Option<Vec<u8>>,
        marker: Option<String>,
    ) -> Option<SyncRequest> {
        let synced = bytes
            .zip(marker)
            .map(|(bytes, marker)| Synced { bytes, marker });
        self.advance(Some(Ok(Response::Synced(synced))))
    }

    /// Whether a guarded write, rename or delete took place.
    #[wasm_bindgen(js_name = answerHappened)]
    pub fn answer_happened(&mut self, happened: bool) -> Option<SyncRequest> {
        self.advance(Some(Ok(Response::Happened(happened))))
    }

    #[wasm_bindgen(js_name = answerDone)]
    pub fn answer_done(&mut self) -> Option<SyncRequest> {
        self.advance(Some(Ok(Response::Done)))
    }

    /// The request failed: `accessLost` (rejected credentials), `full` (no
    /// space) or anything else, which reads as being offline.
    pub fn fail(&mut self, reason: &str, message: &str) -> Option<SyncRequest> {
        let kind = match reason {
            "accessLost" => io::ErrorKind::PermissionDenied,
            "full" => io::ErrorKind::StorageFull,
            _ => io::ErrorKind::NotConnected,
        };
        self.advance(Some(Err(io::Error::new(kind, message.to_owned()))))
    }

    /// How far the transfers are, once their number is known.
    pub fn progress(&self) -> Option<SyncProgress> {
        self.step.progress().map(|p| SyncProgress {
            done: p.done,
            total: p.total,
        })
    }

    /// How the step ended; call it once there is no next request.
    pub fn outcome(&mut self) -> SyncOutcome {
        let outcome = self.outcome.take().expect("the step is over");
        SyncOutcome {
            status: status_name(Status::after(&outcome)).to_owned(),
            changed: outcome.changed,
            error: outcome.error.map(|e| e.to_string()),
            access_lost: outcome.access_lost,
            storage_full: outcome.storage_full,
        }
    }
}

impl SyncRun {
    fn with_merge(now_ms: f64, merge: Arc<dyn pkm_cloud::Merge>) -> SyncRun {
        SyncRun {
            step: SyncStep::new(now_ms as u64, merge),
            outcome: None,
        }
    }

    fn advance(&mut self, reply: Option<io::Result<Response>>) -> Option<SyncRequest> {
        match self.step.resume(reply) {
            Next::Request(request) => Some(SyncRequest::from(request)),
            Next::Done(outcome) => {
                self.outcome = Some(outcome);
                None
            }
        }
    }
}

/// When Sync steps run (default timing: a step 3 s after the last change, a
/// poll every 10 s while in front). Times are milliseconds on any clock the
/// client keeps to.
#[wasm_bindgen]
pub struct SyncSchedule(Schedule);

#[wasm_bindgen]
impl SyncSchedule {
    /// In front, with a step due at once.
    #[wasm_bindgen(constructor)]
    pub fn new(now: f64) -> SyncSchedule {
        SyncSchedule(Schedule::new(Timing::default(), ms(now)))
    }

    #[wasm_bindgen(js_name = cameToFront)]
    pub fn came_to_front(&mut self, now: f64) {
        self.0.came_to_front(ms(now));
    }

    #[wasm_bindgen(js_name = wentToBackground)]
    pub fn went_to_background(&mut self) {
        self.0.went_to_background();
    }

    #[wasm_bindgen(js_name = localChange)]
    pub fn local_change(&mut self, now: f64) {
        self.0.local_change(ms(now));
    }

    #[wasm_bindgen(js_name = setOnline)]
    pub fn set_online(&mut self, online: bool, now: f64) {
        self.0.set_online(online, ms(now));
    }

    #[wasm_bindgen(js_name = accessLost)]
    pub fn access_lost(&mut self) {
        self.0.access_lost();
    }

    pub fn reconnected(&mut self, now: f64) {
        self.0.reconnected(ms(now));
    }

    /// Milliseconds until the next step; none if there won't be one.
    pub fn wait(&self, now: f64) -> Option<f64> {
        self.0.wait(ms(now)).map(|d| d.as_millis() as f64)
    }

    /// Whether a step runs now; if so the schedule counts it as running.
    #[wasm_bindgen(js_name = takeDue)]
    pub fn take_due(&mut self, now: f64) -> bool {
        self.0.take_due(ms(now))
    }

    #[wasm_bindgen(js_name = stepEnded)]
    pub fn step_ended(&mut self, now: f64, succeeded: bool) {
        self.0.step_ended(ms(now), succeeded);
    }
}

fn ms(millis: f64) -> Duration {
    Duration::from_millis(millis.max(0.0) as u64)
}
