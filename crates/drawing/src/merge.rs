//! Merging two versions of a Drawing that changed apart (ADR-0011): the pure
//! function behind the Sync engine, matching Excalidraw's `reconcileElements`
//! (`reconcile.ts` at the pinned commit) for the Elements.
//!
//! - Elements are matched by id; the higher `version` wins, and on equal
//!   versions the lower `versionNonce` does. Erased Elements are `isDeleted`
//!   Elements like any other, so an erase merges as any other change.
//! - The Elements are ordered by fractional index (then id), colliding or
//!   missing indices repaired; missing ones first on each side, from where
//!   the Element is there.
//! - `files` is the union by id.
//! - `appState` and the other top-level members are merged key by key
//!   against the last-Synced state; the remote wins a key both sides changed,
//!   and every key when there is no last-Synced state.
//! - Whatever it does not interpret (other members, unknown Element
//!   properties) is kept (ADR-0003).
//!
//! The Elements, `files` and any key changed on one side only merge in any
//! order, any number of times, to the same bytes. If one side already holds
//! everything the merge gives, that side's bytes are returned as they are.

use std::collections::HashMap;
use std::fmt;

use crate::index_sync;
use pkm_base::json::{self, Object, Value};

/// Why bytes could not be merged: a side is not a Drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeError(String);

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MergeError {}

/// The bytes of the Drawing that `local` and `remote` merge to, given the
/// bytes of the Drawing at the last Sync, if any (`base` that is not a
/// Drawing counts as none).
pub fn merge_drawings(
    base: Option<&[u8]>,
    local: &[u8],
    remote: &[u8],
) -> Result<Vec<u8>, MergeError> {
    let base = base.and_then(|bytes| parse(bytes).ok());
    let (local_doc, remote_doc) = (parse(local)?, parse(remote)?);

    let mut merged = merge_members(
        base.as_ref().map(|d| &d.root),
        &local_doc.root,
        &remote_doc.root,
        &["elements", "appState", "files"],
    );
    merged.insert(
        "elements".into(),
        Value::Array(reconcile_elements(
            local_doc.elements(),
            remote_doc.elements(),
        )),
    );
    merged.insert(
        "appState".into(),
        Value::Object(merge_members(
            base.as_ref().and_then(|d| d.members("appState")),
            local_doc.members("appState").unwrap_or(&Object::new()),
            remote_doc.members("appState").unwrap_or(&Object::new()),
            &[],
        )),
    );
    merged.insert(
        "files".into(),
        Value::Object(union_files(
            local_doc.members("files"),
            remote_doc.members("files"),
        )),
    );
    let merged = Value::Object(order_members(merged));

    // The side that already has it all stays as it is, byte for byte.
    let (remote_root, local_root) = (
        Value::Object(remote_doc.root),
        Value::Object(local_doc.root),
    );
    if equivalent(&merged, &remote_root) {
        Ok(remote.to_vec())
    } else if equivalent(&merged, &local_root) {
        Ok(local.to_vec())
    } else {
        Ok(merged.to_json_pretty().into_bytes())
    }
}

/// Merges `remote`, bytes that Sync wrote, into `local`, the open Drawing,
/// for the Elements whose id `keeps_local` accepts take no part from the
/// remote side: they stay as they are in `local` (those are being dragged,
/// drawn or typed into). With no Sync history to go on, the remote wins
/// every `appState` key.
pub fn merge_keeping_local(
    local: &[u8],
    remote: &[u8],
    keeps_local: impl Fn(&str) -> bool,
) -> Result<Vec<u8>, MergeError> {
    let mut remote_doc = parse(remote)?;
    let Some(Value::Array(elements)) = remote_doc.root.get_mut("elements") else {
        return merge_drawings(None, local, remote);
    };
    if !elements.iter().any(|e| id_of(e).is_some_and(&keeps_local)) {
        return merge_drawings(None, local, remote);
    }
    elements.retain(|e| !id_of(e).is_some_and(&keeps_local));
    let trimmed = Value::Object(remote_doc.root).to_json_pretty();
    merge_drawings(None, local, trimmed.as_bytes())
}

/// Whether `bytes` are a Drawing that `merge_drawings` can merge.
pub fn is_drawing(bytes: &[u8]) -> bool {
    parse(bytes).is_ok()
}

/// Whether two same-named Drawings with no Sync history between them are one
/// Drawing: they are if they share at least one Element id, or both have no
/// Elements. Otherwise they are unrelated and must both be kept.
pub fn are_same_drawing(a: &[u8], b: &[u8]) -> Result<bool, MergeError> {
    let (a, b) = (parse(a)?, parse(b)?);
    let (a, b) = (a.elements(), b.elements());
    if a.is_empty() && b.is_empty() {
        return Ok(true);
    }
    let ids: std::collections::HashSet<&str> = a.iter().filter_map(id_of).collect();
    Ok(b.iter().filter_map(id_of).any(|id| ids.contains(id)))
}

/// A parsed Drawing.
pub(crate) struct Doc {
    /// The whole document, as read.
    pub(crate) root: Object,
}

impl Doc {
    fn elements(&self) -> Vec<Value> {
        match self.root.get("elements") {
            Some(Value::Array(elements)) => elements.clone(),
            _ => Vec::new(),
        }
    }

    fn members(&self, key: &str) -> Option<&Object> {
        self.root.get(key)?.as_object()
    }
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Doc, MergeError> {
    let err = |what: &str| MergeError(format!("not a Drawing: {what}"));
    let text = std::str::from_utf8(bytes).map_err(|_| err("not text"))?;
    let Value::Object(mut root) = json::parse(text).map_err(|e| err(&e.to_string()))? else {
        return Err(err("the document is not an object"));
    };
    if root.get("type").and_then(Value::as_str) != Some("excalidraw") {
        return Err(err("`type` is not \"excalidraw\""));
    }
    if root.get("elements").is_none() {
        root.insert("elements".into(), Value::Array(Vec::new()));
    }
    let Some(elements) = root.get_mut("elements").and_then(Value::as_array_mut) else {
        return Err(err("`elements` is not an array"));
    };
    if elements.iter().any(|e| id_of(e).is_none()) {
        return Err(err("an element has no id"));
    }
    Ok(Doc { root })
}

fn id_of(element: &Value) -> Option<&str> {
    element.get("id")?.as_str()
}

fn index_of(element: &Value) -> Option<&str> {
    element.get("index")?.as_str()
}

/// Gives the Elements whose index is missing, invalid or colliding a valid
/// one, in the order they are in, without touching anything else.
fn repair_indices(elements: &mut [Value]) {
    let updates = {
        let indices: Vec<_> = elements.iter().map(index_of).collect();
        index_sync::sync_moved(&indices, &vec![false; elements.len()])
    };
    set_indices(elements, updates);
}

/// Gives the Elements without a valid index one from their place, see
/// [`index_sync::fill_missing`].
fn fill_missing_indices(elements: &mut [Value]) {
    let updates = {
        let indices: Vec<_> = elements.iter().map(index_of).collect();
        index_sync::fill_missing(&indices)
    };
    set_indices(elements, updates);
}

fn set_indices(elements: &mut [Value], updates: Vec<(usize, String)>) {
    for (position, index) in updates {
        if let Some(element) = elements[position].as_object_mut() {
            element.insert("index".into(), Value::String(index));
        }
    }
}

/// Whether `a` beats `b` for the same Element: the higher `version`, then the
/// lower `versionNonce`, then (for a total order) the smaller serialization.
fn wins(a: &Value, b: &Value) -> bool {
    let number = |e: &Value, key| e.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let (va, vb) = (number(a, "version"), number(b, "version"));
    if va != vb {
        return va > vb;
    }
    let (na, nb) = (number(a, "versionNonce"), number(b, "versionNonce"));
    if na != nb {
        return na < nb;
    }
    a.to_json() < b.to_json()
}

fn reconcile_elements(mut local: Vec<Value>, mut remote: Vec<Value>) -> Vec<Value> {
    // An Element without an index (a new one) gets one from its place on its
    // side, not the bottom of the merge.
    fill_missing_indices(&mut local);
    fill_missing_indices(&mut remote);
    let mut by_id: HashMap<String, Value> = HashMap::new();
    for element in local.into_iter().chain(remote) {
        let id = id_of(&element).unwrap_or_default().to_owned();
        match by_id.get(&id) {
            Some(held) if !wins(&element, held) => {}
            _ => {
                by_id.insert(id, element);
            }
        }
    }
    let mut elements: Vec<Value> = by_id.into_values().collect();
    elements.sort_by(|a, b| {
        (index_of(a).unwrap_or(""), id_of(a)).cmp(&(index_of(b).unwrap_or(""), id_of(b)))
    });
    repair_indices(&mut elements);
    elements
}

/// The union of two `files` maps; the remote's entry for an id both have.
fn union_files(local: Option<&Object>, remote: Option<&Object>) -> Object {
    let mut files = Object::new();
    for (id, file) in local.into_iter().chain(remote).flat_map(Object::iter) {
        files.insert(id.to_owned(), file.clone());
    }
    sorted(files)
}

/// Key by key: the remote's value if the remote changed it since `base` (or
/// there is no `base`), else the local one. Keys in `skip` are left out.
fn merge_members(base: Option<&Object>, local: &Object, remote: &Object, skip: &[&str]) -> Object {
    let mut merged = Object::new();
    let keys = local.iter().chain(remote.iter()).map(|(key, _)| key);
    for key in keys.filter(|key| !skip.contains(key)) {
        let (l, r) = (local.get(key), remote.get(key));
        let chosen = match base {
            None => r.or(l),
            Some(base) if r != base.get(key) => r,
            Some(_) => l,
        };
        if let Some(value) = chosen {
            merged.insert(key.to_owned(), value.clone());
        }
    }
    // A key only the base has was removed on both sides: nothing to add.
    sorted(merged)
}

/// `members` with the keys in alphabetical order, so that the order of the
/// sides does not show.
fn sorted(members: Object) -> Object {
    let mut entries: Vec<_> = members
        .iter()
        .map(|(k, v)| (k.to_owned(), v.clone()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Object::new();
    for (key, value) in entries {
        out.insert(key, value);
    }
    out
}

/// The top-level members in excalidraw.com's order, unknown ones after them.
fn order_members(root: Object) -> Object {
    const KNOWN: [&str; 6] = ["type", "version", "source", "elements", "appState", "files"];
    let mut out = Object::new();
    for key in KNOWN {
        if let Some(value) = root.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    for (key, value) in root.iter().filter(|(key, _)| !KNOWN.contains(key)) {
        out.insert(key.to_owned(), value.clone());
    }
    out
}

/// Whether two values hold the same content: objects regardless of the order
/// of their members, arrays in order.
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| equivalent(value, other)))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    mod merge {
        //! Merging a Drawing changed apart (`merge_drawings`), through bytes in and
        //! bytes out. Expected values are literals worked out from Excalidraw's
        //! `reconcileElements`, never recomputed with the merge's own rules.

        use crate::{are_same_drawing, merge_drawings};
        use pkm_base::json::{Value, parse};

        fn element(id: &str, version: u32, nonce: u32, index: &str) -> String {
            format!(
                r#"{{"id":"{id}","type":"rectangle","version":{version},"versionNonce":{nonce},"index":"{index}","isDeleted":false,"x":0}}"#
            )
        }

        fn deleted(id: &str, version: u32, index: &str) -> String {
            element(id, version, 1, index).replace(r#""isDeleted":false"#, r#""isDeleted":true"#)
        }

        fn drawing(elements: &[String], app_state: &str, files: &str) -> Vec<u8> {
            format!(
        r##"{{"type":"excalidraw","version":2,"source":"test","elements":[{}],"appState":{app_state},"files":{files}}}"##,
        elements.join(",")
    )
    .into_bytes()
        }

        fn elements_only(elements: &[String]) -> Vec<u8> {
            drawing(elements, "{}", "{}")
        }

        fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Vec<u8> {
            merge_drawings(base, local, remote).expect("mergeable")
        }

        fn parsed(bytes: &[u8]) -> Value {
            parse(std::str::from_utf8(bytes).unwrap()).unwrap()
        }

        /// `(id, version, isDeleted)` of the Elements, in order.
        fn summary(bytes: &[u8]) -> Vec<(String, u32, bool)> {
            parsed(bytes)
                .get("elements")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    (
                        e.get("id").unwrap().as_str().unwrap().to_owned(),
                        e.get("version").unwrap().as_f64().unwrap() as u32,
                        e.get("isDeleted").unwrap().as_bool().unwrap(),
                    )
                })
                .collect()
        }

        fn indices(bytes: &[u8]) -> Vec<String> {
            parsed(bytes)
                .get("elements")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.get("index").unwrap().as_str().unwrap().to_owned())
                .collect()
        }

        fn t(id: &str, version: u32, deleted: bool) -> (String, u32, bool) {
            (id.to_owned(), version, deleted)
        }

        #[test]
        fn elements_changed_on_different_sides_are_both_kept() {
            let base = elements_only(&[element("a", 1, 5, "a0"), element("b", 1, 5, "a1")]);
            let local = elements_only(&[element("a", 2, 5, "a0"), element("b", 1, 5, "a1")]);
            let remote = elements_only(&[element("a", 1, 5, "a0"), element("b", 3, 5, "a1")]);

            let merged = merge(Some(&base), &local, &remote);

            assert_eq!(summary(&merged), [t("a", 2, false), t("b", 3, false)]);
        }

        #[test]
        fn the_higher_version_wins() {
            let local = elements_only(&[element("a", 4, 9, "a0")]);
            let remote = elements_only(&[element("a", 3, 1, "a0")]);
            assert_eq!(summary(&merge(None, &local, &remote)), [t("a", 4, false)]);
            assert_eq!(summary(&merge(None, &remote, &local)), [t("a", 4, false)]);
        }

        #[test]
        fn equal_versions_are_decided_by_the_lower_version_nonce() {
            let low = element("a", 2, 10, "a0").replace(r#""x":0"#, r#""x":111"#);
            let high = element("a", 2, 20, "a0").replace(r#""x":0"#, r#""x":222"#);
            let (low, high) = (elements_only(&[low]), elements_only(&[high]));

            for (local, remote) in [(&low, &high), (&high, &low)] {
                let merged = parsed(&merge(None, local, remote));
                let winner = &merged.get("elements").unwrap().as_array().unwrap()[0];
                assert_eq!(winner.get("x").unwrap().as_f64(), Some(111.0));
            }
        }

        #[test]
        fn an_erase_and_edits_to_other_elements_both_survive() {
            let base = elements_only(&[element("a", 1, 5, "a0"), element("b", 1, 5, "a1")]);
            let local = elements_only(&[deleted("a", 2, "a0"), element("b", 1, 5, "a1")]);
            let remote = elements_only(&[element("a", 1, 5, "a0"), element("b", 2, 5, "a1")]);

            let merged = merge(Some(&base), &local, &remote);

            assert_eq!(summary(&merged), [t("a", 2, true), t("b", 2, false)]);
        }

        #[test]
        fn files_are_the_union_by_id() {
            let local = drawing(&[], "{}", r##"{"f1":{"id":"f1","dataURL":"x"}}"##);
            let remote = drawing(&[], "{}", r##"{"f2":{"id":"f2","dataURL":"y"}}"##);

            let merged = parsed(&merge(None, &local, &remote));

            let files = merged.get("files").unwrap().as_object().unwrap();
            assert!(files.get("f1").is_some() && files.get("f2").is_some());
            assert_eq!(files.len(), 2);
        }

        #[test]
        fn app_state_merges_key_by_key_against_the_last_synced_state() {
            let base = drawing(
                &[],
                r##"{"gridSize":null,"viewBackgroundColor":"#ffffff","zoom":1}"##,
                "{}",
            );
            let local = drawing(
                &[],
                r##"{"gridSize":20,"viewBackgroundColor":"#ffffff","zoom":1}"##,
                "{}",
            );
            let remote = drawing(
                &[],
                r##"{"gridSize":null,"viewBackgroundColor":"#000000","zoom":1}"##,
                "{}",
            );

            let merged = parsed(&merge(Some(&base), &local, &remote));

            let state = merged.get("appState").unwrap();
            assert_eq!(state.get("gridSize").unwrap().as_f64(), Some(20.0));
            assert_eq!(
                state.get("viewBackgroundColor").unwrap().as_str(),
                Some("#000000")
            );
            assert_eq!(state.get("zoom").unwrap().as_f64(), Some(1.0));
        }

        #[test]
        fn the_remote_wins_an_app_state_key_both_sides_changed() {
            let base = drawing(&[], r##"{"gridSize":null}"##, "{}");
            let local = drawing(&[], r##"{"gridSize":20}"##, "{}");
            let remote = drawing(&[], r##"{"gridSize":40}"##, "{}");

            let merged = parsed(&merge(Some(&base), &local, &remote));

            assert_eq!(
                merged
                    .get("appState")
                    .unwrap()
                    .get("gridSize")
                    .unwrap()
                    .as_f64(),
                Some(40.0)
            );
        }

        #[test]
        fn without_a_last_synced_state_the_remote_wins_app_state() {
            let local = drawing(&[], r##"{"gridSize":20,"a":1}"##, "{}");
            let remote = drawing(&[], r##"{"gridSize":40,"b":2}"##, "{}");

            let merged = parsed(&merge(None, &local, &remote));

            let state = merged.get("appState").unwrap();
            assert_eq!(state.get("gridSize").unwrap().as_f64(), Some(40.0));
            assert_eq!(state.get("a").unwrap().as_f64(), Some(1.0));
            assert_eq!(state.get("b").unwrap().as_f64(), Some(2.0));
        }

        #[test]
        fn elements_are_ordered_by_fractional_index() {
            // The local file lists them in another order than their indices say.
            let local = elements_only(&[element("c", 1, 1, "a2"), element("a", 1, 1, "a0")]);
            let remote = elements_only(&[element("b", 1, 1, "a1")]);

            let merged = merge(None, &local, &remote);

            let ids: Vec<_> = summary(&merged).into_iter().map(|e| e.0).collect();
            assert_eq!(ids, ["a", "b", "c"]);
        }

        #[test]
        fn an_element_without_an_index_keeps_its_place_in_its_side() {
            // A new Element has no index until a merge gives it one.
            let new = element("new", 1, 1, "").replace(r#""index":"""#, r#""index":null"#);
            let local = elements_only(&[element("image", 1, 1, "a0"), new]);
            let remote = elements_only(&[element("image", 1, 1, "a0")]);

            let merged = merge(None, &local, &remote);

            let ids: Vec<_> = summary(&merged).into_iter().map(|e| e.0).collect();
            assert_eq!(ids, ["image", "new"]);
            let indices = indices(&merged);
            assert!(indices[0] < indices[1], "{indices:?}");
        }

        #[test]
        fn colliding_indices_are_fixed() {
            let local = elements_only(&[element("x", 1, 1, "a0")]);
            let remote = elements_only(&[element("y", 1, 1, "a0")]);

            let merged = merge(None, &local, &remote);

            let indices = indices(&merged);
            assert_eq!(indices.len(), 2);
            assert!(indices[0] < indices[1], "{indices:?}");
            let ids: Vec<_> = summary(&merged).into_iter().map(|e| e.0).collect();
            assert_eq!(ids.len(), 2);
        }

        #[test]
        fn unknown_members_and_element_properties_are_kept() {
            let local = br#"{"type":"excalidraw","version":2,"extra":{"k":[1,2]},"elements":[{"id":"a","version":1,"versionNonce":1,"index":"a0","isDeleted":false,"future":{"n":true}}],"appState":{},"files":{}}"#;
            let remote = elements_only(&[element("b", 1, 1, "a1")]);

            let merged = parsed(&merge(None, local, &remote));

            assert_eq!(merged.get("extra").unwrap().to_json(), r##"{"k":[1,2]}"##);
            let first = &merged.get("elements").unwrap().as_array().unwrap()[0];
            assert_eq!(first.get("future").unwrap().to_json(), r##"{"n":true}"##);
        }

        #[test]
        fn merging_in_any_order_any_number_of_times_gives_the_same_bytes() {
            let base = elements_only(&[element("a", 1, 5, "a0")]);
            let one = drawing(
                &[element("a", 2, 5, "a0"), element("b", 1, 5, "a1")],
                r##"{"gridSize":20}"##,
                r##"{"f1":{"id":"f1"}}"##,
            );
            let two = drawing(
                &[deleted("a", 3, "a0"), element("c", 1, 5, "a2")],
                r##"{"viewBackgroundColor":"#000000"}"##,
                r##"{"f2":{"id":"f2"}}"##,
            );
            let three = elements_only(&[element("d", 1, 5, "a3")]);
            let b = Some(&base[..]);

            let expected = merge(b, &merge(b, &one, &two), &three);

            for (x, y, z) in [
                (&one, &three, &two),
                (&two, &one, &three),
                (&two, &three, &one),
                (&three, &one, &two),
                (&three, &two, &one),
            ] {
                assert_eq!(merge(b, &merge(b, x, y), z), expected);
            }
            assert_eq!(merge(b, &expected, &one), expected);
            assert_eq!(merge(b, &expected, &expected), expected);
            assert_eq!(merge(b, &one, &two), merge(b, &two, &one));
        }

        #[test]
        fn a_side_that_already_has_everything_is_returned_byte_for_byte() {
            let remote = br#"{ "type": "excalidraw", "elements": [ {"id":"a","version":3,"versionNonce":1,"index":"a0","isDeleted":false} ], "appState": { "z": 1, "a": 2 }, "files": {} }"#;
            let local = elements_only(&[element("a", 1, 1, "a0")]);

            assert_eq!(merge(Some(&local), &local, remote), remote);
        }

        #[test]
        fn bytes_that_are_no_drawing_are_refused() {
            let fine = elements_only(&[]);
            assert!(merge_drawings(None, b"not json", &fine).is_err());
            assert!(merge_drawings(None, &fine, br##"{"type":"other"}"##).is_err());
            assert!(
                merge_drawings(None, &fine, br##"{"type":"excalidraw","elements":[{}]}"##).is_err()
            );
        }

        // --- Sameness of same-named Drawings ----------------------------------------

        #[test]
        fn drawings_sharing_an_element_id_are_the_same() {
            let a = elements_only(&[element("a", 1, 1, "a0"), element("b", 1, 1, "a1")]);
            let b = elements_only(&[element("c", 1, 1, "a0"), element("b", 5, 2, "a1")]);
            assert_eq!(are_same_drawing(&a, &b), Ok(true));
        }

        #[test]
        fn a_shared_id_counts_even_when_erased() {
            let a = elements_only(&[element("a", 1, 1, "a0")]);
            let b = elements_only(&[deleted("a", 2, "a0")]);
            assert_eq!(are_same_drawing(&a, &b), Ok(true));
        }

        #[test]
        fn drawings_without_a_shared_id_are_unrelated() {
            let a = elements_only(&[element("a", 1, 1, "a0")]);
            let b = elements_only(&[element("b", 1, 1, "a0")]);
            assert_eq!(are_same_drawing(&a, &b), Ok(false));
        }

        #[test]
        fn two_empty_drawings_are_the_same_but_an_empty_and_a_filled_one_are_not() {
            let empty = elements_only(&[]);
            let filled = elements_only(&[element("a", 1, 1, "a0")]);
            assert_eq!(are_same_drawing(&empty, &empty), Ok(true));
            assert_eq!(are_same_drawing(&empty, &filled), Ok(false));
        }

        #[test]
        fn sameness_of_a_non_drawing_is_an_error() {
            assert!(are_same_drawing(b"nope", &elements_only(&[])).is_err());
        }
    }

    mod merge_synced {
        //! Taking in a Drawing that Sync rewrote while it is open
        //! (`Editor::merge_synced`), tested at the Editor seam: bytes in, saved
        //! Drawing, Selection, undo availability and viewport out. Expected values
        //! are literals worked out from Excalidraw's `version` rule.

        use std::sync::Arc;

        use crate::{Editor, Tool};
        use pkm_base::json::Value;

        use crate::test_support as common;
        use common::{FixedWidth, editor_with, num, replay, saved_elements, tap};

        /// A rectangle at (100, 100), 100 x 50, version 3.
        const A: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "roundness":null,"strokeWidth":2,"backgroundColor":"#ffc9c9","version":3,"versionNonce":7,"index":"a0","isDeleted":false}"##;

        /// A rectangle at (400, 400), 100 x 50, version 3.
        const B: &str = r##"{"id":"b","type":"rectangle","x":400,"y":400,"width":100,"height":50,
    "roundness":null,"strokeWidth":2,"backgroundColor":"#ffc9c9","version":3,"versionNonce":8,"index":"a1","isDeleted":false}"##;

        fn drawing(elements: &[&str]) -> Vec<u8> {
            format!(
                r#"{{"type":"excalidraw","version":2,"elements":[{}]}}"#,
                elements.join(",")
            )
            .into_bytes()
        }

        /// `element` as edited elsewhere: version 9 with `from` replaced by `to`.
        fn edited(element: &str, from: &str, to: &str) -> String {
            assert!(element.contains(from), "{from}");
            element
                .replace(r#""version":3"#, r#""version":9"#)
                .replace(from, to)
        }

        fn element(editor: &Editor, id: &str) -> Value {
            saved_elements(editor)
                .into_iter()
                .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
                .unwrap_or_else(|| panic!("no element {id}"))
        }

        fn x_of(editor: &Editor, id: &str) -> f64 {
            num(&element(editor, id), "x")
        }

        fn is_deleted(editor: &Editor, id: &str) -> bool {
            element(editor, id)
                .get("isDeleted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        }

        /// Drags the rectangle `A` by 20 to the right with the mouse.
        fn drag_a(editor: &mut Editor) {
            replay(
                editor,
                "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 170 125
         1.2 up 1 mouse 180 125",
            );
        }

        #[test]
        fn changes_from_elsewhere_appear_in_the_scene() {
            let mut editor = editor_with(&[A]);
            let remote = drawing(&[A, &edited(B, r#""x":400"#, r#""x":450"#)]);
            editor.merge_synced(&remote).unwrap();

            assert_eq!(editor.element_count(), 2);
            assert_eq!(x_of(&editor, "b"), 450.0);
            assert_eq!(x_of(&editor, "a"), 100.0);
        }

        #[test]
        fn a_change_of_an_element_the_user_changed_too_goes_by_version() {
            let mut editor = editor_with(&[A]);
            drag_a(&mut editor); // a is version 4 at x = 120
            let remote = drawing(&[&edited(A, r#""x":100"#, r#""x":500"#)]); // version 9
            editor.merge_synced(&remote).unwrap();
            assert_eq!(x_of(&editor, "a"), 500.0);
        }

        #[test]
        fn merging_is_not_an_undo_step() {
            let mut editor = editor_with(&[A]);
            drag_a(&mut editor);
            editor.merge_synced(&drawing(&[A, B])).unwrap();

            // Undo takes back the user's drag, not the arrival of `b`.
            editor.undo();
            assert_eq!(x_of(&editor, "a"), 100.0);
            assert!(!is_deleted(&editor, "b"));
            assert!(!editor.can_undo());
            assert!(editor.can_redo());
        }

        #[test]
        fn merging_leaves_redo_as_it_was() {
            let mut editor = editor_with(&[A]);
            drag_a(&mut editor);
            editor.undo();
            editor.merge_synced(&drawing(&[A, B])).unwrap();

            assert!(editor.can_redo());
            editor.redo();
            assert_eq!(x_of(&editor, "a"), 120.0);
            assert!(!is_deleted(&editor, "b"));
        }

        #[test]
        fn the_selection_stays_less_the_elements_deleted_elsewhere() {
            let mut editor = editor_with(&[A, B]);
            tap(&mut editor, "mouse", (150.0, 125.0), 1.0);
            assert_eq!(editor.selection(), ["a"]);

            editor
                .merge_synced(&drawing(&[A, &edited(B, r#""x":400"#, r#""x":450"#)]))
                .unwrap();
            assert_eq!(editor.selection(), ["a"]);

            editor
                .merge_synced(&drawing(&[
                    &edited(A, r#""isDeleted":false"#, r#""isDeleted":true"#),
                    B,
                ]))
                .unwrap();
            assert!(editor.selection().is_empty());
        }

        #[test]
        fn the_viewport_stays() {
            let mut editor = editor_with(&[A]);
            editor.set_viewport((-30.0, 20.0), 2.0);
            let before = editor.viewport();
            editor.merge_synced(&drawing(&[A, B])).unwrap();
            assert_eq!(editor.viewport(), before);
        }

        #[test]
        fn the_revision_changes_so_clients_draw_again() {
            let mut editor = editor_with(&[A]);
            let before = editor.revision();
            editor.merge_synced(&drawing(&[A, B])).unwrap();
            assert_ne!(editor.revision(), before);
        }

        #[test]
        fn bytes_that_add_nothing_change_nothing() {
            let mut editor = Editor::new();
            let file = format!(
                r#"{{"type":"excalidraw","version":2,"elements":[{A}],"appState":{{}},"files":{{}}}}"#
            );
            editor.load(file.as_bytes()).unwrap();
            let before = editor.revision();
            // What the Drawing's own file says.
            let own = editor.save();
            editor.merge_synced(&own).unwrap();
            // Twice over, as Sync may rewrite a file with what it has already.
            editor.merge_synced(&own).unwrap();
            assert_eq!(editor.revision(), before);
        }

        #[test]
        fn bytes_that_are_not_a_drawing_are_refused_and_the_scene_kept() {
            let mut editor = editor_with(&[A]);
            assert!(editor.merge_synced(b"{\"type\":\"nope\"}").is_err());
            assert_eq!(editor.element_count(), 1);
        }

        #[test]
        fn an_element_being_dragged_keeps_the_local_version_until_the_drag_ends() {
            let mut editor = editor_with(&[A, B]);
            replay(
                &mut editor,
                "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 170 125",
            );
            assert_eq!(x_of(&editor, "a"), 120.0);

            let remote = drawing(&[
                &edited(A, r#""x":100"#, r#""x":500"#),
                &edited(B, r#""x":400"#, r#""x":450"#),
            ]);
            editor.merge_synced(&remote).unwrap();
            assert_eq!(x_of(&editor, "a"), 120.0, "dragged element keeps local");
            assert_eq!(x_of(&editor, "b"), 450.0, "others take the remote");

            // The drag goes on where the user's finger is.
            replay(
                &mut editor,
                "1.2 move 1 mouse 180 125
         1.3 up 1 mouse 180 125",
            );
            assert_eq!(x_of(&editor, "a"), 130.0);

            // Once it ended the normal version rule applies.
            editor.merge_synced(&remote).unwrap();
            assert_eq!(x_of(&editor, "a"), 500.0);
        }

        #[test]
        fn a_shape_being_drawn_is_not_taken_away_by_a_drawing_without_it() {
            let mut editor = Editor::new();
            editor.set_tool(Tool::Rectangle);
            replay(
                &mut editor,
                "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40",
            );
            editor.merge_synced(&drawing(&[B])).unwrap();
            replay(&mut editor, "0.2 up 1 mouse 60 40");
            assert_eq!(editor.element_count(), 2);
            assert_eq!(x_of(&editor, "b"), 400.0);
        }

        #[test]
        fn a_new_shape_stays_on_top_when_sync_brings_a_drawing_without_it() {
            let mut editor = editor_with(&[A]);
            editor.set_tool(Tool::Ellipse);
            replay(
                &mut editor,
                "0.0 down 1 mouse 120 110
         0.1 move 1 mouse 260 240
         0.2 up 1 mouse 260 240",
            );
            // Sync rewrites the file before the save with the ellipse reached it.
            editor.merge_synced(&drawing(&[A])).unwrap();

            let types: Vec<_> = saved_elements(&editor)
                .iter()
                .map(|e| e.get("type").and_then(Value::as_str).unwrap().to_owned())
                .collect();
            assert_eq!(types, ["rectangle", "ellipse"]);
        }

        #[test]
        fn the_open_text_view_and_its_text_stay() {
            let mut editor = Editor::new();
            editor.set_text_measurer(Arc::new(FixedWidth));
            editor.set_tool(Tool::Text);
            tap(&mut editor, "finger", (50.0, 50.0), 1.0);
            editor.set_editing_text("hello");

            editor.merge_synced(&drawing(&[A])).unwrap();

            let view = editor.text_editor().expect("the text view stays open");
            assert_eq!(view.text, "hello");
            editor.set_editing_text("hello!");
            assert_eq!(editor.text_editor().unwrap().text, "hello!");
            assert_eq!(editor.element_count(), 2);
        }
    }
}
