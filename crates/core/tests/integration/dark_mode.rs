//! Dark mode is display-only (ADR-0010): with it on the `Editor` reports
//! Excalidraw's dark-mode colours, and nothing that is saved or exported
//! changes. Expected colours come from Excalidraw's `applyDarkModeFilter`
//! (`packages/common/src/colors.ts`), run on each input.

use excalidraw_native_core::{Editor, ExportScope, Property, TextMeasurer, Tool, dark_mode_color};
use std::sync::Arc;

use crate::common::{FixedWidth, editor_with, replay, tap};

#[test]
fn colours_are_transformed_like_excalidraws_dark_mode_filter() {
    let table = [
        // Palette colours.
        ("#e03131", "#ff8383"),
        ("#2f9e44", "#39994b"),
        ("#1971c2", "#56a2e8"),
        ("#f08c00", "#b86200"),
        ("#ffc9c9", "#5a2c2c"),
        ("#b2f2bb", "#043b0c"),
        ("#a5d8ff", "#154162"),
        ("#ffec99", "#362600"),
        ("#ced4da", "#33383d"),
        ("#868e96", "#6e757c"),
        ("#6965db", "#8986eb"),
        // Black, white and Excalidraw's default ink.
        ("#000000", "#ededed"),
        ("#ffffff", "#121212"),
        ("#1e1e1e", "#d3d3d3"),
        // Short hex, alpha, CSS names.
        ("#f80", "#c35d00"),
        ("#ff000080", "#ff909080"),
        ("#ffffff00", "#12121200"),
        ("red", "#ff9090"),
        ("rebeccapurple", "#cea2f9"),
    ];
    for (light, dark) in table {
        assert_eq!(dark_mode_color(light), dark, "{light}");
    }
}

#[test]
fn transparent_stays_transparent() {
    assert_eq!(dark_mode_color("transparent"), "transparent");
}

fn shapes() -> Editor {
    editor_with(&[
        r##"{"id":"r","type":"rectangle","x":0,"y":0,"width":40,"height":20,
            "strokeColor":"#e03131","backgroundColor":"#ffc9c9","fillStyle":"solid",
            "strokeWidth":2,"roundness":null,"index":null,"seed":1,"version":1,"versionNonce":1}"##,
        r##"{"id":"a","type":"arrow","x":0,"y":100,"width":80,"height":0,
            "points":[[0,0],[80,0]],"endArrowhead":"triangle_outline","strokeColor":"#000000",
            "backgroundColor":"transparent","strokeWidth":2,"roundness":null,"index":null,
            "seed":5,"version":1,"versionNonce":3}"##,
    ])
}

fn colours(editor: &Editor) -> Vec<u32> {
    editor.geometry().parts().iter().map(|p| p.color).collect()
}

#[test]
fn dark_mode_is_off_by_default_and_changes_nothing_while_off() {
    let mut editor = shapes();
    assert!(!editor.dark_mode());
    let light = colours(&editor);
    editor.set_dark_mode(true);
    editor.set_dark_mode(false);
    assert_eq!(colours(&editor), light);
}

#[test]
fn geometry_reports_display_colours_in_dark_mode() {
    let mut editor = shapes();
    let light = colours(&editor);
    editor.set_dark_mode(true);
    let dark = colours(&editor);
    assert_eq!(dark.len(), light.len());
    assert!(dark.contains(&0xff83_83ff), "stroke #e03131");
    assert!(dark.contains(&0x5a2c_2cff), "fill #ffc9c9");
    assert!(dark.contains(&0xeded_edff), "stroke #000000");
}

#[test]
fn outline_arrowheads_are_filled_with_the_display_canvas_colour() {
    let mut editor = shapes();
    editor.set_dark_mode(true);
    let dark = colours(&editor);
    // The Canvas is white: #121212 in dark mode, and only the arrowhead's
    // fill uses it.
    assert_eq!(dark.iter().filter(|&&c| c == 0x1212_12ff).count(), 1);
}

#[test]
fn text_is_reported_in_display_colours() {
    let mut editor = editor_with(&[
        r##"{"id":"t","type":"text","x":0,"y":0,"width":20,"height":25,"text":"hi",
            "originalText":"hi","fontSize":20,"fontFamily":1,"strokeColor":"#000000",
            "textAlign":"left","verticalAlign":"top","lineHeight":1.25,
            "index":null,"seed":1,"version":1,"versionNonce":1}"##,
    ]);
    editor.set_dark_mode(true);
    assert_eq!(editor.geometry().text_lines()[0].color, 0xeded_edff);
}

#[test]
fn the_canvas_colour_is_the_background_as_displayed() {
    let mut editor = Editor::new();
    assert_eq!(editor.canvas_color(), 0xffff_ffff);
    editor.set_dark_mode(true);
    assert_eq!(editor.canvas_color(), 0x1212_12ff);

    let custom = br##"{"type":"excalidraw","version":2,"elements":[],
        "appState":{"viewBackgroundColor":"#ffc9c9"}}"##;
    editor.load(custom).unwrap();
    assert_eq!(editor.canvas_color(), 0x5a2c_2cff);
    editor.set_dark_mode(false);
    assert_eq!(editor.canvas_color(), 0xffc9_c9ff);
}

#[test]
fn a_transparent_background_stays_transparent() {
    let mut editor = Editor::new();
    editor
        .load(
            br##"{"type":"excalidraw","version":2,"elements":[],
            "appState":{"viewBackgroundColor":"transparent"}}"##,
        )
        .unwrap();
    editor.set_dark_mode(true);
    assert_eq!(editor.canvas_color() & 0xff, 0);
}

#[test]
fn toggling_regenerates_once_and_then_serves_from_the_cache() {
    let mut editor = shapes();
    editor.geometry();
    let before = editor.elements_generated();
    editor.set_dark_mode(true);
    editor.geometry();
    assert_eq!(editor.elements_generated(), before + 2);
    editor.geometry();
    assert_eq!(editor.elements_generated(), before + 2);
    // Setting what is already set costs nothing.
    editor.set_dark_mode(true);
    editor.geometry();
    assert_eq!(editor.elements_generated(), before + 2);
}

#[test]
fn toggling_keeps_selection_undo_history_and_text_editing() {
    let mut editor = shapes();
    editor.set_text_measurer(Arc::new(FixedWidth) as Arc<dyn TextMeasurer>);
    editor.set_tool(Tool::Selection);
    tap(&mut editor, "finger", (20.0, 10.0), 0.0);
    assert_eq!(editor.selection(), ["r"]);
    editor.set_property(Property::StrokeColor("#1971c2".into()));
    assert!(editor.can_undo());

    editor.set_dark_mode(true);
    assert_eq!(editor.selection(), ["r"]);
    assert!(editor.can_undo());
    editor.undo();
    assert!(editor.can_redo());
}

#[test]
fn export_properties_and_saved_scene_do_not_depend_on_dark_mode() {
    let mut editor = shapes();
    editor.set_tool(Tool::Selection);
    tap(&mut editor, "finger", (20.0, 10.0), 0.0);

    let snapshot = |editor: &Editor| {
        let export = editor.export(ExportScope::Scene).unwrap();
        let g = &export.geometry;
        (
            g.parts().to_vec(),
            g.coords().to_vec(),
            (export.x, export.y, export.width, export.height),
            editor.properties(),
            editor.save(),
        )
    };
    let light = snapshot(&editor);
    editor.set_dark_mode(true);
    let dark = snapshot(&editor);
    assert_eq!(light.0, dark.0);
    assert_eq!(light.1, dark.1);
    assert_eq!(light.2, dark.2);
    assert_eq!(light.3, dark.3);
    assert_eq!(light.4, dark.4);
    // And the display afterwards is still dark.
    assert!(colours(&editor).contains(&0x5a2c_2cff));
}

#[test]
fn toggling_gives_the_geometry_another_generation_so_clients_drop_their_caches() {
    let mut editor = shapes();
    let before = editor.geometry().generation();
    editor.set_dark_mode(true);
    let dark = editor.geometry().generation();
    assert_ne!(before, dark);
    editor.set_dark_mode(true);
    assert_eq!(editor.geometry().generation(), dark);
}

// --- The overlay and the Text view ------------------------------------------------

fn overlay_colours(editor: &Editor) -> Vec<u32> {
    editor.overlay().parts().iter().map(|p| p.color).collect()
}

fn selected_rectangle() -> Editor {
    let mut editor = shapes();
    editor.set_tool(Tool::Selection);
    tap(&mut editor, "finger", (20.0, 10.0), 0.0);
    editor
}

#[test]
fn the_overlay_uses_excalidraws_dark_selection_colours() {
    // Light: `--color-selection` #6965db and white handles. Dark: #b4b0ff
    // (theme.scss) and the white handles' transform, #121212.
    let mut editor = selected_rectangle();
    let light = overlay_colours(&editor);
    assert!(light.contains(&0x6965_dbff) && light.contains(&0xffff_ffff));
    let revision = editor.overlay_revision();
    editor.set_dark_mode(true);
    assert_ne!(editor.overlay_revision(), revision, "clients redraw it");
    let dark = overlay_colours(&editor);
    assert_eq!(dark.len(), light.len());
    assert!(dark.contains(&0xb4b0_ffff) && dark.contains(&0x1212_12ff));
    assert!(!dark.contains(&0x6965_dbff) && !dark.contains(&0xffff_ffff));
}

#[test]
fn group_frames_are_light_on_the_dark_canvas() {
    let mut editor = editor_with(&[
        r##"{"id":"a","type":"rectangle","x":0,"y":0,"width":40,"height":20,
            "groupIds":["g"],"roundness":null,"index":null,"seed":1,"version":1,"versionNonce":1}"##,
        r##"{"id":"b","type":"rectangle","x":100,"y":0,"width":40,"height":20,
            "groupIds":["g"],"roundness":null,"index":null,"seed":2,"version":1,"versionNonce":1}"##,
    ]);
    editor.set_tool(Tool::Selection);
    tap(&mut editor, "finger", (20.0, 10.0), 0.0);
    assert!(overlay_colours(&editor).contains(&0x0000_00ff), "black");
    editor.set_dark_mode(true);
    // Excalidraw's `groupSelectionColor`, `getThemedColor("#000")`.
    assert!(overlay_colours(&editor).contains(&0xeded_edff));
}

#[test]
fn the_box_select_rectangle_follows_the_dark_theme() {
    let mut editor = shapes();
    editor.set_tool(Tool::Selection);
    editor.set_dark_mode(true);
    replay(
        &mut editor,
        "0 down 1 finger 200 200\n0.1 move 1 finger 250 250\n0.2 move 1 finger 300 300",
    );
    let colours = overlay_colours(&editor);
    // The fill `rgba(0, 0, 200, 0.04)` through `getThemedColor`, and the
    // stroke in the dark selection colour.
    assert!(colours.contains(&0xb4b0_ffff));
    let fill = dark_mode_color("#0000c8");
    let fill = u32::from_str_radix(&fill[1..], 16).unwrap() << 8 | 0xff;
    assert!(colours.contains(&fill), "{colours:x?}");
}

#[test]
fn the_binding_highlight_is_excalidraws_dark_one() {
    let mut editor = shapes();
    editor.set_dark_mode(true);
    editor.set_tool(Tool::Arrow);
    replay(
        &mut editor,
        "0 down 1 finger 300 300\n0.1 move 1 finger 100 50\n0.2 move 1 finger 20 10",
    );
    let colours = overlay_colours(&editor);
    // `BINDING_HIGHLIGHT_RGB[DARK]` is 104, 182, 240.
    assert!(colours.contains(&0x68b6_f0ff), "{colours:x?}");
}

#[test]
fn the_text_view_shows_the_display_colour_and_the_scene_keeps_its_own() {
    let mut editor = editor_with(&[
        r##"{"id":"t","type":"text","x":0,"y":0,"width":20,"height":25,"text":"hi",
            "originalText":"hi","fontSize":20,"fontFamily":1,"strokeColor":"#e03131",
            "textAlign":"left","verticalAlign":"top","lineHeight":1.25,
            "index":null,"seed":1,"version":1,"versionNonce":1}"##,
    ]);
    editor.set_text_measurer(Arc::new(FixedWidth) as Arc<dyn TextMeasurer>);
    editor.set_tool(Tool::Text);
    tap(&mut editor, "finger", (5.0, 5.0), 0.0);
    assert_eq!(editor.text_editor().unwrap().color, 0xe031_31ff);
    let revision = editor.revision();
    editor.set_dark_mode(true);
    // Switching while the view is open keeps the edit.
    let open = editor.text_editor().expect("still editing");
    assert_eq!(open.color, 0xff83_83ff);
    assert_eq!(open.text, "hi");
    assert_eq!(editor.revision(), revision);
    editor.set_editing_text("hi there");
    editor.commit_text();
    let saved = editor.save();
    let saved = String::from_utf8_lossy(&saved);
    assert!(saved.contains("#e03131") && !saved.contains("#ff8383"));
}

#[test]
fn the_erase_preview_shows_faded_elements_in_their_dark_colours() {
    let mut editor = shapes();
    editor.set_dark_mode(true);
    editor.set_tool(Tool::Eraser);
    replay(
        &mut editor,
        "0.0 down 1 mouse 20 -10\n0.1 move 1 mouse 20 30",
    );
    let parts = editor.geometry().parts().to_vec();
    let faded: Vec<_> = parts.iter().filter(|p| p.element == 0).collect();
    assert!(!faded.is_empty());
    for part in faded {
        assert_eq!(part.opacity, 0.2);
        assert!([0xff83_83ff, 0x5a2c_2cff].contains(&part.color));
    }
}
