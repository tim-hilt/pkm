//! The Scene: the Elements and files of the open Drawing, read from and
//! written to `.excalidraw` bytes (ADR-0003, ADR-0009).
//!
//! A [`Scene`] keeps the whole document as the JSON it came from. Elements are
//! read through typed views ([`Element`]); everything the core does not
//! interpret (unknown Element types and properties, the `files` map, other
//! top-level members) is simply never touched, so it is written back
//! verbatim and in its original order. The only changes made on load are
//! the repairs Excalidraw makes too (see [`crate::restore`]): defaults for
//! missing fields, ids for Elements without one of their own, and deleting
//! lines too large to draw. Files written by excalidraw.com need none of
//! them, so they round-trip byte for byte.
//!
//! Saving always uses excalidraw.com's own layout: `JSON.stringify(data, null,
//! 2)` without a trailing newline.

use std::fmt;
use std::str::Utf8Error;

use crate::base64;
use crate::geometry::{ImageStatus, TextAlign};
use crate::json::{self, Object, ParseError, Value};
use crate::linear;
use crate::random::Entropy;
use crate::restore;

/// The kinds of Element the core understands. Elements of any other type
/// (frames, embeds, types from newer Excalidraw versions) stay in the Scene
/// as opaque JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
    Text,
    Image,
}

impl ElementType {
    /// The type named by an Element's `type` property, if the core knows it.
    pub fn from_name(name: &str) -> Option<ElementType> {
        Some(match name {
            "rectangle" => ElementType::Rectangle,
            "diamond" => ElementType::Diamond,
            "ellipse" => ElementType::Ellipse,
            "line" => ElementType::Line,
            "arrow" => ElementType::Arrow,
            "freedraw" => ElementType::Freedraw,
            "text" => ElementType::Text,
            "image" => ElementType::Image,
            _ => return None,
        })
    }

    /// Lines and arrows: Elements defined by a list of points.
    pub fn is_linear(self) -> bool {
        matches!(self, ElementType::Line | ElementType::Arrow)
    }
}

/// How a stroke is drawn: the Element's `strokeStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

/// How the background of a closed Element is filled: its `fillStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillPattern {
    Hachure,
    CrossHatch,
    Zigzag,
    Solid,
}

/// What is drawn at an end of an arrow: its `startArrowhead` or
/// `endArrowhead`. An arrow without one has `None` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrowhead {
    Arrow,
    Bar,
    Circle,
    CircleOutline,
    Triangle,
    TriangleOutline,
    Diamond,
    DiamondOutline,
    CardinalityOne,
    CardinalityMany,
    CardinalityOneOrMany,
    CardinalityExactlyOne,
    CardinalityZeroOrOne,
    CardinalityZeroOrMany,
    /// A name this version does not know; drawn as a small open arrowhead.
    Other,
}

impl Arrowhead {
    /// The arrowhead named in a file. Excalidraw's older names are
    /// translated, as its `normalizeArrowhead` does.
    fn from_name(name: &str) -> Arrowhead {
        match name {
            "arrow" => Arrowhead::Arrow,
            "bar" => Arrowhead::Bar,
            "circle" | "dot" => Arrowhead::Circle,
            "circle_outline" => Arrowhead::CircleOutline,
            "triangle" => Arrowhead::Triangle,
            "triangle_outline" => Arrowhead::TriangleOutline,
            "diamond" => Arrowhead::Diamond,
            "diamond_outline" => Arrowhead::DiamondOutline,
            "cardinality_one" | "crowfoot_one" => Arrowhead::CardinalityOne,
            "cardinality_many" | "crowfoot_many" => Arrowhead::CardinalityMany,
            "cardinality_one_or_many" | "crowfoot_one_or_many" => Arrowhead::CardinalityOneOrMany,
            "cardinality_exactly_one" => Arrowhead::CardinalityExactlyOne,
            "cardinality_zero_or_one" => Arrowhead::CardinalityZeroOrOne,
            "cardinality_zero_or_many" => Arrowhead::CardinalityZeroOrMany,
            _ => Arrowhead::Other,
        }
    }
}

/// Where a text sits vertically in the box of its container: its
/// `verticalAlign`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

/// Rounded corners of an Element: its `roundness` property. Elements with
/// sharp corners have none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Roundness {
    /// Type 1, from old files; behaves like [`Roundness::Proportional`].
    Legacy,
    /// Type 2: a quarter of the side (diamonds and linear Elements).
    Proportional,
    /// Type 3: a fixed radius (32 by default) that shrinks on small shapes
    /// (rectangles).
    Adaptive { radius: Option<f64> },
    /// A type this version does not know; its corners are not rounded, but
    /// the shape is still drawn the rounded way.
    Other,
}

/// Why a file could not be loaded as a Drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The bytes are not UTF-8 text.
    NotText(Utf8Error),
    /// The text is not valid JSON.
    InvalidJson(ParseError),
    /// Valid JSON, but not an Excalidraw Drawing; says what is wrong.
    NotADrawing(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::NotText(e) => write!(f, "the file is not UTF-8 text: {e}"),
            LoadError::InvalidJson(e) => e.fmt(f),
            LoadError::NotADrawing(why) => write!(f, "not an Excalidraw Drawing: {why}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Some members of an Element, by key: `None` for one it does not have.
pub(crate) type Members = Vec<(&'static str, Option<Value>)>;

/// The open Drawing's content.
#[derive(Debug, Clone)]
pub struct Scene {
    /// The document: always an object with an `elements` array of objects.
    root: Object,
    /// Which handle stands for which Element of `root`.
    handles: Handles,
}

/// Scenes are equal if their documents are; handles are bookkeeping.
impl PartialEq for Scene {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

/// Gives every Element a handle that it keeps for as long as the Scene is
/// open, whatever happens to the Elements around it, so that clients can
/// cache what they derive from an Element by `(handle, version)`.
///
/// Handles are numbered from 0 in file order on load and are never handed
/// out twice. An Element that is removed and comes back with the same id
/// (undo of a cancelled drawing, a paste of it) gets its old handle again.
#[derive(Debug, Clone, Default)]
struct Handles {
    /// The handle of the Element at each position of the file.
    by_position: Vec<u32>,
    /// The position of each handle, `ABSENT` if its Element was removed.
    position: Vec<u32>,
    /// The handle of each Element ever in the Scene, by id (of the first, if
    /// a file repeats an id); kept for removed Elements so that they get it
    /// back, and to find an Element by id without searching the file.
    by_id: std::collections::HashMap<String, u32>,
}

impl Handles {
    const ABSENT: u32 = u32::MAX;

    /// For a file of Elements with these ids.
    fn new<'a>(ids: impl ExactSizeIterator<Item = &'a str>) -> Self {
        let count = u32::try_from(ids.len()).unwrap_or(Self::ABSENT - 1);
        let mut by_id = std::collections::HashMap::with_capacity(ids.len());
        for (handle, id) in (0..count).zip(ids) {
            by_id.entry(id.to_owned()).or_insert(handle);
        }
        Self {
            by_position: (0..count).collect(),
            position: (0..count).collect(),
            by_id,
        }
    }

    /// The handles for a file of Elements with these ids: those that have
    /// one keep it, the others get new ones, and those gone are removed.
    fn carried_over(&self, ids: &[&str]) -> Self {
        let mut next = Self {
            by_position: Vec::with_capacity(ids.len()),
            position: vec![Self::ABSENT; self.position.len()],
            by_id: self.by_id.clone(),
        };
        for (at, &id) in ids.iter().enumerate() {
            let known = next
                .by_id
                .get(id)
                .copied()
                .filter(|&handle| next.position[handle as usize] == Self::ABSENT);
            let handle = known.unwrap_or_else(|| {
                next.position.push(Self::ABSENT);
                let handle = (next.position.len() - 1) as u32;
                next.by_id.entry(id.to_owned()).or_insert(handle);
                handle
            });
            next.position[handle as usize] = at as u32;
            next.by_position.push(handle);
        }
        next
    }

    /// The position in the file of the Element with `id`.
    fn position_of_id(&self, id: &str) -> Option<usize> {
        self.position_of(*self.by_id.get(id)?)
    }

    fn position_of(&self, handle: u32) -> Option<usize> {
        match *self.position.get(handle as usize)? {
            Self::ABSENT => None,
            at => Some(at as usize),
        }
    }

    fn handle_at(&self, position: usize) -> Option<u32> {
        self.by_position.get(position).copied()
    }

    /// Records an Element with `id` inserted at `position`; returns its
    /// handle.
    fn insert(&mut self, position: usize, id: &str) -> u32 {
        let old = self
            .by_id
            .get(id)
            .copied()
            .filter(|&handle| self.position[handle as usize] == Self::ABSENT);
        let handle = old.unwrap_or_else(|| {
            self.position.push(Self::ABSENT);
            let handle = (self.position.len() - 1) as u32;
            self.by_id.entry(id.to_owned()).or_insert(handle);
            handle
        });
        self.by_position.insert(position, handle);
        if position + 1 == self.by_position.len() {
            self.position[handle as usize] = position as u32;
        } else {
            self.reindex();
        }
        handle
    }

    /// Records that the Element with `id` at `position` is gone.
    fn remove(&mut self, position: usize) {
        let handle = self.by_position.remove(position);
        self.position[handle as usize] = Self::ABSENT;
        self.reindex();
    }

    /// Records that the Element at `order[place]` moved to `place`.
    fn permute(&mut self, order: &[usize]) {
        self.by_position = order.iter().map(|&from| self.by_position[from]).collect();
        self.reindex();
    }

    fn reindex(&mut self) {
        for (at, &handle) in self.by_position.iter().enumerate() {
            self.position[handle as usize] = at as u32;
        }
    }
}

impl Scene {
    /// A Scene without Elements, with the members excalidraw.com exports.
    pub fn empty() -> Scene {
        let mut app_state = Object::new();
        app_state.insert("gridSize".into(), Value::Null);
        app_state.insert(
            "viewBackgroundColor".into(),
            Value::String("#ffffff".into()),
        );
        let mut root = Object::new();
        root.insert("type".into(), Value::String("excalidraw".into()));
        root.insert("version".into(), Value::Number(2.0));
        root.insert("source".into(), Value::String("pkm".into()));
        root.insert("elements".into(), Value::Array(Vec::new()));
        root.insert("appState".into(), Value::Object(app_state));
        root.insert("files".into(), Value::Object(Object::new()));
        Scene {
            root,
            handles: Handles::new(std::iter::empty()),
        }
    }

    /// Reads the bytes of a `.excalidraw` file, repairing its Elements as
    /// Excalidraw does (see [`restore::restore_elements`]); `entropy` makes
    /// the ids of Elements that need new ones.
    pub(crate) fn from_bytes(bytes: &[u8], entropy: &mut Entropy) -> Result<Scene, LoadError> {
        let text = std::str::from_utf8(bytes).map_err(LoadError::NotText)?;
        let Value::Object(mut root) = json::parse(text).map_err(LoadError::InvalidJson)? else {
            return Err(not_a_drawing("the document is not an object"));
        };
        if root.get("type").and_then(Value::as_str) != Some("excalidraw") {
            return Err(not_a_drawing("`type` is not \"excalidraw\""));
        }
        if root.get("elements").is_none() {
            root.insert("elements".into(), Value::Array(Vec::new()));
        }
        let Some(elements) = root.get_mut("elements").and_then(Value::as_array_mut) else {
            return Err(not_a_drawing("`elements` is not an array"));
        };
        if let Some(index) = elements.iter().position(|e| e.as_object().is_none()) {
            return Err(not_a_drawing(&format!("element {index} is not an object")));
        }
        restore::restore_elements(elements, entropy);
        let handles = Handles::new(elements.iter().map(|e| e.as_object().map_or("", id_of)));
        Ok(Scene { root, handles })
    }

    /// Replaces the document with the Drawing in `bytes`, keeping the handle
    /// of every Element that is still there (and giving new ones new
    /// handles), so that what clients and gestures hold by handle stays
    /// valid. If the bytes are not a Drawing the Scene is kept. Returns
    /// whether the document changed.
    pub(crate) fn replace_keeping_handles(
        &mut self,
        bytes: &[u8],
        entropy: &mut Entropy,
    ) -> Result<bool, LoadError> {
        let loaded = Scene::from_bytes(bytes, entropy)?;
        if loaded == *self {
            return Ok(false);
        }
        let ids: Vec<&str> = loaded.raw_elements().map(id_of).collect();
        self.handles = self.handles.carried_over(&ids);
        self.root = loaded.root;
        Ok(true)
    }

    /// The `.excalidraw` file content, in excalidraw.com's layout.
    pub fn to_json(&self) -> String {
        self.root.to_json_pretty()
    }

    /// The Element with `handle`, if there is one (see
    /// [`Element::handle`]).
    pub(crate) fn element(&self, handle: u32) -> Option<Element<'_>> {
        let value = self
            .element_values()
            .get(self.handles.position_of(handle)?)?;
        Element::view(handle, value.as_object()?)
    }

    /// The Element with `id`, if there is one, deleted or not.
    pub(crate) fn element_by_id(&self, id: &str) -> Option<Element<'_>> {
        self.element(self.handles.handle_at(self.index_of(id)?)?)
    }

    /// The Elements of a type the core understands, deleted ones included, in
    /// z-order (file order).
    pub fn elements(&self) -> impl DoubleEndedIterator<Item = Element<'_>> {
        self.element_values()
            .iter()
            .enumerate()
            .filter_map(|(position, value)| {
                let object = value.as_object()?;
                Some(Element {
                    handle: self.handles.handle_at(position)?,
                    kind: type_of(object)?,
                    object,
                })
            })
    }

    /// Every Element of the file, whatever its type, deleted ones included,
    /// in z-order. An Element's position here is not its handle; see
    /// [`Scene::handle_at`].
    pub(crate) fn raw_elements(&self) -> impl Iterator<Item = &Object> {
        self.element_values().iter().filter_map(Value::as_object)
    }

    /// The handle of the Element at `position` in the file (see
    /// [`Scene::raw_elements`]).
    pub(crate) fn handle_at(&self, position: usize) -> Option<u32> {
        self.handles.handle_at(position)
    }

    /// The position in the file of the Element with `handle`.
    pub(crate) fn position_of(&self, handle: u32) -> Option<usize> {
        self.handles.position_of(handle)
    }

    /// The id of the Element with `handle`.
    pub(crate) fn element_id(&self, handle: u32) -> Option<&str> {
        self.element_values()
            .get(self.handles.position_of(handle)?)?
            .as_object()?
            .get("id")?
            .as_str()
    }

    /// The ids of all Elements in z-order; an Element without one is `""`.
    pub(crate) fn order(&self) -> Vec<String> {
        self.raw_elements()
            .map(|e| {
                e.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    }

    /// Reorders the Elements: the one that is at `order[place]` now goes to
    /// `place`. `order` has to be a permutation of the positions.
    pub(crate) fn permute(&mut self, order: &[usize]) {
        let elements = self.element_values_mut();
        debug_assert_eq!(order.len(), elements.len());
        let mut old: Vec<Option<Value>> = std::mem::take(elements).into_iter().map(Some).collect();
        *elements = order
            .iter()
            .filter_map(|&from| old.get_mut(from)?.take())
            .collect();
        self.handles.permute(order);
    }

    /// Puts the Elements with `ids` into the places that they take now, in
    /// the order of `ids`, leaving the others where they are. Ids that are
    /// not in the Scene are skipped.
    pub(crate) fn arrange(&mut self, ids: &[String]) {
        let positions: Vec<usize> = ids.iter().filter_map(|id| self.index_of(id)).collect();
        let mut places = positions.clone();
        places.sort_unstable();
        let mut order: Vec<usize> = (0..self.element_values().len()).collect();
        for (&place, &at) in places.iter().zip(&positions) {
            order[place] = at;
        }
        self.permute(&order);
    }

    /// The colour of the Canvas as written in the file (a CSS colour), white
    /// if the file has none.
    pub fn view_background_color(&self) -> &str {
        self.root
            .get("appState")
            .and_then(Value::as_object)
            .and_then(|app_state| app_state.get("viewBackgroundColor"))
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
    }

    /// The bytes of the file `id` of the Drawing's `files`, exactly as stored:
    /// the payload of its `dataURL`, base64-decoded. The core never looks at
    /// the bytes (they are an image for the client to decode, ADR-0003).
    /// `None` if there is no such file or its `dataURL` is not a base64 data
    /// URL.
    pub fn file_bytes(&self, id: &str) -> Option<Vec<u8>> {
        let data_url = self
            .root
            .get("files")?
            .as_object()?
            .get(id)?
            .as_object()?
            .get("dataURL")?
            .as_str()?;
        let (header, payload) = data_url.split_once(',')?;
        header
            .strip_prefix("data:")?
            .ends_with(";base64")
            .then(|| base64::decode(payload))?
    }

    /// The record of the file `id` of the Drawing's `files`, as stored.
    pub(crate) fn file_record(&self, id: &str) -> Option<&Value> {
        self.root.get("files")?.as_object()?.get(id)
    }

    /// Stores `bytes` as the file `id` of the Drawing's `files`, as
    /// Excalidraw does: a record with the MIME type and a base64 data URL.
    /// A file that is there already is kept: an id stands for its content.
    pub(crate) fn add_file(&mut self, id: &str, mime_type: &str, bytes: &[u8], now: f64) {
        let text = |s: &str| Value::String(s.to_owned());
        let mut file = Object::new();
        for (key, value) in [
            ("mimeType", text(mime_type)),
            ("id", text(id)),
            (
                "dataURL",
                Value::String(format!("data:{mime_type};base64,{}", base64::encode(bytes))),
            ),
            ("created", Value::Number(now)),
            ("lastRetrieved", Value::Number(now)),
        ] {
            file.insert(key.into(), value);
        }
        self.add_file_record(id, Value::Object(file));
    }

    /// Stores `record`, a file as another Drawing has it, as the file `id`
    /// of the Drawing's `files`, unless there is one already.
    pub(crate) fn add_file_record(&mut self, id: &str, record: Value) {
        if !matches!(self.root.get("files"), Some(Value::Object(_))) {
            self.root
                .insert("files".into(), Value::Object(Object::new()));
        }
        if let Some(Value::Object(files)) = self.root.get_mut("files")
            && files.get(id).is_none()
        {
            files.insert(id.to_owned(), record);
        }
    }

    /// Adds `element`, an object with all the members of an Element, on top
    /// of the others. Returns its handle.
    pub(crate) fn push(&mut self, element: Object) -> u32 {
        let position = self.element_values().len();
        self.insert_at(position, element)
    }

    fn insert_at(&mut self, position: usize, element: Object) -> u32 {
        let handle = self.handles.insert(position, id_of(&element));
        self.element_values_mut()
            .insert(position, Value::Object(element));
        handle
    }

    /// Puts `element`, an object with all the members of an Element, right
    /// above the Element `anchor` in z-order (on top if there is no such
    /// Element). Returns its handle.
    pub(crate) fn insert_after(&mut self, anchor: &str, element: Object) -> u32 {
        let Some(index) = self.index_of(anchor) else {
            return self.push(element);
        };
        self.insert_at(index + 1, element)
    }

    /// Removes the Element with `handle`. The handles of the others stay.
    pub(crate) fn remove(&mut self, handle: u32) {
        let Some(position) = self.handles.position_of(handle) else {
            return;
        };
        self.element_values_mut().remove(position);
        self.handles.remove(position);
    }

    /// The Element with `id` as it is now, whatever its type, deleted or
    /// not; `None` if the Scene has no such Element.
    pub(crate) fn snapshot(&self, id: &str) -> Option<Object> {
        let index = self.index_of(id)?;
        self.element_values()[index].as_object().cloned()
    }

    /// The members `keys` of the Element with `id` as they are now, `None`
    /// for those it does not have; `None` if the Scene has no such Element.
    pub(crate) fn members(&self, id: &str, keys: &[&'static str]) -> Option<Members> {
        let element = self.element_values()[self.index_of(id)?].as_object()?;
        Some(keys.iter().map(|&k| (k, element.get(k).cloned())).collect())
    }

    /// Sets the `members` of the Element with `id` as they are, and makes it
    /// a new version of itself, see [`Scene::update`].
    pub(crate) fn restore_members(&mut self, id: &str, members: &Members, nonce: f64) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let Some(element) = self.element_values_mut()[index].as_object_mut() else {
            return;
        };
        let version = element
            .get("version")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        for (key, value) in members {
            match value {
                Some(value) => element.insert((*key).into(), value.clone()),
                None => element.remove(key),
            }
        }
        element.insert("version".into(), Value::Number(version + 1.0));
        element.insert("versionNonce".into(), Value::Number(nonce));
    }

    /// Makes the Element `element` the one of its `id`, in the place of the
    /// current one (or on top if there is none), as a new version of it, see
    /// [`Scene::revise`].
    pub(crate) fn put(&mut self, mut element: Object, nonce: f64) {
        let Some(id) = element.get("id").and_then(Value::as_str).map(str::to_owned) else {
            return;
        };
        let Some(index) = self.index_of(&id) else {
            self.push(element);
            return;
        };
        let current = &mut self.element_values_mut()[index];
        let version = current
            .as_object()
            .and_then(|o| o.get("version"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        element.insert("version".into(), Value::Number(version + 1.0));
        element.insert("versionNonce".into(), Value::Number(nonce));
        *current = Value::Object(element);
    }

    /// Marks the Element with `id` deleted, as a new version of itself; it
    /// stays in the Scene, and in the file, as on excalidraw.com.
    pub(crate) fn mark_deleted(&mut self, id: &str, nonce: f64) {
        if let Some(mut element) = self.snapshot(id) {
            element.insert("isDeleted".into(), Value::Bool(true));
            self.put(element, nonce);
        }
    }

    /// The position in the file of the Element with `id`.
    fn index_of(&self, id: &str) -> Option<usize> {
        self.handles.position_of_id(id)
    }

    /// Moves and resizes the Element with `handle`, and makes it a new
    /// version of itself, see [`Scene::revise`].
    pub(crate) fn set_frame(
        &mut self,
        handle: u32,
        (x, y, width, height): (f64, f64, f64, f64),
        nonce: f64,
    ) {
        self.revise(
            handle,
            nonce,
            [("x", x), ("y", y), ("width", width), ("height", height)],
        );
    }

    /// Moves the Element with `handle` to `(x, y)`, keeping its size, and
    /// makes it a new version of itself, see [`Scene::revise`]. Lines and
    /// freedraw strokes have points relative to their position and move with
    /// it.
    pub(crate) fn set_position(&mut self, handle: u32, (x, y): (f64, f64), nonce: f64) {
        self.revise(handle, nonce, [("x", x), ("y", y)]);
    }

    /// Gives the line or arrow with `handle` the `points`, which are relative
    /// to its position `(x, y)`, and makes it a new version of itself, see
    /// [`Scene::revise`]. Its width and height become the extent of the
    /// points, as in Excalidraw (`getSizeFromPoints`).
    pub(crate) fn set_points(
        &mut self,
        handle: u32,
        (x, y): (f64, f64),
        points: &[(f64, f64)],
        nonce: f64,
    ) {
        let (width, height) = linear::extent(points);
        let points = points
            .iter()
            .map(|&(px, py)| Value::Array(vec![Value::Number(px), Value::Number(py)]))
            .collect();
        self.update(
            handle,
            nonce,
            [
                ("x", Value::Number(x)),
                ("y", Value::Number(y)),
                ("width", Value::Number(width)),
                ("height", Value::Number(height)),
                ("points", Value::Array(points)),
            ],
        );
    }

    /// Adds `point`, relative to the position of the freedraw stroke with
    /// `handle`, to its points, and `pressure` to its pressures if it has
    /// one, and makes the stroke a new version of itself, see
    /// [`Scene::revise`]. Its width and height become `size`, the extent of
    /// all its points. Unlike [`Scene::set_points`] this does not touch the
    /// points that are there, so that a stroke of any length grows in
    /// constant time.
    pub(crate) fn push_stroke_point(
        &mut self,
        handle: u32,
        (px, py): (f64, f64),
        pressure: Option<f64>,
        (width, height): (f64, f64),
        nonce: f64,
    ) {
        let Some(stroke) = self.element_object_mut(handle) else {
            return;
        };
        if let Some(points) = stroke.get_mut("points").and_then(Value::as_array_mut) {
            points.push(Value::Array(vec![Value::Number(px), Value::Number(py)]));
        }
        if let Some(pressure) = pressure
            && let Some(pressures) = stroke.get_mut("pressures").and_then(Value::as_array_mut)
        {
            pressures.push(Value::Number(pressure));
        }
        self.revise(handle, nonce, [("width", width), ("height", height)]);
    }

    /// Sets the numeric `members` of the Element with `handle` and makes it
    /// a new version of itself, see [`Scene::update`].
    fn revise<const N: usize>(
        &mut self,
        handle: u32,
        nonce: f64,
        members: [(&'static str, f64); N],
    ) {
        self.update(
            handle,
            nonce,
            members.map(|(key, value)| (key, Value::Number(value))),
        );
    }

    /// Sets `members` of the Element with `handle` and makes it a new
    /// version of itself as Excalidraw does on every change: `version` goes
    /// up and `versionNonce` becomes `nonce`.
    pub(crate) fn update(
        &mut self,
        handle: u32,
        nonce: f64,
        members: impl IntoIterator<Item = (&'static str, Value)>,
    ) {
        let Some(element) = self.element_object_mut(handle) else {
            return;
        };
        let version = element
            .get("version")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        for (key, value) in members.into_iter().chain([
            ("version", Value::Number(version + 1.0)),
            ("versionNonce", Value::Number(nonce)),
        ]) {
            element.insert(key.into(), value);
        }
    }

    fn element_object_mut(&mut self, handle: u32) -> Option<&mut Object> {
        let position = self.handles.position_of(handle)?;
        self.element_values_mut()
            .get_mut(position)
            .and_then(Value::as_object_mut)
    }

    fn element_values_mut(&mut self) -> &mut Vec<Value> {
        // `from_bytes` and `empty` make sure there is an array of Elements,
        // and nothing replaces it by anything else.
        self.root
            .get_mut("elements")
            .and_then(Value::as_array_mut)
            .expect("a Scene's `elements` is an array")
    }

    fn element_values(&self) -> &[Value] {
        self.root
            .get("elements")
            .and_then(Value::as_array)
            .unwrap_or_default()
    }
}

fn not_a_drawing(why: &str) -> LoadError {
    LoadError::NotADrawing(why.to_owned())
}

/// A point of a path: two finite numbers.
fn finite_pair(point: &Value) -> Option<(f64, f64)> {
    let [x, y] = point.as_array()? else {
        return None;
    };
    let (x, y) = (x.as_f64()?, y.as_f64()?);
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

fn id_of(element: &Object) -> &str {
    element
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn type_of(element: &Object) -> Option<ElementType> {
    ElementType::from_name(element.get("type")?.as_str()?)
}

/// Read-only view of one Element of a known type.
#[derive(Debug, Clone, Copy)]
pub struct Element<'a> {
    handle: u32,
    kind: ElementType,
    object: &'a Object,
}

impl<'a> Element<'a> {
    /// The Element's `id`, empty if the file has none.
    pub fn id(&self) -> &'a str {
        let id: Option<&'a str> = self.object.get("id").and_then(Value::as_str);
        id.unwrap_or_default()
    }

    /// The member `key` as the file has it, for what the core reads itself
    /// instead of through a typed accessor.
    pub(crate) fn member(&self, key: &str) -> Option<&'a Value> {
        self.object.get(key)
    }

    /// The ids of the groups the Element is in, innermost first, as
    /// Excalidraw lists them.
    pub fn group_ids(&self) -> impl Iterator<Item = &'a str> + use<'a> {
        self.object
            .get("groupIds")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
    }
}

impl<'a> Element<'a> {
    /// A view of `object`, which may be a snapshot taken earlier (see
    /// [`Scene::snapshot`]), as the Element with `handle`.
    pub(crate) fn view(handle: u32, object: &'a Object) -> Option<Self> {
        Some(Self {
            handle,
            kind: type_of(object)?,
            object,
        })
    }

    /// The `id` of the Element a text is bound to (a label), if any.
    pub fn container_id(&self) -> Option<&'a str> {
        self.object.get("containerId").and_then(Value::as_str)
    }
}

impl Element<'_> {
    /// Identifies the Element within the Scene for as long as the Scene is
    /// open, whatever is added, removed or reordered: not its position in
    /// the file. An Element that is removed and comes back with the same id
    /// has its old handle again.
    pub fn handle(&self) -> u32 {
        self.handle
    }

    pub fn kind(&self) -> ElementType {
        self.kind
    }

    pub fn is_deleted(&self) -> bool {
        self.object
            .get("isDeleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Whether the Element is locked against selection and editing.
    pub fn is_locked(&self) -> bool {
        self.object
            .get("locked")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// The id of the outermost group the Element is in, if any.
    pub fn outermost_group(&self) -> Option<&str> {
        self.group_ids().last()
    }

    /// Whether the Element is in the group `group`, however deeply nested.
    pub fn is_in_group(&self, group: &str) -> bool {
        self.group_ids().any(|id| id == group)
    }

    /// Whether a text is bound to the Element as its label.
    pub fn has_bound_text(&self) -> bool {
        self.object
            .get("boundElements")
            .and_then(Value::as_array)
            .is_some_and(|bound| {
                bound
                    .iter()
                    .any(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            })
    }

    pub fn x(&self) -> f64 {
        self.number("x", 0.0)
    }

    pub fn y(&self) -> f64 {
        self.number("y", 0.0)
    }

    pub fn width(&self) -> f64 {
        self.number("width", 0.0)
    }

    pub fn height(&self) -> f64 {
        self.number("height", 0.0)
    }

    /// Rotation about the Element's centre in radians, clockwise.
    pub fn angle(&self) -> f64 {
        self.number("angle", 0.0)
    }

    /// The seed of the Element's hand-drawn jitter; any number, as in the file.
    pub fn seed(&self) -> f64 {
        self.number("seed", 1.0)
    }

    /// The stroke colour as written in the file (a CSS colour, usually hex).
    pub fn stroke_color(&self) -> &str {
        self.object
            .get("strokeColor")
            .and_then(Value::as_str)
            .unwrap_or("#1e1e1e")
    }

    pub fn stroke_width(&self) -> f64 {
        self.number("strokeWidth", 2.0)
    }

    pub fn line_style(&self) -> LineStyle {
        match self.object.get("strokeStyle").and_then(Value::as_str) {
            Some("dashed") => LineStyle::Dashed,
            Some("dotted") => LineStyle::Dotted,
            _ => LineStyle::Solid,
        }
    }

    /// The background colour as written in the file (a CSS colour, usually
    /// hex), `transparent` if the Element has none.
    pub fn background_color(&self) -> &str {
        self.object
            .get("backgroundColor")
            .and_then(Value::as_str)
            .unwrap_or("transparent")
    }

    /// How the background is filled. Like rough.js, anything unknown is
    /// hachure.
    pub fn fill_pattern(&self) -> FillPattern {
        match self.object.get("fillStyle").and_then(Value::as_str) {
            Some("solid") => FillPattern::Solid,
            Some("cross-hatch") => FillPattern::CrossHatch,
            Some("zigzag") => FillPattern::Zigzag,
            _ => FillPattern::Hachure,
        }
    }

    /// How sketchy the Element is drawn: 0 (architect), 1 (artist) or 2
    /// (cartoonist).
    pub fn roughness(&self) -> f64 {
        self.number("roughness", 1.0)
    }

    /// The Element's rounded corners, if it has any.
    pub fn roundness(&self) -> Option<Roundness> {
        let roundness = self.object.get("roundness")?.as_object()?;
        Some(match roundness.get("type").and_then(Value::as_f64) {
            Some(1.0) => Roundness::Legacy,
            Some(2.0) => Roundness::Proportional,
            Some(3.0) => Roundness::Adaptive {
                radius: roundness.get("value").and_then(Value::as_f64),
            },
            _ => Roundness::Other,
        })
    }

    /// The points of a line, arrow or freedraw stroke in the Element's own
    /// frame, which starts at its `x` and `y`. Points that are not number
    /// pairs are left out; a line or arrow without two valid points goes from
    /// the origin to `(width, height)`, as Excalidraw restores it. Empty for
    /// other types.
    pub fn points(&self) -> Vec<(f64, f64)> {
        let valid = self.valid_points();
        match self.kind {
            ElementType::Freedraw => valid,
            kind if kind.is_linear() && valid.len() < 2 => {
                vec![(0.0, 0.0), (self.width(), self.height())]
            }
            kind if kind.is_linear() => valid,
            _ => Vec::new(),
        }
    }

    fn valid_points(&self) -> Vec<(f64, f64)> {
        self.raw_points().iter().filter_map(finite_pair).collect()
    }

    fn raw_points(&self) -> &[Value] {
        self.object
            .get("points")
            .and_then(Value::as_array)
            .unwrap_or_default()
    }

    /// The pressure of each point of a freedraw stroke, in the order of
    /// [`Element::points`], as Excalidraw restores them: a point the file
    /// gives no pressure for has none here (the list is then shorter than
    /// the points), and a pressure that is not a number is `0.5`.
    pub(crate) fn pressures(&self) -> Vec<f64> {
        let given = self
            .object
            .get("pressures")
            .and_then(Value::as_array)
            .unwrap_or_default();
        self.raw_points()
            .iter()
            .enumerate()
            .filter(|(_, point)| finite_pair(point).is_some())
            .filter_map(|(index, _)| given.get(index))
            .map(|pressure| pressure.as_f64().filter(|p| p.is_finite()).unwrap_or(0.5))
            .collect()
    }

    /// Whether the pressure of a freedraw stroke is made up from the speed
    /// of the pointer instead of being measured. Excalidraw reads a file
    /// that does not say as measured.
    pub(crate) fn simulates_pressure(&self) -> bool {
        self.object
            .get("simulatePressure")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Whether a freedraw stroke is drawn with the same width all along
    /// (`strokeOptions.variability` is `"constant"`) rather than by pressure.
    pub(crate) fn has_constant_width(&self) -> bool {
        self.object
            .get("strokeOptions")
            .and_then(Value::as_object)
            .and_then(|options| options.get("variability"))
            .and_then(Value::as_str)
            == Some("constant")
    }

    /// How much a freedraw stroke smooths out the movement of the pointer,
    /// `0.0..=1.0`; Excalidraw's `0.5` if the file has no usable value.
    pub(crate) fn streamline(&self) -> f64 {
        self.object
            .get("strokeOptions")
            .and_then(Value::as_object)
            .and_then(|options| options.get("streamline"))
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite())
            .unwrap_or(0.5)
    }

    /// The id of the file an image shows, in the Drawing's `files`; `None`
    /// for an image that has none yet.
    pub fn file_id(&self) -> Option<&str> {
        self.str("fileId")
    }

    pub fn image_status(&self) -> ImageStatus {
        match self.str("status") {
            Some("saved") => ImageStatus::Saved,
            Some("error") => ImageStatus::Error,
            _ => ImageStatus::Pending,
        }
    }

    /// Whether the image is mirrored horizontally (`-1`) or vertically (`-1`),
    /// as `(x, y)` factors; `(1, 1)` if it is not or the file does not say.
    pub fn image_scale(&self) -> (f64, f64) {
        let factor = |value: Option<&Value>| match value.and_then(Value::as_f64) {
            Some(f) if f.is_finite() && f != 0.0 => f.signum(),
            _ => 1.0,
        };
        match self.object.get("scale").and_then(Value::as_array) {
            Some([x, y, ..]) => (factor(Some(x)), factor(Some(y))),
            _ => (1.0, 1.0),
        }
    }

    /// The part of the image's file that is shown, in fractions of the file's
    /// width and height, as `(left, top, right, bottom)`; `None` for an
    /// uncropped image. A crop of a file without size is no crop.
    pub fn image_crop(&self) -> Option<(f64, f64, f64, f64)> {
        let crop = self.object.get("crop")?.as_object()?;
        let number = |key: &str| {
            crop.get(key)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
        };
        let (natural_width, natural_height) = (number("naturalWidth")?, number("naturalHeight")?);
        if natural_width <= 0.0 || natural_height <= 0.0 {
            return None;
        }
        let (x, y) = (number("x")?, number("y")?);
        let (width, height) = (number("width")?, number("height")?);
        Some((
            x / natural_width,
            y / natural_height,
            (x + width) / natural_width,
            (y + height) / natural_height,
        ))
    }

    /// What an arrow has at its first point; `None` for lines and arrows
    /// without.
    pub fn start_arrowhead(&self) -> Option<Arrowhead> {
        if self.kind != ElementType::Arrow {
            return None;
        }
        self.arrowhead("startArrowhead", None)
    }

    /// What an arrow has at its last point. Arrows from old files that do not
    /// say have an arrow, as Excalidraw restores them.
    pub fn end_arrowhead(&self) -> Option<Arrowhead> {
        if self.kind != ElementType::Arrow {
            return None;
        }
        self.arrowhead("endArrowhead", Some(Arrowhead::Arrow))
    }

    fn arrowhead(&self, key: &str, missing: Option<Arrowhead>) -> Option<Arrowhead> {
        match self.object.get(key) {
            None => missing,
            Some(Value::String(name)) => Some(Arrowhead::from_name(name)),
            Some(_) => None,
        }
    }

    /// Whether the Element is an arrow that Excalidraw routes at right angles.
    /// The core keeps its points as they are in the file and draws them
    /// without routing.
    pub fn is_elbow_arrow(&self) -> bool {
        self.kind == ElementType::Arrow
            && self.object.get("elbowed").and_then(Value::as_bool) == Some(true)
    }

    /// The text of a text Element as shown: wrapped to its container (or
    /// width) as Excalidraw saved it.
    pub fn text(&self) -> &str {
        self.str("text").unwrap_or_default()
    }

    /// The text as typed, without the line breaks of wrapping.
    pub fn original_text(&self) -> &str {
        self.str("originalText").unwrap_or_else(|| self.text())
    }

    pub fn font_size(&self) -> f64 {
        self.number("fontSize", 20.0)
    }

    /// Excalidraw's font family number; 5 is Excalifont, the default.
    pub fn font_family(&self) -> u32 {
        // Saturating float-to-int conversion is exactly what is wanted.
        self.number("fontFamily", 5.0) as u32
    }

    /// The height of a line of text relative to the font size.
    pub fn line_height(&self) -> f64 {
        match self.number("lineHeight", 1.25) {
            height if height > 0.0 => height,
            _ => 1.25,
        }
    }

    pub fn text_align(&self) -> TextAlign {
        match self.str("textAlign") {
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Right,
            _ => TextAlign::Left,
        }
    }

    pub fn vertical_align(&self) -> VerticalAlign {
        match self.str("verticalAlign") {
            Some("middle") => VerticalAlign::Middle,
            Some("bottom") => VerticalAlign::Bottom,
            _ => VerticalAlign::Top,
        }
    }

    /// Whether a text's width follows its content; `false` for texts with a
    /// width of their own, which wrap to it.
    pub fn auto_resize(&self) -> bool {
        self.object
            .get("autoResize")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    /// `0.0` (invisible) to `1.0`.
    pub fn opacity(&self) -> f64 {
        (self.number("opacity", 100.0) / 100.0).clamp(0.0, 1.0)
    }

    /// Changes whenever the Element does; `0` if the file has no usable one.
    pub fn version(&self) -> u32 {
        // Saturating float-to-int conversion is exactly what is wanted.
        self.number("version", 0.0) as u32
    }

    fn str(&self, key: &str) -> Option<&str> {
        self.object.get(key).and_then(Value::as_str)
    }

    fn number(&self, key: &str, default: f64) -> f64 {
        match self.object.get(key).and_then(Value::as_f64) {
            Some(n) if n.is_finite() => n,
            _ => default,
        }
    }
}
