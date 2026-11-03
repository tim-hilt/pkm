//! Loading and saving Drawings through the `Editor` (ADR-0003, ADR-0009).
//!
//! Expected values are literals: either Excalidraw's documented defaults
//! (`restore.ts` at the pinned commit) or fixture bytes written by
//! Excalidraw itself, never recomputed from the code under test.

use std::fs;
use std::path::PathBuf;

use workingtitle_core::json::{Value, parse};
use workingtitle_core::{Editor, Geometry, Verb};

fn fixture(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn editor_with(drawing: &str) -> Editor {
    let mut editor = Editor::new();
    editor
        .load(drawing.as_bytes())
        .unwrap_or_else(|e| panic!("drawing should load: {e}"));
    editor
}

fn saved(editor: &Editor) -> String {
    String::from_utf8(editor.save()).expect("saved Drawings are UTF-8")
}

/// A one-Element Drawing as another tool might write it: `element` is the
/// inside of the Element's JSON object.
fn drawing_with_element(element: &str) -> String {
    format!(r#"{{"type":"excalidraw","version":2,"elements":[{{{element}}}]}}"#)
}

/// The saved Element at `index`, parsed.
fn saved_element(editor: &Editor, index: usize) -> Value {
    let drawing = parse(&saved(editor)).expect("saved Drawings are valid JSON");
    drawing
        .get("elements")
        .and_then(Value::as_array)
        .expect("elements")[index]
        .clone()
}

fn member<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("missing member {key}"))
}

/// Compact JSON of `value[key]`, for comparing whole sub-trees as literals.
fn json_of(value: &Value, key: &str) -> String {
    member(value, key).to_json()
}

/// The path ops of a part as `(verb, operands)`.
fn ops(geometry: &Geometry, part: usize) -> Vec<(Verb, Vec<f32>)> {
    let part = geometry.parts()[part];
    let verbs = &geometry.verbs()[part.verb_start as usize..][..part.verb_count as usize];
    let mut coords = &geometry.coords()[part.coord_start as usize..];
    verbs
        .iter()
        .map(|&byte| {
            let verb = Verb::from_byte(byte).expect("known verb");
            let (taken, rest) = coords.split_at(verb.coord_count());
            coords = rest;
            (verb, taken.to_vec())
        })
        .collect()
}

// --- Byte stability -------------------------------------------------------

#[test]
fn saving_what_excalidraw_wrote_gives_the_same_bytes() {
    // The golden fixtures are complete `.excalidraw` files written by
    // `JSON.stringify(scene, null, 2)`; excalidraw.com's own files end after
    // the closing brace, the generator adds a newline.
    for name in ["shapes", "fills", "lines-arrows", "freedraw"] {
        let original = fixture(&format!("golden/{name}.excalidraw"));
        let editor = editor_with(&original);
        assert_eq!(saved(&editor), original.trim_end_matches('\n'), "{name}");
    }
}

#[test]
fn saving_a_minified_file_writes_excalidraws_indented_layout() {
    let minified = fixture("drawing.min.excalidraw");
    let pretty = fixture("drawing.pretty.excalidraw");
    // Same content, different layout: the layout is not preserved.
    assert_eq!(saved(&editor_with(&minified)), saved(&editor_with(&pretty)));
}

// --- Unknown content survives ---------------------------------------------

#[test]
fn unknown_element_types_and_the_files_map_survive_verbatim() {
    let original = fixture("drawing.pretty.excalidraw");
    let editor = editor_with(&original);

    let before = parse(&original).unwrap();
    let after = parse(&saved(&editor)).unwrap();

    let widget = |drawing: &Value| {
        drawing
            .get("elements")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .find(|e| e.get("type").and_then(Value::as_str) == Some("futuristic-widget"))
            .expect("the unknown Element is still there")
            .to_json()
    };
    assert_eq!(widget(&after), widget(&before), "no defaults, no reorder");
    assert_eq!(
        widget(&after),
        r#"{"id":"X1","type":"futuristic-widget","x":1,"y":2,"customProp":{"nested":[1,2.5,null,true,false,"s"],"empty":{},"none":[]}}"#
    );
    assert_eq!(json_of(&after, "files"), json_of(&before, "files"));
    assert_eq!(
        json_of(&after, "files"),
        r#"{"abc123":{"mimeType":"image/png","id":"abc123","dataURL":"data:image/png;base64,iVBORw0KGgo=","created":1730380000000,"lastRetrieved":1730380000000}}"#
    );
}

#[test]
fn unknown_properties_survive_on_known_elements_and_keep_their_place() {
    let editor = editor_with(&drawing_with_element(
        r#""id":"a","type":"rectangle","x":1,"zeta":{"deep":[1,2]},"y":2,"alpha":"kept","width":3,"height":4"#,
    ));

    let rectangle = saved_element(&editor, 0).to_json();
    assert!(
        rectangle.starts_with(
            r#"{"id":"a","type":"rectangle","x":1,"zeta":{"deep":[1,2]},"y":2,"alpha":"kept","width":3,"height":4"#
        ),
        "{rectangle}"
    );
}

#[test]
fn other_top_level_members_survive() {
    let editor = editor_with(
        r##"{"type":"excalidraw","version":2,"source":"somewhere","elements":[],"appState":{"viewBackgroundColor":"#fafafa","zzz":1},"files":{},"extra":[1]}"##,
    );

    assert_eq!(
        saved(&editor),
        concat!(
            "{\n",
            "  \"type\": \"excalidraw\",\n",
            "  \"version\": 2,\n",
            "  \"source\": \"somewhere\",\n",
            "  \"elements\": [],\n",
            "  \"appState\": {\n",
            "    \"viewBackgroundColor\": \"#fafafa\",\n",
            "    \"zzz\": 1\n",
            "  },\n",
            "  \"files\": {},\n",
            "  \"extra\": [\n",
            "    1\n",
            "  ]\n",
            "}"
        )
    );
}

// --- Excalidraw's defaults for missing fields -----------------------------

#[test]
fn a_bare_shape_gets_excalidraws_defaults() {
    for kind in ["rectangle", "diamond", "ellipse"] {
        let editor = editor_with(&drawing_with_element(&format!(
            r#""id":"a","type":"{kind}","x":10,"y":20,"width":30,"height":40"#
        )));
        let element = saved_element(&editor, 0);

        let defaults = [
            ("version", "1"),
            ("versionNonce", "0"),
            ("index", "null"),
            ("isDeleted", "false"),
            ("fillStyle", r#""solid""#),
            ("strokeWidth", "2"),
            ("strokeStyle", r#""solid""#),
            ("roughness", "1"),
            ("opacity", "100"),
            ("angle", "0"),
            ("strokeColor", r##""#1e1e1e""##),
            ("backgroundColor", r#""transparent""#),
            ("seed", "1"),
            ("groupIds", "[]"),
            ("frameId", "null"),
            ("roundness", "null"),
            ("boundElements", "[]"),
            ("created", "null"),
            ("link", "null"),
            ("locked", "false"),
        ];
        for (key, expected) in defaults {
            assert_eq!(json_of(&element, key), expected, "{kind}.{key}");
        }
        // What was there stays as it was.
        assert_eq!(json_of(&element, "x"), "10");
        assert_eq!(json_of(&element, "height"), "40");
    }
}

#[test]
fn fields_that_are_present_are_never_replaced() {
    let editor = editor_with(&drawing_with_element(
        r##""id":"a","type":"rectangle","x":1,"y":2,"width":3,"height":4,"roughness":0,"opacity":0,"strokeWidth":4,"seed":0,"boundElements":null,"strokeColor":"#ff0000","version":7"##,
    ));
    let element = saved_element(&editor, 0);

    assert_eq!(json_of(&element, "roughness"), "0");
    assert_eq!(json_of(&element, "opacity"), "0");
    assert_eq!(json_of(&element, "strokeWidth"), "4");
    assert_eq!(json_of(&element, "seed"), "0");
    assert_eq!(json_of(&element, "boundElements"), "null");
    assert_eq!(json_of(&element, "strokeColor"), r##""#ff0000""##);
    assert_eq!(json_of(&element, "version"), "7");
}

#[test]
fn a_bare_text_gets_text_defaults() {
    let editor = editor_with(&drawing_with_element(
        r#""id":"t","type":"text","x":0,"y":0,"width":50,"height":25,"text":"Hi""#,
    ));
    let element = saved_element(&editor, 0);

    assert_eq!(json_of(&element, "fontSize"), "20");
    assert_eq!(json_of(&element, "fontFamily"), "5", "Excalifont");
    assert_eq!(json_of(&element, "textAlign"), r#""left""#);
    assert_eq!(json_of(&element, "verticalAlign"), r#""top""#);
    assert_eq!(json_of(&element, "containerId"), "null");
    assert_eq!(json_of(&element, "originalText"), r#""Hi""#, "the text");
    assert_eq!(json_of(&element, "autoResize"), "true");
    // 25 px tall, one line of 20 px text: the file's own line height.
    assert_eq!(json_of(&element, "lineHeight"), "1.25");
    assert_eq!(json_of(&element, "labelPosition"), "null");
    assert_eq!(json_of(&element, "baseFontSize"), "null");
}

#[test]
fn a_text_without_size_or_text_gets_empty_text_and_the_fonts_line_height() {
    let editor = editor_with(&drawing_with_element(
        r#""id":"t","type":"text","fontFamily":2"#,
    ));
    let element = saved_element(&editor, 0);

    assert_eq!(json_of(&element, "text"), r#""""#);
    assert_eq!(json_of(&element, "originalText"), r#""""#);
    assert_eq!(json_of(&element, "lineHeight"), "1.15", "Helvetica");
}

#[test]
fn a_text_without_line_height_detects_it_from_its_height() {
    // Two lines of 10 px text in a 30 px box: 30 / 2 / 10 = 1.5.
    let editor = editor_with(&drawing_with_element(
        r#""id":"t","type":"text","height":30,"fontSize":10,"text":"a\r\nb""#,
    ));

    assert_eq!(json_of(&saved_element(&editor, 0), "lineHeight"), "1.5");
}

#[test]
fn linear_elements_default_to_a_straight_two_point_path() {
    let line = editor_with(&drawing_with_element(
        r#""id":"l","type":"line","x":0,"y":0,"width":30,"height":40"#,
    ));
    let element = saved_element(&line, 0);
    assert_eq!(json_of(&element, "points"), "[[0,0],[30,40]]");
    assert_eq!(json_of(&element, "startBinding"), "null");
    assert_eq!(json_of(&element, "endBinding"), "null");
    assert_eq!(json_of(&element, "startArrowhead"), "null");
    assert_eq!(json_of(&element, "endArrowhead"), "null");
    assert_eq!(json_of(&element, "polygon"), "false");

    let arrow = editor_with(&drawing_with_element(
        r#""id":"r","type":"arrow","points":[[0,0],[5,5],[9,1]]"#,
    ));
    let element = saved_element(&arrow, 0);
    assert_eq!(json_of(&element, "points"), "[[0,0],[5,5],[9,1]]");
    assert_eq!(json_of(&element, "endArrowhead"), r#""arrow""#);
    assert_eq!(json_of(&element, "startArrowhead"), "null");
    assert!(element.get("polygon").is_none(), "only lines have polygon");
}

#[test]
fn freedraw_and_image_defaults() {
    let freedraw = editor_with(&drawing_with_element(r#""id":"f","type":"freedraw""#));
    let element = saved_element(&freedraw, 0);
    assert_eq!(json_of(&element, "points"), "[]");
    assert_eq!(json_of(&element, "pressures"), "[]");
    assert_eq!(
        json_of(&element, "strokeOptions"),
        r#"{"variability":"variable","streamline":0.5}"#
    );

    let image = editor_with(&drawing_with_element(r#""id":"i","type":"image""#));
    let element = saved_element(&image, 0);
    assert_eq!(json_of(&element, "status"), r#""pending""#);
    assert_eq!(json_of(&element, "scale"), "[1,1]");
    assert_eq!(json_of(&element, "crop"), "null");
}

// --- Deleted Elements -----------------------------------------------------

const WITH_DELETED: &str = r#"{"type":"excalidraw","version":2,"elements":[
    {"id":"a","type":"rectangle","x":0,"y":0,"width":10,"height":10},
    {"id":"b","type":"ellipse","x":20,"y":0,"width":10,"height":10,"isDeleted":true},
    {"id":"c","type":"diamond","x":40,"y":0,"width":10,"height":10}
]}"#;

#[test]
fn deleted_elements_stay_in_the_file_but_not_in_the_scene() {
    let editor = editor_with(WITH_DELETED);

    assert_eq!(editor.element_count(), 2);
    let drawing = parse(&saved(&editor)).unwrap();
    let ids: Vec<_> = member(&drawing, "elements")
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.get("id").and_then(Value::as_str).unwrap())
        .collect();
    assert_eq!(ids, ["a", "b", "c"]);
    assert_eq!(
        json_of(&saved_element(&editor, 1), "isDeleted"),
        "true",
        "still marked as deleted"
    );
}

#[test]
fn deleted_elements_are_not_drawn() {
    let geometry = editor_with(WITH_DELETED).geometry();

    let drawn: Vec<u32> = geometry.parts().iter().map(|part| part.element).collect();
    assert_eq!(drawn, [0, 2], "a and c, not b");
}

// --- Bounds on the Canvas -------------------------------------------------

#[test]
fn elements_without_a_generator_yet_are_drawn_as_a_box_around_their_bounds() {
    let editor = editor_with(&drawing_with_element(
        r#""id":"a","type":"text","x":10,"y":20,"width":30,"height":40"#,
    ));
    let geometry = editor.geometry();

    let [part] = geometry.parts() else {
        panic!("one part per Element")
    };
    assert_eq!(part.opacity, 1.0);
    assert_eq!(part.color, 0x1e1e_1eff);
    assert_eq!(part.width, 2.0);
    assert_eq!(
        ops(&geometry, 0),
        [
            (Verb::Move, vec![10.0, 20.0]),
            (Verb::Line, vec![40.0, 20.0]),
            (Verb::Line, vec![40.0, 60.0]),
            (Verb::Line, vec![10.0, 60.0]),
            (Verb::Line, vec![10.0, 20.0]),
        ]
    );
}

#[test]
fn the_box_of_a_rotated_element_turns_about_its_centre() {
    // Centre (25, 40); a quarter turn clockwise (y points down) takes the
    // corner (10, 20) to (45, 25), the next one (40, 20) to (45, 55), ...
    let editor = editor_with(&drawing_with_element(&format!(
        r#""id":"a","type":"text","x":10,"y":20,"width":30,"height":40,"angle":{}"#,
        std::f64::consts::FRAC_PI_2
    )));
    let geometry = editor.geometry();

    let corners: Vec<_> = ops(&geometry, 0)
        .into_iter()
        .map(|(_, p)| [p[0].round(), p[1].round()])
        .collect();
    assert_eq!(
        corners,
        [
            [45.0, 25.0],
            [45.0, 55.0],
            [5.0, 55.0],
            [5.0, 25.0],
            [45.0, 25.0]
        ]
    );
}

#[test]
fn every_supported_element_type_is_drawn_and_unknown_ones_are_not() {
    let kinds = [
        "rectangle",
        "diamond",
        "ellipse",
        "line",
        "arrow",
        "freedraw",
        "text",
        "image",
    ];
    let elements: Vec<_> = kinds
        .iter()
        .map(|kind| format!(r#"{{"id":"{kind}","type":"{kind}","width":5,"height":5}}"#))
        .chain([r#"{"id":"u","type":"frame","width":5,"height":5}"#.to_owned()])
        .collect();
    let editor = editor_with(&format!(
        r#"{{"type":"excalidraw","version":2,"elements":[{}]}}"#,
        elements.join(",")
    ));

    // Arrows have more than one part, so count the Elements drawn.
    let drawn: std::collections::BTreeSet<_> = editor
        .geometry()
        .parts()
        .iter()
        .map(|part| part.element)
        .collect();
    assert_eq!(drawn.len(), kinds.len());
    assert_eq!(editor.element_count(), kinds.len() as u64);
}

#[test]
fn parts_carry_the_element_handle_and_version_and_opacity() {
    let editor = editor_with(
        r#"{"type":"excalidraw","version":2,"elements":[
            {"id":"a","type":"rectangle","width":1,"height":1,"version":3},
            {"id":"b","type":"rectangle","width":1,"height":1,"version":9,"opacity":50}
        ]}"#,
    );
    let geometry = editor.geometry();

    let [a, b] = geometry.parts() else {
        panic!("two parts")
    };
    assert_eq!((a.element, a.version, a.opacity), (0, 3, 1.0));
    assert_eq!((b.element, b.version, b.opacity), (1, 9, 0.5));
}

// --- Rejecting what is not a Drawing --------------------------------------

#[test]
fn input_that_is_not_a_drawing_is_rejected_and_leaves_the_scene_alone() {
    let mut editor = editor_with(&drawing_with_element(
        r#""id":"a","type":"rectangle","width":1,"height":1"#,
    ));
    let before = saved(&editor);

    let rejected: [&[u8]; 8] = [
        b"",
        b"not json",
        b"{\"type\":\"excalidraw\",\"elements\":[",
        &[0xff, 0xfe, 0x00],
        b"[]",
        br#"{"type":"excalidraw/clipboard","elements":[]}"#,
        br#"{"type":"excalidraw","elements":{}}"#,
        br#"{"type":"excalidraw","elements":[1]}"#,
    ];
    for bytes in rejected {
        assert!(
            editor.load(bytes).is_err(),
            "{:?}",
            String::from_utf8_lossy(bytes)
        );
    }

    assert_eq!(saved(&editor), before);
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn error_messages_say_what_is_wrong() {
    let mut editor = Editor::new();
    let message = |bytes: &[u8], editor: &mut Editor| editor.load(bytes).unwrap_err().to_string();

    assert!(message(b"{", &mut editor).contains("line 1, column 2"));
    assert!(message(b"[]", &mut editor).contains("not an Excalidraw Drawing"));
    assert!(message(&[0xff], &mut editor).contains("UTF-8"));
}

#[test]
fn a_drawing_without_elements_is_an_empty_scene() {
    let editor = editor_with(r#"{"type":"excalidraw","version":2}"#);

    assert_eq!(editor.element_count(), 0);
    assert_eq!(json_of(&parse(&saved(&editor)).unwrap(), "elements"), "[]");
}

#[test]
fn loading_replaces_the_scene() {
    let mut editor = editor_with(WITH_DELETED);
    editor
        .load(drawing_with_element(r#""id":"z","type":"ellipse","width":1,"height":1"#).as_bytes())
        .unwrap();

    assert_eq!(editor.element_count(), 1);
    assert_eq!(editor.geometry().parts().len(), 1);
}

#[test]
fn a_new_editor_saves_an_empty_drawing_that_loads_again() {
    let bytes = Editor::new().save();

    let mut again = Editor::new();
    again.load(&bytes).expect("an empty Drawing loads");
    assert_eq!(again.element_count(), 0);
    assert_eq!(again.save(), bytes);
    let drawing = parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(json_of(&drawing, "type"), r#""excalidraw""#);
    assert_eq!(json_of(&drawing, "version"), "2");
    assert_eq!(json_of(&drawing, "elements"), "[]");
}
