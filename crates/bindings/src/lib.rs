//! UniFFI surface of the core (ADR-0007). Foreign clients see exactly what is
//! exported here; the logic itself lives in `workingtitle-core`.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use workingtitle_core::{
    MagnifyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, ScrollEvent, Tool,
};

uniffi::setup_scaffolding!("WorkingTitleCore");

/// Why a file could not be opened as a Drawing, in words fit for a user.
#[derive(Debug, uniffi::Error)]
pub enum LoadError {
    Invalid { message: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let LoadError::Invalid { message } = self;
        f.write_str(message)
    }
}

impl From<workingtitle_core::LoadError> for LoadError {
    fn from(error: workingtitle_core::LoadError) -> Self {
        LoadError::Invalid {
            message: error.to_string(),
        }
    }
}

// Input events are plain data: UniFFI mirrors the core's definitions as
// records and enums, so there is exactly one definition of each.

#[uniffi::remote(Record)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[uniffi::remote(Enum)]
pub enum PointerKind {
    Finger,
    Pencil,
    Mouse,
}

#[uniffi::remote(Enum)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
}

#[uniffi::remote(Record)]
pub struct PointerEvent {
    pub id: u64,
    pub phase: PointerPhase,
    pub x: f64,
    pub y: f64,
    pub kind: PointerKind,
    pub pressure: f32,
    pub timestamp: f64,
    pub modifiers: Modifiers,
}

#[uniffi::remote(Record)]
pub struct ScrollEvent {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub modifiers: Modifiers,
}

#[uniffi::remote(Record)]
pub struct MagnifyEvent {
    pub x: f64,
    pub y: f64,
    pub magnification: f64,
}

#[uniffi::remote(Enum)]
pub enum Tool {
    Selection,
    Hand,
}

/// What the Canvas shows; see [`workingtitle_core::Viewport`]. Small and
/// cheap enough to read after every input event.
#[derive(uniffi::Record)]
pub struct ViewportState {
    /// Scene point at the Canvas's top-left corner.
    pub origin_x: f64,
    pub origin_y: f64,
    /// View points per Scene unit.
    pub zoom: f64,
}

impl From<workingtitle_core::Viewport> for ViewportState {
    fn from(viewport: workingtitle_core::Viewport) -> Self {
        let (origin_x, origin_y) = viewport.origin();
        Self {
            origin_x,
            origin_y,
            zoom: viewport.zoom(),
        }
    }
}

/// Measures text for the core; implemented by clients with their font
/// machinery. See [`workingtitle_core::TextMeasurer`].
#[uniffi::export(with_foreign)]
pub trait TextMeasurer: Send + Sync {
    /// The advance width in Scene units of `text`, a single line, set in
    /// Excalifont at `font_size`.
    fn line_width(&self, text: String, font_size: f64) -> f64;
}

/// Lets the core call a client's measurer.
struct Measurer(Arc<dyn TextMeasurer>);

impl workingtitle_core::TextMeasurer for Measurer {
    fn line_width(&self, text: &str, font_size: f64) -> f64 {
        self.0.line_width(text.to_owned(), font_size)
    }
}

/// Handle to the core's editor; see [`workingtitle_core::Editor`].
///
/// UniFFI objects are shared, so the editor sits behind a lock; clients drive
/// it from one thread and never contend for it.
#[derive(uniffi::Object)]
pub struct Editor {
    inner: Mutex<workingtitle_core::Editor>,
}

impl Editor {
    fn editor(&self) -> MutexGuard<'_, workingtitle_core::Editor> {
        // A panic while holding the lock cannot leave the editor half-edited
        // in a way that matters more than carrying on does.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[uniffi::export]
impl Editor {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(workingtitle_core::Editor::new()),
        }
    }

    /// Replaces the Scene with the Drawing in `bytes` (a `.excalidraw` file's
    /// content); see [`workingtitle_core::Editor::load`].
    pub fn load(&self, bytes: Vec<u8>) -> Result<(), LoadError> {
        Ok(self.editor().load(&bytes)?)
    }

    /// The Scene as `.excalidraw` file content.
    pub fn save(&self) -> Vec<u8> {
        self.editor().save()
    }

    /// Number of Elements shown in the Scene.
    pub fn element_count(&self) -> u64 {
        self.editor().element_count()
    }

    /// Hands the core the client's way to measure text; see
    /// [`workingtitle_core::Editor::set_text_measurer`]. Call it before
    /// reading geometry that has labels in it.
    pub fn set_text_measurer(&self, measurer: Arc<dyn TextMeasurer>) {
        self.editor()
            .set_text_measurer(Arc::new(Measurer(measurer)));
    }

    /// The part of the Scene the Canvas shows. Read it after forwarding
    /// input; it never changes the geometry, which clients keep.
    pub fn viewport(&self) -> ViewportState {
        self.editor().viewport().into()
    }

    /// Reports the Canvas size in points; call on start and on every resize.
    pub fn set_view_size(&self, width: f64, height: f64) {
        self.editor().set_view_size(width, height);
    }

    /// One zoom step in around the middle of the view.
    pub fn zoom_in(&self) {
        self.editor().zoom_in();
    }

    /// One zoom step out around the middle of the view.
    pub fn zoom_out(&self) {
        self.editor().zoom_out();
    }

    /// Back to 100 % around the middle of the view.
    pub fn reset_zoom(&self) {
        self.editor().reset_zoom();
    }

    /// Frames all shown Elements.
    pub fn zoom_to_fit(&self) {
        self.editor().zoom_to_fit();
    }

    /// The active Tool.
    pub fn tool(&self) -> Tool {
        self.editor().tool()
    }

    pub fn set_tool(&self, tool: Tool) {
        self.editor().set_tool(tool);
    }

    /// A step of a finger, Pencil or mouse; see
    /// [`workingtitle_core::Editor::pointer`].
    pub fn pointer(&self, event: PointerEvent) {
        self.editor().pointer(event);
    }

    /// A scroll wheel or trackpad scroll (Mac).
    pub fn scroll(&self, event: ScrollEvent) {
        self.editor().scroll(event);
    }

    /// A trackpad pinch (Mac).
    pub fn magnify(&self, event: MagnifyEvent) {
        self.editor().magnify(event);
    }

    /// The bytes of the image file `id` as stored in the Drawing, for the
    /// client to decode; `None` if the Drawing has no such file. Read it
    /// when [`Geometry`] names a file the client has no texture for yet.
    pub fn file_bytes(&self, id: String) -> Option<Vec<u8>> {
        self.editor().file_bytes(&id)
    }

    /// The paths to draw for the Scene; see [`workingtitle_core::Editor::geometry`].
    pub fn geometry(&self) -> Arc<Geometry> {
        Arc::new(Geometry {
            inner: self.editor().geometry(),
        })
    }
}

/// Zero-copy geometry (ADR-0007): the format is documented in
/// [`workingtitle_core::geometry`].
///
/// Only sizes and buffer addresses cross UniFFI; the data itself stays where
/// it is and the client reads it in place. An address is valid exactly as long
/// as the client holds this object, and the buffers never change.
#[derive(uniffi::Object)]
pub struct Geometry {
    inner: workingtitle_core::Geometry,
}

#[uniffi::export]
impl Geometry {
    /// Tells whether cached per-Element results are still valid; see
    /// [`workingtitle_core::Geometry::generation`].
    pub fn generation(&self) -> u64 {
        self.inner.generation()
    }

    /// Number of [`workingtitle_core::Part`] records at `parts_address`.
    pub fn part_count(&self) -> u64 {
        self.inner.parts().len() as u64
    }

    /// Size in bytes of one part record, so clients can check that their
    /// mirror of the record has the same layout.
    pub fn part_stride(&self) -> u64 {
        size_of::<workingtitle_core::Part>() as u64
    }

    pub fn parts_address(&self) -> u64 {
        self.inner.parts().as_ptr() as u64
    }

    /// Number of bytes (one verb each) at `verbs_address`.
    pub fn verb_count(&self) -> u64 {
        self.inner.verbs().len() as u64
    }

    pub fn verbs_address(&self) -> u64 {
        self.inner.verbs().as_ptr() as u64
    }

    /// Number of `f32` operands at `coords_address`.
    pub fn coord_count(&self) -> u64 {
        self.inner.coords().len() as u64
    }

    pub fn coords_address(&self) -> u64 {
        self.inner.coords().as_ptr() as u64
    }

    /// Number of [`workingtitle_core::TextLine`] records at
    /// `text_lines_address`.
    pub fn text_line_count(&self) -> u64 {
        self.inner.text_lines().len() as u64
    }

    /// Size in bytes of one text line record; see `part_stride`.
    pub fn text_line_stride(&self) -> u64 {
        size_of::<workingtitle_core::TextLine>() as u64
    }

    pub fn text_lines_address(&self) -> u64 {
        self.inner.text_lines().as_ptr() as u64
    }

    /// Number of bytes (UTF-8) at `text_address`.
    pub fn text_byte_count(&self) -> u64 {
        self.inner.text().len() as u64
    }

    pub fn text_address(&self) -> u64 {
        self.inner.text().as_ptr() as u64
    }

    /// Number of [`workingtitle_core::Image`] records at `images_address`.
    pub fn image_count(&self) -> u64 {
        self.inner.images().len() as u64
    }

    /// Size in bytes of one image record; see `part_stride`.
    pub fn image_stride(&self) -> u64 {
        size_of::<workingtitle_core::Image>() as u64
    }

    pub fn images_address(&self) -> u64 {
        self.inner.images().as_ptr() as u64
    }

    /// Number of bytes (UTF-8) at `file_ids_address`.
    pub fn file_id_byte_count(&self) -> u64 {
        self.inner.file_ids().len() as u64
    }

    pub fn file_ids_address(&self) -> u64 {
        self.inner.file_ids().as_ptr() as u64
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_addresses_point_at_the_cores_buffers_without_copying() {
        let editor = Editor::new();
        editor
            .load(
                br#"{"type":"excalidraw","elements":[
                    {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40}]}"#
                    .to_vec(),
            )
            .unwrap();
        let geometry = editor.geometry();

        assert_eq!(geometry.part_count(), 1);
        assert_eq!(geometry.generation(), editor.geometry().generation());
        assert_eq!(geometry.part_stride(), 48);
        // Four edges, each drawn twice as a move and a cubic.
        assert_eq!(geometry.verb_count(), 16);
        assert_eq!(geometry.coord_count(), 8 * 2 + 8 * 6);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let parts = geometry.parts_address() as *const workingtitle_core::Part;
            assert_eq!((*parts).width, 2.0);
            let verbs = geometry.verbs_address() as *const u8;
            assert_eq!(*verbs, workingtitle_core::Verb::Move as u8);
            let coords = geometry.coords_address() as *const f32;
            assert_eq!(*coords, 10.0);
        }
    }

    /// Half a font size per character.
    struct Monospace;

    impl TextMeasurer for Monospace {
        fn line_width(&self, text: String, font_size: f64) -> f64 {
            text.chars().count() as f64 * font_size / 2.0
        }
    }

    #[test]
    fn text_lines_are_exposed_as_buffers_and_labels_wrap_with_the_clients_measurer() {
        let editor = Editor::new();
        editor.set_text_measurer(Arc::new(Monospace));
        editor
            .load(
                br#"{"type":"excalidraw","elements":[
                {"id":"b","type":"rectangle","x":0,"y":0,"width":100,"height":100,
                 "boundElements":[{"type":"text","id":"t"}]},
                {"id":"t","type":"text","containerId":"b","fontSize":20,"text":"ab cd ef",
                 "originalText":"aaaa bbbb cccc","width":10,"height":25}]}"#
                    .to_vec(),
            )
            .unwrap();
        let geometry = editor.geometry();

        assert_eq!(geometry.text_line_count(), 2);
        assert_eq!(geometry.text_line_stride(), 52);
        assert_eq!(geometry.text_byte_count(), "aaaa bbbbcccc".len() as u64);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let lines = geometry.text_lines_address() as *const workingtitle_core::TextLine;
            let second = *lines.add(1);
            let text = std::slice::from_raw_parts(
                (geometry.text_address() as *const u8).add(second.text_start as usize),
                second.text_len as usize,
            );
            assert_eq!(text, b"cccc");
            assert_eq!(second.font_size, 20.0);
        }
    }

    #[test]
    fn images_and_their_files_are_exposed_without_the_core_decoding_anything() {
        let editor = Editor::new();
        editor
            .load(
                br#"{"type":"excalidraw","elements":[
                {"id":"i","type":"image","x":1,"y":2,"width":30,"height":40,"fileId":"f1"}],
                "files":{"f1":{"dataURL":"data:image/png;base64,aGVsbG8="}}}"#
                    .to_vec(),
            )
            .unwrap();
        let geometry = editor.geometry();

        assert_eq!(geometry.image_count(), 1);
        assert_eq!(geometry.image_stride(), 76);
        assert_eq!(geometry.file_id_byte_count(), 2);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let image = *(geometry.images_address() as *const workingtitle_core::Image);
            assert_eq!((image.x, image.width), (1.0, 30.0));
            let id = std::slice::from_raw_parts(
                (geometry.file_ids_address() as *const u8).add(image.file_id_start as usize),
                image.file_id_len as usize,
            );
            assert_eq!(id, b"f1");
        }
        assert_eq!(editor.file_bytes("f1".into()), Some(b"hello".to_vec()));
        assert_eq!(editor.file_bytes("other".into()), None);
    }

    #[test]
    fn input_events_move_the_viewport_reported_back() {
        let editor = Editor::new();
        editor.scroll(ScrollEvent {
            x: 0.0,
            y: 0.0,
            dx: 10.0,
            dy: 20.0,
            modifiers: Modifiers::default(),
        });

        let viewport = editor.viewport();
        assert_eq!((viewport.origin_x, viewport.origin_y), (-10.0, -20.0));
        assert_eq!(viewport.zoom, 1.0);
    }

    #[test]
    fn zoom_commands_and_the_hand_tool_work_through_the_bindings() {
        let editor = Editor::new();
        editor.set_view_size(400.0, 200.0);
        editor.zoom_in();
        assert!((editor.viewport().zoom - 1.1).abs() < 1e-9);
        editor.reset_zoom();
        assert_eq!(editor.viewport().zoom, 1.0);

        editor.set_tool(Tool::Hand);
        assert_eq!(editor.tool(), Tool::Hand);
        let touch = |phase, x| PointerEvent {
            id: 1,
            phase,
            x,
            y: 0.0,
            kind: PointerKind::Finger,
            pressure: 0.5,
            timestamp: 0.0,
            modifiers: Modifiers::default(),
        };
        editor.pointer(touch(PointerPhase::Down, 10.0));
        editor.pointer(touch(PointerPhase::Move, 30.0));
        assert_eq!(editor.viewport().origin_x, -20.0);
    }

    #[test]
    fn load_errors_carry_the_cores_message() {
        let error = Editor::new().load(b"[]".to_vec()).unwrap_err();

        assert!(error.to_string().contains("not an Excalidraw Drawing"));
    }

    #[test]
    fn loaded_bytes_are_saved_back() {
        let editor = Editor::new();
        let bytes = Editor::new().save();
        editor.load(bytes.clone()).unwrap();

        assert_eq!(editor.save(), bytes);
    }
}
