//! Merging a Drawing changed apart (`merge_drawings`), through bytes in and
//! bytes out. Expected values are literals worked out from Excalidraw's
//! `reconcileElements`, never recomputed with the merge's own rules.

use pkm_core::json::{Value, parse};
use pkm_core::{are_same_drawing, merge_drawings};

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
    assert!(merge_drawings(None, &fine, br##"{"type":"excalidraw","elements":[{}]}"##).is_err());
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
