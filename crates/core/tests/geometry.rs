//! The geometry output format (the contract with clients, ADR-0007/0008),
//! tested through `Editor::geometry` and the `Geometry` accessors.
//!
//! Expected values are literals: the layout numbers are what the Swift client
//! hard-codes on its side, so they must not be derived from the Rust types.

use std::mem::{offset_of, size_of};

use workingtitle_core::{Editor, Geometry, Part, PartKind, Verb};

/// Splits one part's ops into `(verb, coordinates)` pairs.
fn ops(geometry: &Geometry, part: &Part) -> Vec<(Verb, Vec<f32>)> {
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

#[test]
fn the_tracer_scene_is_one_stroked_rectangle() {
    let geometry = Editor::new().geometry();

    let [part] = geometry.parts() else {
        panic!("expected exactly one part, got {}", geometry.parts().len());
    };
    assert_eq!(part.kind, PartKind::Stroke as u32);
    assert_eq!(
        part.color, 0x1e1e_1eff,
        "Excalidraw's default stroke #1e1e1e"
    );
    assert_eq!(part.width, 2.0);
    assert_eq!(part.opacity, 1.0);
    assert_eq!(part.version, 1);
}

#[test]
fn the_rectangle_outline_is_closed_and_has_rounded_corners() {
    let geometry = Editor::new().geometry();
    let ops = ops(&geometry, &geometry.parts()[0]);

    // Starts after the top-left corner radius (32) and runs clockwise.
    assert_eq!(ops[0], (Verb::Move, vec![82.0, 100.0]));
    assert_eq!(ops[1], (Verb::Line, vec![298.0, 100.0]));
    let cubics = ops.iter().filter(|(verb, _)| *verb == Verb::Cubic).count();
    assert_eq!(cubics, 4, "one cubic per corner");

    let (last_verb, last) = ops.last().unwrap();
    assert_eq!(*last_verb, Verb::Cubic);
    assert_eq!(last[4..], [82.0, 100.0], "ends where it started");
}

#[test]
fn the_rectangle_stays_inside_its_bounds() {
    let geometry = Editor::new().geometry();
    // x: 50..330, y: 100..280 in Scene coordinates; Bézier control points of a
    // rounded corner never leave the bounding box either.
    for point in geometry.coords().as_chunks::<2>().0 {
        assert!((50.0..=330.0).contains(&point[0]), "x = {}", point[0]);
        assert!((100.0..=280.0).contains(&point[1]), "y = {}", point[1]);
    }
}

#[test]
fn parts_follow_each_other_in_the_shared_buffers() {
    let mut geometry = Geometry::new();
    let style = workingtitle_core::StrokeStyle {
        color: 0xff00_00ff,
        width: 1.5,
        opacity: 0.5,
    };
    let mut first = geometry.stroke(7, 3, style);
    first.move_to(1.0, 2.0);
    first.line_to(3.0, 4.0);
    let mut second = geometry.stroke(8, 9, style);
    second.move_to(5.0, 6.0);
    second.cubic_to(1.0, 1.0, 2.0, 2.0, 3.0, 3.0);

    let [a, b] = geometry.parts() else {
        panic!("two parts")
    };
    assert_eq!((a.element, a.version), (7, 3));
    assert_eq!((a.verb_start, a.verb_count, a.coord_start), (0, 2, 0));
    assert_eq!((b.element, b.version), (8, 9));
    assert_eq!((b.verb_start, b.verb_count, b.coord_start), (2, 2, 4));
    assert_eq!(geometry.verbs(), [0, 1, 0, 2]);
    assert_eq!(geometry.coords().len(), 4 + 2 + 6);
}

#[test]
fn record_layout_is_the_c_layout_clients_read_directly() {
    assert_eq!(size_of::<Part>(), 36);
    assert_eq!(offset_of!(Part, element), 0);
    assert_eq!(offset_of!(Part, version), 4);
    assert_eq!(offset_of!(Part, kind), 8);
    assert_eq!(offset_of!(Part, color), 12);
    assert_eq!(offset_of!(Part, opacity), 16);
    assert_eq!(offset_of!(Part, width), 20);
    assert_eq!(offset_of!(Part, verb_start), 24);
    assert_eq!(offset_of!(Part, verb_count), 28);
    assert_eq!(offset_of!(Part, coord_start), 32);
}

#[test]
fn verb_and_kind_numbers_are_stable() {
    assert_eq!(Verb::Move as u8, 0);
    assert_eq!(Verb::Line as u8, 1);
    assert_eq!(Verb::Cubic as u8, 2);
    assert_eq!(Verb::from_byte(3), None);
    assert_eq!(PartKind::Stroke as u32, 0);
}
