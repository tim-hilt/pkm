//! `Editor::geometry` regenerates only Elements whose drawing may have
//! changed (issue 56). Tested at that seam: after every kind of change the
//! output equals that of a fresh `Editor` loaded with the same Scene, and
//! unchanged Elements are not generated again.

use excalidraw_native_core::{Editor, Geometry, Tool, ZOrder};

use crate::common::{editor_with, replay, tap};

fn rect(id: &str, x: f64) -> String {
    format!(
        r##"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"index":null,
        "seed":11,"version":3,"versionNonce":7}}"##
    )
}

fn arrow(id: &str) -> String {
    format!(
        r##"{{"id":"{id}","type":"arrow","x":0,"y":100,"width":80,"height":0,
        "points":[[0,0],[80,0]],"startArrowhead":"circle_outline","endArrowhead":"triangle_outline",
        "strokeWidth":2,"backgroundColor":"transparent","roundness":null,"index":null,
        "seed":5,"version":2,"versionNonce":3}}"##
    )
}

fn three() -> Editor {
    editor_with(&[&rect("a", 0.0), &rect("b", 100.0), &arrow("c")])
}

fn drawing(editor: &Editor, background: &str) -> Vec<u8> {
    let saved = String::from_utf8(editor.save()).unwrap();
    // The Scene's own background is what a fresh editor must share.
    saved
        .replacen("\"#ffffff\"", &format!("\"{background}\""), 1)
        .into_bytes()
}

fn same(a: &Geometry, b: &Geometry) -> bool {
    a.parts() == b.parts()
        && a.verbs() == b.verbs()
        && a.coords() == b.coords()
        && a.text_lines() == b.text_lines()
        && a.text() == b.text()
        && a.images() == b.images()
        && a.file_ids() == b.file_ids()
}

/// Asserts that `editor` draws what a fresh editor with its Scene draws.
fn assert_matches_fresh(editor: &Editor) {
    let mut fresh = Editor::new();
    fresh.load(&editor.save()).unwrap();
    assert!(same(&editor.geometry(), &fresh.geometry()));
}

#[test]
fn repeating_geometry_generates_nothing() {
    let editor = three();
    editor.geometry();
    let generated = editor.elements_generated();
    assert_eq!(generated, 3);

    editor.geometry();

    assert_eq!(editor.elements_generated(), generated);
}

#[test]
fn moving_one_element_regenerates_only_it() {
    let mut editor = three();
    editor.geometry();
    let before = editor.elements_generated();

    tap(&mut editor, "mouse", (120.0, 10.0), 0.0);
    replay(
        &mut editor,
        "1.0 down 1 mouse 120 10\n1.1 move 1 mouse 150 40\n1.2 move 1 mouse 160 50\n1.3 up 1 mouse 160 50",
    );
    editor.geometry();

    // One regeneration per distinct version the Element went through.
    assert!(editor.elements_generated() - before <= 3);
    assert_matches_fresh(&editor);
}

#[test]
fn edits_deletes_and_undo_match_a_fresh_editor() {
    let mut editor = three();
    editor.geometry();

    editor.set_tool(Tool::Rectangle);
    replay(
        &mut editor,
        "1.0 down 1 mouse 300 300\n1.1 move 1 mouse 340 330\n1.2 up 1 mouse 340 330",
    );
    assert_matches_fresh(&editor);

    editor.delete_selection();
    assert_matches_fresh(&editor);
    editor.undo();
    assert_matches_fresh(&editor);
    editor.redo();
    assert_matches_fresh(&editor);
    editor.undo();
    editor.undo();
    assert_matches_fresh(&editor);

    editor.select_all();
    editor.move_in_z_order(ZOrder::ToBack);
    assert_matches_fresh(&editor);
}

#[test]
fn deleted_elements_leave_the_cache_and_come_back() {
    let mut editor = three();
    editor.geometry();
    tap(&mut editor, "mouse", (120.0, 10.0), 0.0);
    editor.delete_selection();
    editor.geometry();
    let generated = editor.elements_generated();

    editor.undo();
    let geometry = editor.geometry();

    assert_eq!(geometry.parts().iter().map(|p| p.element).max(), Some(2));
    assert!(editor.elements_generated() > generated);
    assert_matches_fresh(&editor);
}

#[test]
fn the_eraser_preview_fades_without_regenerating() {
    let mut editor = three();
    editor.geometry();
    let generated = editor.elements_generated();
    editor.set_tool(Tool::Eraser);

    replay(
        &mut editor,
        "0.0 down 1 mouse 20 -10\n0.1 move 1 mouse 20 30",
    );
    let preview = editor.geometry();
    assert!(preview.parts().iter().any(|p| p.opacity < 1.0));
    assert_eq!(editor.elements_generated(), generated);

    replay(&mut editor, "0.2 up 1 mouse 20 30");
    assert_matches_fresh(&editor);
    // Cancelling brings the Elements back unfaded.
}

#[test]
fn loading_another_background_regenerates_what_it_changes() {
    let mut editor = three();
    editor.geometry();
    let bytes = drawing(&editor, "#000000");
    editor.load(&bytes).unwrap();

    let mut fresh = Editor::new();
    fresh.load(&bytes).unwrap();
    assert!(same(&editor.geometry(), &fresh.geometry()));
}

#[test]
fn texts_and_images_are_cached_and_match_a_fresh_editor() {
    let text = r##"{"id":"t","type":"text","x":10,"y":200,"width":60,"height":50,"text":"two\nlines",
        "originalText":"two\nlines","fontSize":20,"fontFamily":5,"lineHeight":1.25,
        "textAlign":"left","verticalAlign":"top","strokeColor":"#1e1e1e","version":4,"versionNonce":1}"##;
    let image = r##"{"id":"i","type":"image","x":200,"y":200,"width":50,"height":40,"fileId":"f1",
        "status":"saved","scale":[1,1],"version":2,"versionNonce":1}"##;
    let editor = editor_with(&[&rect("a", 0.0), text, image, &rect("b", 100.0)]);
    editor.geometry();
    let generated = editor.elements_generated();
    assert_eq!(generated, 4);

    assert!(same(&editor.geometry(), &editor.geometry()));
    assert_eq!(editor.elements_generated(), generated);
    assert_matches_fresh(&editor);
    assert_eq!(editor.geometry().text_lines().len(), 2);
    assert_eq!(editor.geometry().images().len(), 1);
}
