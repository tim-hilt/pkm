//! Copying and pasting Elements as Excalidraw's clipboard JSON
//! (`serializeAsClipboardJSON` and `addElementsFromPasteOrLibrary` in
//! `clipboard.ts` and `App.tsx` at the pinned commit), so that what is copied
//! here can be pasted into excalidraw.com and the other way round.
//!
//! The text is `{"type":"excalidraw/clipboard","elements":[…],"files":{…}}`:
//! the Elements as they are in the Scene and the files of the images among
//! them. Pasting accepts that, or a whole `.excalidraw` file, and inserts the
//! Elements as new ones (see [`crate::duplicate::Copying`]), centred on a
//! point, on top of the Scene: with new ids, new groups and new seeds,
//! referring to each other and to nothing that was not pasted.

use std::collections::HashSet;

use crate::duplicate::Copying;
use crate::frame::Frame;
use crate::history::Step;
use crate::restore;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use pkm_base::json::{Object, Value, parse};
use pkm_base::random::Entropy;

/// The `type` of what Excalidraw puts on the clipboard.
const CLIPBOARD: &str = "excalidraw/clipboard";
/// What Excalidraw also takes as Elements to paste: a `.excalidraw` file.
/// Not the clipboard of its API (`excalidraw-api/clipboard`), which holds
/// Element skeletons that would need `convertToExcalidrawElements`.
const ACCEPTED: [&str; 2] = [CLIPBOARD, "excalidraw"];

/// The text to put on the clipboard for copying `selection`: its Elements, with
/// their labels, in z-order, and the files of its images. `None` if that is
/// nothing.
pub(crate) fn copy(scene: &Scene, selection: &Selection) -> Option<String> {
    let copied: Vec<Element> = selection.covered(scene).collect();
    if copied.is_empty() {
        return None;
    }
    let ids: HashSet<&str> = copied.iter().map(Element::id).collect();

    let mut elements = Vec::with_capacity(copied.len());
    let mut files = Object::new();
    for element in &copied {
        let Some(mut object) = scene.snapshot(element.id()) else {
            continue;
        };
        // Excalidraw lets an element leave a frame that is not copied.
        let frame = element.member("frameId").and_then(Value::as_str);
        if frame.is_some_and(|frame| !ids.contains(frame)) {
            object.insert("frameId".into(), Value::Null);
        }
        if let Some(file_id) = element.file_id()
            && let Some(record) = scene.file_record(file_id)
        {
            files.insert(file_id.to_owned(), record.clone());
        }
        elements.push(Value::Object(object));
    }

    let mut clipboard = Object::new();
    clipboard.insert("type".into(), Value::String(CLIPBOARD.into()));
    clipboard.insert("elements".into(), Value::Array(elements));
    clipboard.insert("files".into(), Value::Object(files));
    Some(Value::Object(clipboard).to_json())
}

/// Adds the Elements of `text`, if it is Excalidraw's clipboard JSON, to the
/// Scene as part of `step`, their middle at `centre`. Returns the ids of what
/// is to be selected (the Elements the core knows, without their labels), or
/// `None` if `text` is something else or has no Elements.
pub(crate) fn paste(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    text: &str,
    centre: (f64, f64),
) -> Option<Vec<String>> {
    let Ok(Value::Object(payload)) = parse(text) else {
        return None;
    };
    if !ACCEPTED.contains(&payload.get("type")?.as_str()?) {
        return None;
    }
    let originals = pasted_elements(payload.get("elements")?.as_array()?);
    if originals.is_empty() {
        return None;
    }

    let middle = extent(&originals)?.centre();
    let offset = (centre.0 - middle.0, centre.1 - middle.1);
    let ids: Vec<String> = originals.iter().map(|(id, _)| id.clone()).collect();
    let mut copying = Copying::new(&ids, entropy, None);
    let files = payload.get("files").and_then(Value::as_object);

    let mut pasted = Vec::with_capacity(originals.len());
    for (original, mut element) in originals {
        let id = copying.make_copy(&mut element, &original, entropy, offset);
        let frame = match element.get("frameId").and_then(Value::as_str) {
            Some(frame) => copying.copy_of(frame).map(|f| Value::String(f.to_owned())),
            None => None,
        };
        element.insert("frameId".into(), frame.unwrap_or(Value::Null));
        if let Some(file_id) = element.get("fileId").and_then(Value::as_str)
            && let Some(record) = files.and_then(|files| files.get(file_id))
            && record.as_object().is_some()
        {
            scene.add_file_record(file_id, record.clone());
        }
        step.create(&id);
        scene.push(element);
        pasted.push(id);
    }

    // Labels cannot be selected on their own, nor can what the core does not
    // know.
    let selected = pasted
        .into_iter()
        .filter(|id| {
            scene
                .element_by_id(id)
                .is_some_and(|e| e.container_id().is_none())
        })
        .collect();
    Some(selected)
}

/// The Elements of the payload to paste, with the id each has in it: the ones
/// that have an id (the first of each) and are not deleted, as Elements of
/// the types the core knows have Excalidraw's defaults for what is missing.
fn pasted_elements(payload: &[Value]) -> Vec<(String, Object)> {
    let mut seen = HashSet::new();
    let mut elements = Vec::new();
    for value in payload {
        let Some(element) = value.as_object() else {
            continue;
        };
        let Some(id) = element.get("id").and_then(Value::as_str) else {
            continue;
        };
        if element.get("isDeleted").and_then(Value::as_bool) == Some(true)
            || !seen.insert(id.to_owned())
        {
            continue;
        }
        let mut element = element.clone();
        let kind = element
            .get("type")
            .and_then(Value::as_str)
            .and_then(ElementType::from_name);
        if let Some(kind) = kind {
            restore::fill_missing(&mut element, kind);
        }
        elements.push((id.to_owned(), element));
    }
    elements
}

/// The smallest box around the Elements as they are shown (Excalidraw's
/// `getCommonBounds`). Elements the core does not know count with their
/// boxes as they are.
fn extent(elements: &[(String, Object)]) -> Option<Frame> {
    elements
        .iter()
        .map(|(_, object)| {
            if let Some(element) = Element::view(0, object) {
                return Frame::shown(&element);
            }
            let number = |key: &str| object.get(key).and_then(Value::as_f64).unwrap_or(0.0);
            let (x, y) = (number("x"), number("y"));
            let (width, height) = (number("width"), number("height"));
            Frame {
                min: (x.min(x + width), y.min(y + height)),
                max: (x.max(x + width), y.max(y + height)),
            }
        })
        .reduce(Frame::union)
}
