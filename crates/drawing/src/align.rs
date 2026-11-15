//! Aligning and distributing the Selection, as Excalidraw's `actionAlign` and
//! `actionDistribute` do (`align.ts`, `distribute.ts` and
//! `getSelectedElementsByGroup` in `groups.ts` at the pinned commit).
//!
//! Both move *units* by the boxes they are shown in (see [`Frame::shown`]):
//! each selected group, or Element, with the labels of its containers. With
//! one group selected, its next groups down and its own single Elements are
//! the units, so that its parts line up. Arrows bound to what moves follow it
//! (see [`crate::binding`]).

use crate::binding::Attachments;
use crate::frame::Frame;
use crate::history::Step;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use pkm_base::random::Entropy;

/// Which edges or middles of the units are lined up with the Selection's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    /// The middles across (Excalidraw's `alignHorizontallyCentered`).
    CenterHorizontally,
    Right,
    Top,
    /// The middles down (`alignVerticallyCentered`).
    CenterVertically,
    Bottom,
}

/// Along which axis the gaps between the units are made equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution {
    Horizontally,
    Vertically,
}

/// Moves of less than this are none: what is aligned already stays.
const EPSILON: f64 = 1e-9;

/// A group or Element that moves as a whole.
struct Unit {
    /// Its Elements, labels included.
    ids: Vec<String>,
    frame: Frame,
}

/// The units of the Selection, in z-order (see the module).
fn units(scene: &Scene, selection: &Selection, editing: Option<&str>) -> Vec<Unit> {
    if selection.is_empty() {
        return Vec::new();
    }
    let selected: Vec<Element> = scene
        .elements()
        .filter(|e| !e.is_deleted() && !e.is_locked() && selection.contains(e.id()))
        .filter(|e| !is_label(e))
        .collect();
    // The group selecting each Element selects, as `group::unit_group`.
    let outer = |e: &Element<'_>| -> Option<String> {
        e.group_ids()
            .take_while(|group| Some(*group) != editing)
            .last()
            .map(str::to_owned)
    };
    let first = selected.first().and_then(outer);
    let one_group = first.is_some() && selected.iter().all(|e| outer(e) == first);
    let key = |e: &Element<'_>| -> String {
        let Some(group) = outer(e) else {
            return e.id().to_owned();
        };
        if !one_group {
            return group;
        }
        // The group right inside the selected one, if any.
        let ids: Vec<&str> = e.group_ids().collect();
        match ids.iter().position(|g| *g == group) {
            Some(at) if at > 0 => ids[at - 1].to_owned(),
            _ => e.id().to_owned(),
        }
    };

    let mut keys: Vec<String> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    for element in &selected {
        let key = key(element);
        let at = keys.iter().position(|k| *k == key).unwrap_or_else(|| {
            keys.push(key);
            units.push(Unit {
                ids: Vec::new(),
                frame: Frame::shown(element),
            });
            units.len() - 1
        });
        let unit = &mut units[at];
        unit.ids.push(element.id().to_owned());
        unit.frame = unit.frame.union(Frame::shown(element));
    }
    // The labels go with their containers.
    for label in scene.elements().filter(|e| !e.is_deleted() && is_label(e)) {
        let container = label.container_id().unwrap_or_default();
        if let Some(unit) = units
            .iter_mut()
            .find(|u| u.ids.iter().any(|id| id == container))
        {
            unit.ids.push(label.id().to_owned());
            unit.frame = unit.frame.union(Frame::shown(&label));
        }
    }
    units
}

fn is_label(element: &Element) -> bool {
    element.kind() == ElementType::Text && element.container_id().is_some()
}

/// How many units the Selection has: aligning takes two, distributing three.
pub(crate) fn unit_count(scene: &Scene, selection: &Selection, editing: Option<&str>) -> usize {
    units(scene, selection, editing).len()
}

/// Lines up the units of `selection` with the edge or middle of all of them,
/// as part of `step`. Returns whether anything moved.
pub(crate) fn align(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
    editing: Option<&str>,
    how: Alignment,
) -> bool {
    let units = units(scene, selection, editing);
    if units.len() < 2 {
        return false;
    }
    let bounds = all(&units);
    let middle = |f: &Frame| f.centre();
    let moves: Vec<(f64, f64)> = units
        .iter()
        .map(|u| {
            let f = &u.frame;
            match how {
                Alignment::Left => (bounds.min.0 - f.min.0, 0.0),
                Alignment::CenterHorizontally => (middle(&bounds).0 - middle(f).0, 0.0),
                Alignment::Right => (bounds.max.0 - f.max.0, 0.0),
                Alignment::Top => (0.0, bounds.min.1 - f.min.1),
                Alignment::CenterVertically => (0.0, middle(&bounds).1 - middle(f).1),
                Alignment::Bottom => (0.0, bounds.max.1 - f.max.1),
            }
        })
        .collect();
    shift(scene, entropy, step, &units, &moves)
}

/// Spaces the units of `selection` evenly between the outermost ones, as part
/// of `step`: with equal gaps, or, where they would overlap, with equally
/// spaced middles. Returns whether anything moved.
pub(crate) fn distribute(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
    editing: Option<&str>,
    how: Distribution,
) -> bool {
    let mut units = units(scene, selection, editing);
    if units.len() < 3 {
        return false;
    }
    // Start, end and middle of a frame along the axis.
    let span = |f: &Frame| match how {
        Distribution::Horizontally => (f.min.0, f.max.0),
        Distribution::Vertically => (f.min.1, f.max.1),
    };
    let mid = |f: &Frame| {
        let (start, end) = span(f);
        (start + end) / 2.0
    };
    let (start, end) = span(&all(&units));
    units.sort_by(|a, b| mid(&a.frame).total_cmp(&mid(&b.frame)));
    let sizes: f64 = units
        .iter()
        .map(|u| span(&u.frame).1 - span(&u.frame).0)
        .sum();
    let gap = (end - start - sizes) / (units.len() - 1) as f64;

    let along: Vec<f64> = if gap < 0.0 {
        // Overlapping: the middles are spaced between those of the units
        // that hold the ends, which stay.
        let first = units.iter().position(|u| span(&u.frame).0 == start);
        let last = units.iter().position(|u| span(&u.frame).1 == end);
        let (Some(first), Some(last)) = (first, last) else {
            return false;
        };
        let (from, to) = (mid(&units[first].frame), mid(&units[last].frame));
        let step = (to - from) / (units.len() - 1) as f64;
        let mut at = from;
        units
            .iter()
            .enumerate()
            .map(|(index, u)| {
                if index == first || index == last {
                    return 0.0;
                }
                at += step;
                at - mid(&u.frame)
            })
            .collect()
    } else {
        let mut at = start;
        units
            .iter()
            .map(|u| {
                let (s, e) = span(&u.frame);
                let by = at - s;
                at += e - s + gap;
                by
            })
            .collect()
    };
    let moves: Vec<(f64, f64)> = along
        .into_iter()
        .map(|by| match how {
            Distribution::Horizontally => (by, 0.0),
            Distribution::Vertically => (0.0, by),
        })
        .collect();
    shift(scene, entropy, step, &units, &moves)
}

/// The box around all the units.
fn all(units: &[Unit]) -> Frame {
    units
        .iter()
        .map(|u| u.frame)
        .reduce(Frame::union)
        .expect("at least one unit")
}

/// Moves each unit by its move, as part of `step`, and the arrows bound to
/// them along. Returns whether anything moved.
fn shift(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    units: &[Unit],
    moves: &[(f64, f64)],
) -> bool {
    let moving: Vec<(&Unit, (f64, f64))> = units
        .iter()
        .zip(moves.iter().copied())
        .filter(|(_, (dx, dy))| dx.abs() > EPSILON || dy.abs() > EPSILON)
        .collect();
    if moving.is_empty() {
        return false;
    }
    let ids: Vec<String> = moving.iter().flat_map(|(u, _)| u.ids.clone()).collect();
    for id in &ids {
        step.touch_members(scene, id, &["x", "y"]);
    }
    let attachments = Attachments::begin(step, scene, ids);
    for (unit, (dx, dy)) in moving {
        for id in &unit.ids {
            let Some(element) = scene.element_by_id(id) else {
                continue;
            };
            let (handle, at) = (element.handle(), (element.x() + dx, element.y() + dy));
            scene.set_position(handle, at, entropy.integer());
        }
    }
    attachments.update(scene, entropy, step);
    true
}

#[cfg(test)]
mod tests {
    //! Aligning and distributing the Selection (ADR-0006), tested at the Editor
    //! seam: Select All in, commands, the saved Drawing out.
    //!
    //! Expected positions follow Excalidraw's `align.ts`, `distribute.ts` and
    //! `getSelectedElementsByGroup` (`groups.ts`) at the pinned commit.

    use crate::{Alignment, Distribution, Editor};
    use pkm_base::json::Value;

    use crate::test_support as common;
    use common::{editor_with, num, saved_elements};

    /// A rectangle `id` at `(x, y)`, `w` wide and `h` high, in `groups` (JSON
    /// array contents, innermost first).
    fn rect(id: &str, (x, y): (f64, f64), (w, h): (f64, f64), groups: &str) -> String {
        format!(
            r#"{{"id":"{id}","type":"rectangle","x":{x},"y":{y},"width":{w},"height":{h},
        "groupIds":[{groups}],"seed":1,"version":3,"versionNonce":7}}"#
        )
    }

    /// `a` (0, 0; 40 × 20), `b` (100, 50; 60 × 30) and `c` (200, 10; 20 × 40),
    /// all selected: the Selection spans 0 to 220 across and 0 to 80 down.
    fn three(groups: [&str; 3]) -> Editor {
        let [a, b, c] = groups;
        let mut editor = editor_with(&[
            &rect("a", (0.0, 0.0), (40.0, 20.0), a),
            &rect("b", (100.0, 50.0), (60.0, 30.0), b),
            &rect("c", (200.0, 10.0), (20.0, 40.0), c),
        ]);
        editor.select_all();
        editor
    }

    fn position(editor: &Editor, id: &str) -> (f64, f64) {
        let element = saved_elements(editor)
            .into_iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
            .unwrap_or_else(|| panic!("no element {id}"));
        (num(&element, "x"), num(&element, "y"))
    }

    fn positions(editor: &Editor) -> [(f64, f64); 3] {
        ["a", "b", "c"].map(|id| position(editor, id))
    }

    #[test]
    fn aligning_lines_up_the_edges_or_middles_of_the_selection() {
        let cases = [
            (Alignment::Left, [(0.0, 0.0), (0.0, 50.0), (0.0, 10.0)]),
            (
                Alignment::CenterHorizontally,
                [(90.0, 0.0), (80.0, 50.0), (100.0, 10.0)],
            ),
            (
                Alignment::Right,
                [(180.0, 0.0), (160.0, 50.0), (200.0, 10.0)],
            ),
            (Alignment::Top, [(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)]),
            (
                Alignment::CenterVertically,
                [(0.0, 30.0), (100.0, 25.0), (200.0, 20.0)],
            ),
            (
                Alignment::Bottom,
                [(0.0, 60.0), (100.0, 50.0), (200.0, 40.0)],
            ),
        ];
        for (how, expected) in cases {
            let mut editor = three(["", "", ""]);
            editor.align(how);
            assert_eq!(positions(&editor), expected, "{how:?}");
        }
    }

    #[test]
    fn distributing_makes_the_gaps_between_them_equal() {
        // 220 wide, 120 of it shapes: two gaps of 50.
        let mut editor = three(["", "", ""]);
        editor.distribute(Distribution::Horizontally);
        assert_eq!(
            positions(&editor),
            [(0.0, 0.0), (90.0, 50.0), (200.0, 10.0)]
        );

        // 80 high, 90 of it shapes: they overlap, so the middles are spaced
        // instead, between those of the topmost and the bottommost.
        let mut editor = three(["", "", ""]);
        editor.distribute(Distribution::Vertically);
        // a's middle is at 10, b's at 65; c's goes halfway between, to 37.5.
        assert_eq!(
            positions(&editor),
            [(0.0, 0.0), (100.0, 50.0), (200.0, 17.5)]
        );
    }

    #[test]
    fn a_selected_group_moves_as_a_whole() {
        let mut editor = three([r#""g""#, r#""g""#, ""]);
        editor.align(Alignment::Right);
        // The group (0 to 160) moves 60 to the right; c is there already.
        assert_eq!(
            positions(&editor),
            [(60.0, 0.0), (160.0, 50.0), (200.0, 10.0)]
        );
    }

    #[test]
    fn with_one_group_selected_its_parts_are_aligned() {
        let mut editor = three([r#""inner","outer""#, r#""inner","outer""#, r#""outer""#]);
        editor.align(Alignment::Top);
        // The inner group (top at 0) stays; c, on its own in the outer one, moves.
        assert_eq!(
            positions(&editor),
            [(0.0, 0.0), (100.0, 50.0), (200.0, 0.0)]
        );
    }

    #[test]
    fn a_label_moves_with_its_container() {
        let label = r#"{"id":"t","type":"text","x":110,"y":55,"width":40,"height":20,
        "text":"Hi","originalText":"Hi","fontSize":20,"containerId":"b","seed":2}"#;
        let b = rect("b", (100.0, 50.0), (60.0, 30.0), "").replace(
            r#""groupIds""#,
            r#""boundElements":[{"type":"text","id":"t"}],"groupIds""#,
        );
        let mut editor = editor_with(&[&rect("a", (0.0, 0.0), (40.0, 20.0), ""), &b, label]);
        editor.select_all();
        let before = position(&editor, "t");
        editor.align(Alignment::Top);
        assert_eq!(position(&editor, "b"), (100.0, 0.0));
        assert_eq!(position(&editor, "t"), (before.0, before.1 - 50.0));
    }

    #[test]
    fn aligning_is_one_undo_step() {
        let mut editor = three(["", "", ""]);
        editor.align(Alignment::Left);
        editor.undo();
        assert_eq!(
            positions(&editor),
            [(0.0, 0.0), (100.0, 50.0), (200.0, 10.0)]
        );
        assert!(!editor.can_undo());
    }

    #[test]
    fn what_is_aligned_already_records_no_step() {
        let mut editor = three(["", "", ""]);
        editor.align(Alignment::Left);
        editor.align(Alignment::Left);
        editor.undo();
        assert!(!editor.can_undo());
    }

    #[test]
    fn aligning_takes_two_units_and_distributing_three() {
        let editor = three([r#""g""#, r#""g""#, ""]);
        assert!(editor.can_align());
        assert!(!editor.can_distribute());
        let sections = editor.properties().sections;
        assert!(sections.align && !sections.distribute);

        let editor = three(["", "", ""]);
        assert!(editor.can_distribute());
        assert!(editor.properties().sections.distribute);

        let mut editor = three([r#""g""#, r#""g""#, r#""g""#]);
        editor.distribute(Distribution::Horizontally);
        assert!(
            editor.can_distribute(),
            "one group: its Elements are the units"
        );
    }

    #[test]
    fn a_single_element_cannot_be_aligned() {
        let mut editor = editor_with(&[&rect("a", (0.0, 0.0), (40.0, 20.0), "")]);
        editor.select_all();
        assert!(!editor.can_align());
        assert!(!editor.properties().sections.align);
        editor.align(Alignment::Right);
        assert!(!editor.can_undo());
    }
}
