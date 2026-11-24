//! Dimensions: the size of what is being resized or drawn, shown beside it while the
//! pointer is down (the "Show dimensions" setting, off by default).
//!
//! The output is made for one zoom like the overlay, and clients ask again
//! when [`Editor::overlay_revision`] changes: lines and ticks are drawn as
//! geometry, the labels by the client in the system font, centred on their
//! anchor and turned by their angle. Never part of the Scene, exports or the
//! saved file.

use crate::frame::Frame;
use crate::geometry::{Geometry, StrokeStyle};
use crate::overlay::{Colors, NO_ELEMENT};
use crate::path::Point;

/// How far in view points a dimension line keeps from the box.
const LINE_OFFSET: f64 = 20.0;
/// The length of the tick at each end of a dimension line in view points.
const TICK_LENGTH: f64 = 8.0;
/// How far in view points the anchor of the width label is below its line:
/// room for half the height of the text.
const LABEL_OFFSET: f64 = 12.0;
/// The room in view points between the height line and its label, and the
/// width of a character of the label (the system font at the size clients
/// use): the label is centred on its anchor, so it needs half its width.
const LABEL_GAP: f64 = 6.0;
const CHARACTER_WIDTH: f64 = 7.0;
const LINE_WIDTH: f64 = 1.0;
const OPACITY: f32 = 0.6;

/// What to draw for the Dimensions of a resize or drawn shape, in Scene coordinates.
#[derive(Debug)]
pub struct Dimensions {
    /// Dimension lines with end ticks, strokes in the overlay's format.
    pub geometry: Geometry,
    /// The numbers, to draw over the geometry.
    pub labels: Vec<DimensionLabel>,
    /// The colour of lines and numbers, `0xRRGGBBAA`, drawn at `opacity`.
    pub color: u32,
    pub opacity: f32,
}

/// A number of the Dimensions.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionLabel {
    /// Formatted with two decimals and `.` as separator, e.g. `"240.00"`.
    pub text: String,
    /// The Scene point the label is centred on.
    pub x: f64,
    pub y: f64,
    /// Clockwise in radians: that of the Element, so that labels read along
    /// its sides.
    pub angle: f64,
}

/// The Dimensions of a box of `frame` (where it is when not turned) turned
/// clockwise by `angle` about its middle: its width below it and height left
/// of it, along its own sides. Made for a view at `zoom`.
pub(crate) fn of_box(frame: Frame, angle: f64, zoom: f64, colors: Colors) -> Dimensions {
    let (width, height) = (frame.width(), frame.height());
    let (sin, cos) = angle.sin_cos();
    let (cx, cy) = frame.centre();
    // From a point relative to the middle of the box, not turned, to the Scene.
    let to_scene = |(x, y): Point| (cx + x * cos - y * sin, cy + x * sin + y * cos);

    let line = LINE_OFFSET / zoom;
    let tick = TICK_LENGTH / zoom / 2.0;
    let (half_w, half_h) = (width / 2.0, height / 2.0);
    // Both lines run along a side of the box; the ticks cross them.
    let below = [
        ((-half_w, half_h + line), (half_w, half_h + line)),
        (
            (-half_w, half_h + line - tick),
            (-half_w, half_h + line + tick),
        ),
        (
            (half_w, half_h + line - tick),
            (half_w, half_h + line + tick),
        ),
    ];
    let left = [
        ((-half_w - line, -half_h), (-half_w - line, half_h)),
        (
            (-half_w - line - tick, -half_h),
            (-half_w - line + tick, -half_h),
        ),
        (
            (-half_w - line - tick, half_h),
            (-half_w - line + tick, half_h),
        ),
    ];

    let mut geometry = Geometry::new();
    let mut stroke = geometry.stroke(
        NO_ELEMENT,
        0,
        StrokeStyle {
            color: colors.selection,
            width: (LINE_WIDTH / zoom) as f32,
            opacity: OPACITY,
            dash: None,
        },
    );
    for (from, to) in below.into_iter().chain(left) {
        let (from, to) = (to_scene(from), to_scene(to));
        stroke.move_to(from.0 as f32, from.1 as f32);
        stroke.line_to(to.0 as f32, to.1 as f32);
    }
    let width_text = format!("{width:.2}");
    let height_text = format!("{height:.2}");
    let height_reach = LINE_OFFSET + LABEL_GAP + height_text.len() as f64 * CHARACTER_WIDTH / 2.0;
    let labels = [
        (
            width_text,
            (0.0, half_h + (LINE_OFFSET + LABEL_OFFSET) / zoom),
        ),
        (height_text, (-half_w - height_reach / zoom, 0.0)),
    ]
    .map(|(text, anchor)| {
        let (x, y) = to_scene(anchor);
        DimensionLabel { text, x, y, angle }
    });
    Dimensions {
        geometry,
        labels: labels.to_vec(),
        color: colors.selection,
        opacity: OPACITY,
    }
}

#[cfg(test)]
mod tests {
    //! Dimensions while resizing, tested at the Editor seam with recorded
    //! pointer sequences (see `select.rs` for their format). Expected values
    //! are worked out by hand in the comments.

    use super::DimensionLabel;
    use crate::test_support::{editor_with, replay, tap};
    use crate::{Editor, Tool, Verb};

    /// A rectangle at (100, 100), 100 x 50, whose middle is (150, 125). Its
    /// south-east handle is centred on (206, 156), 4 for the selection frame
    /// and 2 of spacing beyond the corner.
    const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

    /// The same turned a quarter clockwise about (150, 125); its east handle
    /// is then 56 below the middle, at (150, 181); grabbed 2 closer.
    const TURNED: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "angle":1.5707963267948966,"strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

    fn selected(element: &str) -> Editor {
        let mut editor = editor_with(&[element]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        editor.set_show_dimensions(true);
        editor
    }

    /// A mouse takes hold at `from` and drags to `to`, staying down.
    fn drag_without_lifting(editor: &mut Editor, from: (f64, f64), to: (f64, f64)) {
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        replay(
            editor,
            &format!(
                "1.0 down 1 mouse {} {}\n1.1 move 1 mouse {} {}\n1.2 move 1 mouse {} {}",
                from.0, from.1, mid.0, mid.1, to.0, to.1
            ),
        );
    }

    fn lift(editor: &mut Editor, at: (f64, f64)) {
        replay(editor, &format!("1.3 up 1 mouse {} {}", at.0, at.1));
    }

    fn near(label: &DimensionLabel, (x, y): (f64, f64)) {
        assert!(
            (label.x - x).abs() < 1e-6 && (label.y - y).abs() < 1e-6,
            "{label:?} is not at ({x}, {y})"
        );
    }

    #[test]
    fn are_off_by_default() {
        assert!(!Editor::new().show_dimensions());
    }

    #[test]
    fn a_resize_shows_width_below_and_height_left_in_two_decimals() {
        let mut editor = selected(RECTANGLE);

        // The corner ends at (250, 180): 150 x 80, still at (100, 100), whose
        // middle is (175, 140). Lines are 20 out. The width label is 12
        // further; the height label's 5 characters take 17.5 either side of
        // its middle, with 6 to spare: 43.5 from the box.
        drag_without_lifting(&mut editor, (204.0, 154.0), (254.0, 184.0));

        let dimensions = editor.dimensions().expect("shown while resizing");
        let [width, height] = &dimensions.labels[..] else {
            panic!("{:?}", dimensions.labels)
        };
        assert_eq!(width.text, "150.00");
        near(width, (175.0, 140.0 + 40.0 + 32.0));
        assert_eq!(height.text, "80.00");
        near(height, (175.0 - 75.0 - 43.5, 140.0));
        assert_eq!((width.angle, height.angle), (0.0, 0.0));
    }

    #[test]
    fn the_lines_have_end_ticks() {
        let mut editor = selected(RECTANGLE);
        drag_without_lifting(&mut editor, (204.0, 154.0), (254.0, 184.0));

        let geometry = editor.dimensions().unwrap().geometry;
        let lines = geometry
            .verbs()
            .iter()
            .filter(|&&v| v == Verb::Line as u8)
            .count();
        // A line and two ticks for each of width and height.
        assert_eq!(lines, 6);
        // The width line runs from x 100 to 250, 20 below the box at y 180.
        assert_eq!(&geometry.coords()[..4], &[100.0, 200.0, 250.0, 200.0]);
    }

    #[test]
    fn are_gone_without_a_resize_or_with_the_setting_off() {
        let mut editor = selected(RECTANGLE);
        assert!(editor.dimensions().is_none(), "selected only");

        drag_without_lifting(&mut editor, (204.0, 154.0), (254.0, 184.0));
        assert!(editor.dimensions().is_some());
        lift(&mut editor, (254.0, 184.0));
        assert!(editor.dimensions().is_none(), "pointer lifted");

        editor.set_show_dimensions(false);
        drag_without_lifting(&mut editor, (254.0, 184.0), (300.0, 200.0));
        assert!(editor.dimensions().is_none(), "setting off");
    }

    #[test]
    fn follow_the_rotated_sides_and_carry_the_angle_of_the_element() {
        let mut editor = selected(TURNED);

        // Pulling the east handle 30 along the element's own x grows the
        // width to 130 with the west side, at (150, 75), staying put. The
        // middle is then (150, 140); width's anchor is 57 beyond the
        // element's bottom side, which now faces west.
        drag_without_lifting(&mut editor, (150.0, 179.0), (150.0, 209.0));

        let dimensions = editor.dimensions().unwrap();
        let [width, height] = &dimensions.labels[..] else {
            panic!("{:?}", dimensions.labels)
        };
        assert_eq!(
            (width.text.as_str(), height.text.as_str()),
            ("130.00", "50.00")
        );
        near(width, (150.0 - 25.0 - 32.0, 140.0));
        near(height, (150.0, 140.0 - 65.0 - 43.5));
        assert_eq!(width.angle, std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn keep_their_screen_distance_at_any_zoom() {
        let mut editor = selected(RECTANGLE);
        editor.set_viewport((0.0, 0.0), 2.0);

        // View points are twice the Scene's: the handle, 3 units out, is at
        // (406, 306) and the box ends 150 x 80 as before.
        drag_without_lifting(&mut editor, (406.0, 306.0), (506.0, 366.0));

        let revision = editor.overlay_revision();
        let dimensions = editor.dimensions().unwrap();
        // 32 view points beyond the box's bottom side are 16 units.
        near(&dimensions.labels[0], (175.0, 140.0 + 40.0 + 16.0));
        editor.set_viewport((0.0, 0.0), 1.0);
        assert_ne!(editor.overlay_revision(), revision);
    }

    #[test]
    fn use_the_selection_colour_dimmed_and_the_dark_one_in_dark_mode() {
        let mut editor = selected(RECTANGLE);
        drag_without_lifting(&mut editor, (204.0, 154.0), (254.0, 184.0));
        let light = editor.dimensions().unwrap().geometry.parts()[0];
        editor.set_dark_mode(true);
        let dark = editor.dimensions().unwrap().geometry.parts()[0];

        assert_eq!(light.color, 0x6965_dbff);
        assert_eq!(editor.dimensions().unwrap().color, 0xb4b0_ffff);
        assert_eq!(dark.color, 0xb4b0_ffff);
        assert!(light.opacity < 1.0);
    }

    #[test]
    fn are_not_part_of_the_scene_or_the_saved_file() {
        let mut editor = selected(RECTANGLE);
        let before = editor.geometry().parts().len();
        drag_without_lifting(&mut editor, (204.0, 154.0), (254.0, 184.0));

        assert_eq!(editor.geometry().parts().len(), before);
        let saved = String::from_utf8(editor.save()).unwrap();
        assert!(!saved.contains("150.00") && !saved.contains("imension"));
    }

    #[test]
    fn are_not_shown_for_rotating_moving_or_tapping() {
        let mut editor = selected(RECTANGLE);
        // The rotation handle is 22 above the top: (150, 100 - 4 - 2 - 16).
        drag_without_lifting(&mut editor, (150.0, 78.0), (180.0, 78.0));
        assert!(editor.dimensions().is_none(), "rotating");
        lift(&mut editor, (180.0, 78.0));

        drag_without_lifting(&mut editor, (150.0, 125.0), (170.0, 135.0));
        assert!(editor.dimensions().is_none(), "moving");
        lift(&mut editor, (170.0, 135.0));

        tap(&mut editor, "mouse", (170.0, 135.0), 5.0);
        assert!(editor.dimensions().is_none(), "tapping");
    }

    const OTHER: &str = r##"{"id":"b","type":"rectangle","x":300,"y":100,"width":50,"height":50,
    "version":3,"versionNonce":7}"##;

    #[test]
    fn several_elements_resized_together_have_one_pair_for_their_box() {
        let mut editor = editor_with(&[RECTANGLE, OTHER]);
        editor.set_show_dimensions(true);
        replay(
            &mut editor,
            "0.0 down 1 mouse 50 50\n0.1 move 1 mouse 360 160\n0.2 up 1 mouse 360 160",
        );
        assert_eq!(editor.selection().len(), 2);
        // The box is (100, 100)-(350, 150), 250 x 50; its east handle is
        // 6 beyond the middle of its east side. Pulling it 50 right makes
        // it 300 wide.
        drag_without_lifting(&mut editor, (356.0, 125.0), (406.0, 125.0));

        let dimensions = editor.dimensions().expect("shown while resizing");
        let texts: Vec<_> = dimensions.labels.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["300.00", "50.00"]);
    }

    #[test]
    fn a_group_resized_has_one_pair_for_its_box() {
        let grouped = |id, x| {
            format!(
                r##"{{"id":"{id}","type":"rectangle","x":{x},"y":100,"width":50,"height":50,
                "groupIds":["g"],"version":3,"versionNonce":7}}"##
            )
        };
        let mut editor = editor_with(&[&grouped("a", 100), &grouped("b", 200)]);
        editor.set_show_dimensions(true);
        tap(&mut editor, "mouse", (100.0, 125.0), 0.0);
        assert_eq!(editor.selection().len(), 2);
        // The box is (100, 100)-(250, 150), 150 x 50; pull its east handle
        // 30 right. A Group keeps the ratio of its sides: 180 x 60.
        drag_without_lifting(&mut editor, (256.0, 125.0), (286.0, 125.0));

        let dimensions = editor.dimensions().expect("shown while resizing");
        let texts: Vec<_> = dimensions.labels.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["180.00", "60.00"]);
    }

    /// With the given Tool chosen, a mouse draws from (100, 100) towards
    /// (160, 130), staying down.
    fn drawing(tool: Tool) -> Editor {
        let mut editor = Editor::new();
        editor.set_tool(tool);
        editor.set_show_dimensions(true);
        replay(
            &mut editor,
            "1.0 down 1 mouse 100 100\n1.1 move 1 mouse 130 115\n1.2 move 1 mouse 160 130",
        );
        editor
    }

    #[test]
    fn drawing_a_shape_follows_the_pointer_from_zero_size() {
        for tool in [Tool::Rectangle, Tool::Diamond, Tool::Ellipse] {
            let mut editor = Editor::new();
            editor.set_tool(tool);
            editor.set_show_dimensions(true);
            replay(&mut editor, "1.0 down 1 mouse 100 100");
            let at_start = editor.dimensions().expect("shown");
            let texts: Vec<_> = at_start.labels.iter().map(|l| l.text.as_str()).collect();
            assert_eq!(texts, ["0.00", "0.00"], "{tool:?}");

            replay(&mut editor, "1.1 move 1 mouse 160 130");
            let revision = editor.overlay_revision();
            replay(&mut editor, "1.2 move 1 mouse 170 140");
            assert_ne!(editor.overlay_revision(), revision, "{tool:?}");
            let texts: Vec<_> = editor
                .dimensions()
                .unwrap()
                .labels
                .iter()
                .map(|l| l.text.clone())
                .collect();
            assert_eq!(texts, ["70.00", "40.00"], "{tool:?}");
        }
    }

    #[test]
    fn a_drawn_shape_has_dimensions_next_to_it_until_the_pointer_lifts() {
        let mut editor = drawing(Tool::Rectangle);
        // 60 x 30 from (100, 100): the width label is 20 + 12 below its
        // bottom side at y 130, in the middle at x 130.
        let dimensions = editor.dimensions().unwrap();
        near(&dimensions.labels[0], (130.0, 130.0 + 32.0));
        assert_eq!(dimensions.labels[0].text, "60.00");

        let before = editor.overlay_revision();
        lift(&mut editor, (160.0, 130.0));
        assert!(editor.dimensions().is_none());
        assert_ne!(editor.overlay_revision(), before, "so clients redraw");
    }

    #[test]
    fn drawing_shows_nothing_with_the_setting_off() {
        let mut editor = drawing(Tool::Rectangle);
        lift(&mut editor, (160.0, 130.0));
        editor.set_show_dimensions(false);
        replay(
            &mut editor,
            "2.0 down 1 mouse 100 100\n2.1 move 1 mouse 150 150",
        );
        assert!(editor.dimensions().is_none());
    }
}
