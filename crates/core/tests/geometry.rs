//! The geometry output format (the contract with clients, ADR-0007/0008),
//! tested through the `Geometry` accessors. What the Editor puts into
//! geometry is tested with the Scene (`scene.rs`).
//!
//! Expected values are literals: the layout numbers are what the Swift client
//! hard-codes on its side, so they must not be derived from the Rust types.

use std::mem::{offset_of, size_of};

use workingtitle_core::{Dash, FillRule, FillStyle, Geometry, Part, PartKind, StrokeStyle, Verb};

#[test]
fn parts_follow_each_other_in_the_shared_buffers() {
    let mut geometry = Geometry::new();
    let style = StrokeStyle {
        color: 0xff00_00ff,
        width: 1.5,
        opacity: 0.5,
        dash: None,
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
    assert_eq!(size_of::<Part>(), 48);
    assert_eq!(offset_of!(Part, element), 0);
    assert_eq!(offset_of!(Part, version), 4);
    assert_eq!(offset_of!(Part, kind), 8);
    assert_eq!(offset_of!(Part, color), 12);
    assert_eq!(offset_of!(Part, opacity), 16);
    assert_eq!(offset_of!(Part, width), 20);
    assert_eq!(offset_of!(Part, verb_start), 24);
    assert_eq!(offset_of!(Part, verb_count), 28);
    assert_eq!(offset_of!(Part, coord_start), 32);
    assert_eq!(offset_of!(Part, dash_on), 36);
    assert_eq!(offset_of!(Part, dash_off), 40);
    assert_eq!(offset_of!(Part, fill_rule), 44);
}

#[test]
fn verb_and_kind_numbers_are_stable() {
    assert_eq!(Verb::Move as u8, 0);
    assert_eq!(Verb::Line as u8, 1);
    assert_eq!(Verb::Cubic as u8, 2);
    assert_eq!(Verb::from_byte(3), None);
    assert_eq!(PartKind::Stroke as u32, 0);
    assert_eq!(PartKind::Fill as u32, 1);
    assert_eq!(FillRule::NonZero as u32, 0);
    assert_eq!(FillRule::EvenOdd as u32, 1);
}

#[test]
fn a_part_records_its_dash_pattern_and_solid_parts_record_zeros() {
    let mut geometry = Geometry::new();
    let solid = StrokeStyle {
        color: 0,
        width: 2.0,
        opacity: 1.0,
        dash: None,
    };
    let dashed = StrokeStyle {
        dash: Some(Dash { on: 8.0, off: 10.0 }),
        ..solid
    };
    geometry.stroke(0, 1, solid).move_to(0.0, 0.0);
    geometry.stroke(1, 1, dashed).move_to(0.0, 0.0);

    let [a, b] = geometry.parts() else {
        panic!("two parts")
    };
    assert_eq!((a.dash_on, a.dash_off), (0.0, 0.0));
    assert_eq!((b.dash_on, b.dash_off), (8.0, 10.0));
}

#[test]
fn a_fill_part_records_its_rule_and_has_no_line_width() {
    let mut geometry = Geometry::new();
    let style = FillStyle {
        color: 0x00ff_00ff,
        opacity: 0.25,
        rule: FillRule::EvenOdd,
    };
    let mut fill = geometry.fill(4, 2, style);
    fill.move_to(0.0, 0.0);
    fill.line_to(10.0, 0.0);
    fill.line_to(10.0, 10.0);

    let [part] = geometry.parts() else {
        panic!("one part")
    };
    assert_eq!(part.kind, PartKind::Fill as u32);
    assert_eq!((part.color, part.opacity), (0x00ff_00ff, 0.25));
    assert_eq!(part.fill_rule, 1);
    assert_eq!((part.width, part.dash_on, part.dash_off), (0.0, 0.0, 0.0));
    assert_eq!(part.verb_count, 3);
}
