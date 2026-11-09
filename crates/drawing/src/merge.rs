//! Merging two versions of a Drawing that changed apart (ADR-0011): the pure
//! function behind the Sync engine, matching Excalidraw's `reconcileElements`
//! (`reconcile.ts` at the pinned commit) for the Elements.
//!
//! - Elements are matched by id; the higher `version` wins, and on equal
//!   versions the lower `versionNonce` does. Erased Elements are `isDeleted`
//!   Elements like any other, so an erase merges as any other change.
//! - The Elements are ordered by fractional index (then id), colliding or
//!   missing indices repaired.
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

fn reconcile_elements(local: Vec<Value>, remote: Vec<Value>) -> Vec<Value> {
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
