//! A Drawing's Item ID: the top-level `"id"` property of its `.excalidraw`
//! file (ADR-0019). Excalidraw ignores properties it does not know, so the
//! file still opens on excalidraw.com, which drops the property when it
//! saves; the app then gives the Drawing a new one.

use crate::merge::{MergeError, parse};
use pkm_base::item_id::ItemId;
use pkm_base::json::Value;

/// The Item ID in the Drawing `bytes`, if they are a Drawing that holds one.
pub fn of(bytes: &[u8]) -> Option<ItemId> {
    let drawing = parse(bytes).ok()?;
    ItemId::parse(drawing.root.get("id")?.as_str()?)
}

/// The Drawing `bytes` with `id` as its Item ID, replacing the one it holds.
/// Everything else in it is kept (ADR-0003).
pub fn with(bytes: &[u8], id: &ItemId) -> Result<Vec<u8>, MergeError> {
    let mut drawing = parse(bytes)?;
    drawing
        .root
        .insert("id".into(), Value::String(id.to_string()));
    Ok(Value::Object(drawing.root).to_json_pretty().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pkm_base::json::parse as parse_json;

    const ID: &str = "k3j7x2m4q5ab";

    fn id(text: &str) -> ItemId {
        ItemId::parse(text).unwrap()
    }

    #[test]
    fn a_drawing_without_an_id_has_none() {
        assert_eq!(of(br#"{"type":"excalidraw","elements":[]}"#), None);
    }

    #[test]
    fn what_is_no_item_id_is_none() {
        assert_eq!(of(br#"{"type":"excalidraw","id":"short"}"#), None);
        assert_eq!(of(br#"{"type":"excalidraw","id":7}"#), None);
        assert_eq!(of(b"not a drawing"), None);
    }

    #[test]
    fn a_written_id_is_read_back() {
        let bytes = with(br#"{"type":"excalidraw","elements":[]}"#, &id(ID)).unwrap();
        assert_eq!(of(&bytes), Some(id(ID)));
    }

    #[test]
    fn writing_replaces_an_id_and_keeps_everything_else() {
        let before = format!(
            r##"{{"type":"excalidraw","id":"{ID}","version":2,"source":"x","elements":[],"appState":{{"a":1}},"files":{{}},"other":[true]}}"##
        );
        let after = with(before.as_bytes(), &id("234567abcdef")).unwrap();
        let expected = before.replace(ID, "234567abcdef");
        let (after, expected) = (
            parse_json(std::str::from_utf8(&after).unwrap()).unwrap(),
            parse_json(&expected).unwrap(),
        );
        assert_eq!(after, expected);
    }

    #[test]
    fn an_edited_drawing_keeps_its_id() {
        let bytes = with(br#"{"type":"excalidraw","elements":[]}"#, &id(ID)).unwrap();
        let mut editor = crate::Editor::new();
        editor.load(&bytes).unwrap();
        assert_eq!(of(&editor.save()), Some(id(ID)));
    }

    #[test]
    fn only_a_drawing_gets_an_id() {
        assert!(with(b"nope", &id(ID)).is_err());
    }
}
