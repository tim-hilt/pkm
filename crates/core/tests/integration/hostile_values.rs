//! Files with extreme or invalid Element values must not make the core do
//! unbounded work (issue 55), tested at the Editor seam.
//!
//! The bounded cases assert how much geometry comes out, with sizes chosen so
//! that code without the bounds makes a lot of it quickly instead of running
//! out of memory: they are safe to run on any machine. The full probe Drawing
//! is only meant for code that has the bounds.

use excalidraw_native_core::{Editor, ExportScope};

fn editor_with(elements: &str) -> Editor {
    let json =
        format!(r#"{{"type":"excalidraw","version":2,"elements":[{elements}],"files":{{}}}}"#);
    let mut editor = Editor::new();
    editor.load(json.as_bytes()).unwrap();
    editor
}

/// Every call that turns Element values into work.
fn use_all(editor: &mut Editor) -> usize {
    let drawn = editor.geometry().coords().len();
    let exported = editor.export(ExportScope::Scene);
    editor.select_all();
    let _ = editor.overlay();
    let _ = editor.properties();
    let _ = editor.copy_selection();
    editor.duplicate();
    editor.undo();
    editor.delete_selection();
    drawn + exported.map_or(0, |e| e.geometry.coords().len())
}

#[test]
fn a_tall_hatched_rectangle_is_not_filled_with_hundreds_of_thousands_of_lines() {
    // 300,000 scan lines, a hatch line every 8th.
    let mut editor = editor_with(
        r##"{"id":"a","type":"rectangle","width":50,"height":300000,"strokeWidth":2,
        "backgroundColor":"#ff0000","fillStyle":"hachure","roundness":null}"##,
    );

    assert!(use_all(&mut editor) < 100_000);
}

#[test]
fn a_huge_ellipse_is_drawn_in_a_bounded_number_of_steps() {
    // Unbounded, 1e10 would take over a hundred thousand steps.
    let mut editor = editor_with(r#"{"id":"a","type":"ellipse","width":1e10,"height":1e10}"#);

    assert!(use_all(&mut editor) < 300_000);
}

#[test]
fn extreme_ellipses_and_fills_finish() {
    let mut editor = editor_with(
        r##"{"id":"a","type":"ellipse","width":1e300,"height":1e300,"x":-1e300},
        {"id":"b","type":"ellipse","width":1e300,"height":1e300,"backgroundColor":"#f00","fillStyle":"zigzag"},
        {"id":"c","type":"rectangle","width":1e300,"height":1e300,"backgroundColor":"#f00","fillStyle":"cross-hatch"},
        {"id":"d","type":"diamond","width":1e300,"height":1e300,"roundness":{"type":2},"backgroundColor":"#f00"},
        {"id":"e","type":"rectangle","width":1e300,"height":1e300,"roundness":{"type":3},"strokeStyle":"dotted"},
        {"id":"f","type":"line","points":[[0,0],[1e300,1e300],[5,5]],"strokeStyle":"dashed","roughness":99},
        {"id":"g","type":"freedraw","points":[[0,0],[1e300,1e300],[5,5]],"pressures":[1,1,1],"strokeWidth":1e300}"##,
    );

    use_all(&mut editor);
}

/// The Drawing of the review probe: every kind of invalid value at once.
#[test]
fn the_probe_drawing_finishes() {
    let mut editor = editor_with(
        r#"
  {"id":"a","type":"rectangle","x":"10","y":null,"width":-5,"height":1e999,"angle":"x",
   "strokeWidth":-1,"roughness":99,"opacity":"50","seed":-3.5,"roundness":{"type":"x"},
   "boundElements":[{"id":"gone","type":"text"},7],"groupIds":"g","fillStyle":5},
  {"id":"b","type":"diamond","width":0,"height":0,"strokeStyle":["dashed"]},
  {"id":"c","type":"line","points":[],"strokeStyle":"dotted"},
  {"id":"d","type":"arrow","points":[[0,0],["x",1],[1e999,0]],
   "startBinding":{"elementId":"gone"},"endBinding":{"elementId":"a","fixedPoint":"no"},
   "startArrowhead":7,"endArrowhead":"unknown"},
  {"id":"e","type":"freedraw","points":[[1,1]],"pressures":[2,"x"]},
  {"id":"f","type":"freedraw","points":"none","pressures":null},
  {"id":"g","type":"text","text":null,"fontSize":0,"lineHeight":-1,"containerId":"gone"},
  {"id":"h","type":"text","text":"x","containerId":"a","fontFamily":"Virgil"},
  {"id":"i","type":"image","fileId":"missing","scale":[0,"x"],"crop":{"naturalWidth":0}},
  {"id":"j","type":"ellipse","width":1e300,"height":1e300,"x":-1e300}"#,
    );

    use_all(&mut editor);
}
