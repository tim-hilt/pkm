//! Zoom controls (ADR-0006), tested at the Editor seam: zoom in, zoom out,
//! reset to 100 % and zoom to fit, matching Excalidraw's zoom actions (step of
//! 0.1, limits 0.1..=30, fit never zooms in past 100 %). Expected viewports
//! are worked out by hand in the comments.

use pkm_drawing::{Editor, MAX_ZOOM, MIN_ZOOM, MagnifyEvent};

const EPSILON: f64 = 1e-9;

fn editor_with_view(width: f64, height: f64) -> Editor {
    let mut editor = Editor::new();
    editor.set_view_size(width, height);
    editor
}

fn editor_showing(elements: &str, width: f64, height: f64) -> Editor {
    let mut editor = editor_with_view(width, height);
    editor
        .load(format!(r#"{{"type":"excalidraw","elements":[{elements}]}}"#).as_bytes())
        .unwrap();
    editor
}

fn rectangle(x: f64, y: f64, width: f64, height: f64) -> String {
    format!(
        r#"{{"id":"r{x}-{y}","type":"rectangle","x":{x},"y":{y},"width":{width},"height":{height}}}"#
    )
}

fn zoom(editor: &Editor) -> f64 {
    editor.viewport().zoom()
}

#[track_caller]
fn assert_viewport(editor: &Editor, origin: (f64, f64), zoom: f64) {
    let viewport = editor.viewport();
    let actual = (viewport.origin().0, viewport.origin().1, viewport.zoom());
    let close = |a: f64, b: f64| (a - b).abs() < EPSILON;
    assert!(
        close(actual.0, origin.0) && close(actual.1, origin.1) && close(actual.2, zoom),
        "{actual:?} != {:?}",
        (origin.0, origin.1, zoom)
    );
}

// --- Zoom in / out / reset ----------------------------------------------

#[test]
fn zooming_in_adds_a_tenth_each_time() {
    let mut editor = Editor::new();

    editor.zoom_in();
    assert!((zoom(&editor) - 1.1).abs() < EPSILON);
    editor.zoom_in();
    // Steps add up, they do not compound: 1.2, not 1.21.
    assert!((zoom(&editor) - 1.2).abs() < EPSILON);
}

#[test]
fn zooming_out_subtracts_a_tenth_each_time() {
    let mut editor = Editor::new();

    editor.zoom_out();
    assert!((zoom(&editor) - 0.9).abs() < EPSILON);
    editor.zoom_out();
    assert!((zoom(&editor) - 0.8).abs() < EPSILON);
}

#[test]
fn zoom_steps_come_out_round_after_many_repetitions() {
    let mut editor = Editor::new();

    for _ in 0..7 {
        editor.zoom_in();
    }
    for _ in 0..7 {
        editor.zoom_out();
    }

    // Excalidraw rounds to six digits so that stepping never drifts.
    assert_eq!(zoom(&editor), 1.0);
}

#[test]
fn zooming_out_stops_at_the_minimum() {
    let mut editor = Editor::new();

    for _ in 0..20 {
        editor.zoom_out();
    }

    assert_eq!(zoom(&editor), MIN_ZOOM);
    assert_eq!(MIN_ZOOM, 0.1);
}

#[test]
fn zooming_in_stops_at_the_maximum() {
    let mut editor = Editor::new();

    for _ in 0..400 {
        editor.zoom_in();
    }

    assert_eq!(zoom(&editor), MAX_ZOOM);
    assert_eq!(MAX_ZOOM, 30.0);
}

#[test]
fn zoom_steps_keep_the_centre_of_the_view_in_place() {
    let mut editor = editor_with_view(400.0, 200.0);

    editor.zoom_in();

    // The Scene point (200, 100) in the middle of the view stays there:
    // origin = 200 - 200 / 1.1 and 100 - 100 / 1.1.
    assert_viewport(&editor, (200.0 - 200.0 / 1.1, 100.0 - 100.0 / 1.1), 1.1);

    let mut editor = editor_with_view(400.0, 200.0);
    editor.zoom_out();
    assert_viewport(&editor, (200.0 - 200.0 / 0.9, 100.0 - 100.0 / 0.9), 0.9);
}

#[test]
fn zooming_at_a_limit_leaves_the_viewport_alone() {
    let mut editor = editor_with_view(400.0, 200.0);
    for _ in 0..20 {
        editor.zoom_out();
    }
    let at_minimum = editor.viewport();

    editor.zoom_out();

    assert_eq!(editor.viewport(), at_minimum);
}

#[test]
fn reset_returns_to_100_percent_around_the_centre_of_the_view() {
    let mut editor = editor_with_view(400.0, 200.0);
    // Zoom 2 around the top-left corner: the view's centre shows the Scene
    // point (100, 50).
    editor.magnify(MagnifyEvent {
        x: 0.0,
        y: 0.0,
        magnification: 1.0,
    });

    editor.reset_zoom();

    // At zoom 1 the centre (200, 100) shows (100, 50) again: origin (-100, -50).
    assert_viewport(&editor, (-100.0, -50.0), 1.0);
}

// --- Zoom to fit ---------------------------------------------------------

#[test]
fn fit_frames_all_elements_centred_in_the_view() {
    // Together the rectangles cover x 100..600 and y 100..400: 500 x 300.
    let elements = [
        rectangle(100.0, 100.0, 200.0, 100.0),
        rectangle(500.0, 300.0, 100.0, 100.0),
    ];
    let mut editor = editor_showing(&elements.join(","), 250.0, 300.0);

    editor.zoom_to_fit();

    // The width limits: 250 / 500 = 0.5. The content centre (350, 250) goes
    // to the view centre (125, 150): origin = (350 - 125 / 0.5, 250 - 150 / 0.5).
    assert_viewport(&editor, (100.0, -50.0), 0.5);
}

#[test]
fn fit_is_limited_by_the_height_when_that_is_tighter() {
    let mut editor = editor_showing(&rectangle(0.0, 0.0, 100.0, 400.0), 1000.0, 200.0);

    editor.zoom_to_fit();

    // 200 / 400 = 0.5; centre (50, 200) at the view centre (500, 100).
    assert_viewport(&editor, (50.0 - 1000.0, 200.0 - 200.0), 0.5);
}

#[test]
fn fit_ignores_deleted_elements() {
    let deleted = r#"{"id":"gone","type":"ellipse","x":5000,"y":5000,"width":10,"height":10,"isDeleted":true}"#;
    let elements = format!("{},{deleted}", rectangle(0.0, 0.0, 200.0, 100.0));
    let mut editor = editor_showing(&elements, 100.0, 100.0);

    editor.zoom_to_fit();

    // Only the rectangle counts: 100 / 200 = 0.5, centre (100, 50) at (50, 50).
    assert_viewport(&editor, (0.0, 50.0 - 50.0 / 0.5), 0.5);
}

#[test]
fn fit_does_not_zoom_in_past_100_percent() {
    let mut editor = editor_showing(&rectangle(0.0, 0.0, 50.0, 50.0), 400.0, 200.0);

    editor.zoom_to_fit();

    // Small content is centred at 100 %: centre (25, 25) at (200, 100).
    assert_viewport(&editor, (25.0 - 200.0, 25.0 - 100.0), 1.0);
}

#[test]
fn fit_zooms_out_to_100_percent_from_a_closer_zoom() {
    let mut editor = editor_showing(&rectangle(0.0, 0.0, 50.0, 50.0), 400.0, 200.0);
    for _ in 0..10 {
        editor.zoom_in();
    }

    editor.zoom_to_fit();

    assert_viewport(&editor, (-175.0, -75.0), 1.0);
}

#[test]
fn fit_uses_the_extent_of_a_rotated_element() {
    // A 100 x 20 rectangle turned a quarter circle about its centre (50, 10)
    // covers x 40..60 and y -40..60: 20 x 100.
    let element = r#"{"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":20,"angle":1.5707963267948966}"#;
    let mut editor = editor_showing(element, 50.0, 50.0);

    editor.zoom_to_fit();

    // The height limits: 50 / 100 = 0.5. The centre (50, 10) goes to (25, 25).
    assert_viewport(&editor, (50.0 - 25.0 / 0.5, 10.0 - 25.0 / 0.5), 0.5);
}

#[test]
fn fit_cannot_zoom_out_beyond_the_minimum() {
    let mut editor = editor_showing(&rectangle(0.0, 0.0, 10000.0, 10.0), 100.0, 100.0);

    editor.zoom_to_fit();

    // 100 / 10000 would be 0.01; the minimum is 0.1. Centre (5000, 5) at (50, 50).
    assert_viewport(&editor, (5000.0 - 50.0 / 0.1, 5.0 - 50.0 / 0.1), 0.1);
}

#[test]
fn fit_of_an_empty_scene_leaves_the_viewport_alone() {
    let mut editor = editor_with_view(400.0, 200.0);
    editor.zoom_in();
    let before = editor.viewport();

    editor.zoom_to_fit();

    assert_eq!(editor.viewport(), before);
}

#[test]
fn fit_of_a_scene_with_only_deleted_elements_leaves_the_viewport_alone() {
    let deleted =
        r#"{"id":"gone","type":"ellipse","x":0,"y":0,"width":10,"height":10,"isDeleted":true}"#;
    let mut editor = editor_showing(deleted, 400.0, 200.0);

    editor.zoom_to_fit();

    assert_eq!(editor.viewport(), Editor::new().viewport());
}

#[test]
fn fit_of_a_single_point_sized_element_is_well_defined() {
    let mut editor = editor_showing(&rectangle(30.0, 40.0, 0.0, 0.0), 100.0, 100.0);

    editor.zoom_to_fit();

    assert_viewport(&editor, (30.0 - 50.0, 40.0 - 50.0), 1.0);
}

#[test]
fn fit_before_the_view_has_a_size_leaves_the_viewport_alone() {
    let mut editor = Editor::new();
    editor
        .load(br#"{"type":"excalidraw","elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":500,"height":500}]}"#)
        .unwrap();

    editor.zoom_to_fit();

    assert_eq!(editor.viewport(), Editor::new().viewport());
}

#[test]
fn zooming_does_not_touch_the_scenes_geometry() {
    let mut editor = editor_showing(&rectangle(10.0, 20.0, 30.0, 40.0), 400.0, 200.0);
    let before = editor.geometry();

    editor.zoom_in();
    editor.zoom_to_fit();
    editor.reset_zoom();
    let after = editor.geometry();

    assert_eq!(before.coords(), after.coords());
    assert_eq!(before.parts(), after.parts());
}

// --- Restoring a viewport ------------------------------------------------

#[test]
fn a_viewport_that_was_set_is_the_viewport_shown() {
    let mut editor = Editor::new();

    editor.set_viewport((120.0, -40.5), 2.5);

    assert_viewport(&editor, (120.0, -40.5), 2.5);
}

#[test]
fn loading_a_drawing_leaves_a_viewport_to_be_set_afterwards() {
    let mut editor = editor_showing(&rectangle(0.0, 0.0, 10.0, 10.0), 400.0, 200.0);
    let bytes = editor.save();
    editor.set_viewport((5.0, 6.0), 3.0);

    editor.load(&bytes).unwrap();
    editor.set_viewport((7.0, 8.0), 0.5);

    assert_viewport(&editor, (7.0, 8.0), 0.5);
}

#[test]
fn a_set_zoom_is_kept_within_the_limits() {
    let mut editor = Editor::new();

    editor.set_viewport((0.0, 0.0), 1000.0);
    assert_eq!(zoom(&editor), MAX_ZOOM);
    editor.set_viewport((0.0, 0.0), 0.0);
    assert_eq!(zoom(&editor), MIN_ZOOM);
}

#[test]
fn a_viewport_that_is_not_a_number_is_ignored() {
    let mut editor = Editor::new();
    editor.set_viewport((10.0, 20.0), 2.0);

    editor.set_viewport((f64::NAN, 0.0), 1.0);
    editor.set_viewport((0.0, f64::INFINITY), 1.0);
    editor.set_viewport((0.0, 0.0), f64::NAN);

    assert_viewport(&editor, (10.0, 20.0), 2.0);
}

#[test]
fn setting_a_viewport_does_not_touch_the_scene() {
    let mut editor = editor_showing(&rectangle(10.0, 20.0, 30.0, 40.0), 400.0, 200.0);
    let revision = editor.revision();
    let saved = editor.save();

    editor.set_viewport((300.0, 300.0), 4.0);

    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.save(), saved);
}

#[test]
fn setting_a_zoom_updates_the_overlay_like_any_zoom_change() {
    let mut editor = Editor::new();
    let before = editor.overlay_revision();

    editor.set_viewport((0.0, 0.0), 2.0);

    assert_ne!(editor.overlay_revision(), before);
}
