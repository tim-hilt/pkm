//! Editing the points of a line or arrow (ADR-0006).
//!
//! A line or arrow that is alone in the Selection shows [`PointHandles`]: one
//! on each point, and one in the middle of each segment that is long enough
//! on screen to have its own. As everywhere in the app they are made for the
//! pointer that uses them, so for a finger they are big. A drag that takes
//! hold of a point handle moves that point, with every other selected point
//! (see [`PointSelection`]), and one that takes hold of a middle adds a point
//! there and moves it. Both are a [`PointDrag`].
//!
//! Unlike on excalidraw.com no separate editing mode has to be entered by
//! double-clicking: a touch screen has no double-click to spare, and the
//! handles are only in the way of a line that is selected anyway. Elbow
//! arrows have no point handles, as the core does not route them.
//!
//! Dragging the first or last point of an arrow on its own binds it to the
//! shape it is put on and unbinds it elsewhere, and the bound ends of an arrow
//! whose points move are kept where their bindings put them (see
//! [`crate::binding`]).
//!
//! Points are stored relative to the Element's position, which is its first
//! point, and the Element turns about the middle of its box. The handles are
//! where the points are *shown*, so everything here works in the shown
//! positions and [`reshape`] turns them back into what is stored.

use crate::binding::{self, End};
use crate::frame::Frame;
use crate::history::Step;
use crate::input::PointerKind;
use crate::linear;
use crate::path::Point;
use crate::rough::Op;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use crate::transform_handles::{Grip, targets};
use pkm_base::random::Entropy;

/// How many handles of points a segment must be long on screen to have a
/// handle in its middle (Excalidraw's `POINT_HANDLE_SIZE * 4`).
const MIDDLE_FROM: f64 = 4.0;

/// What a pointer that went down on a handle holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grab {
    /// The point with this index.
    Point(usize),
    /// The middle of a segment: a point with this index would be added there.
    Middle(usize),
}

/// The handles of the line or arrow that is alone in the Selection, in a view
/// at some zoom, with where they are shown in the Scene.
#[derive(Debug)]
pub(crate) struct PointHandles {
    id: String,
    handle: u32,
    is_arrow: bool,
    /// Where the points are shown.
    points: Vec<Point>,
    /// For the segments with a handle in their middle: the index a point
    /// added there gets, and where the handle is shown.
    middles: Vec<(usize, Point)>,
}

impl PointHandles {
    /// The handles of the Selection, for a pointer of `kind` in a view at
    /// `zoom`; `None` unless it is a single line or arrow that has points to
    /// edit.
    pub fn of(scene: &Scene, selection: &Selection, kind: PointerKind, zoom: f64) -> Option<Self> {
        let mut targets = targets(scene, selection);
        let element = targets.next()?;
        if targets.next().is_some() || !element.kind().is_linear() || element.is_elbow_arrow() {
            return None;
        }
        let shown = Shown::of(&element);
        let points = shown_points(&element);

        let curve = linear::exact_curve(&element).map(|ops| cubic_middles(&ops));
        let too_short = MIDDLE_FROM * Grip::of(kind).visual;
        let middles = (1..points.len())
            .filter(|&i| distance(points[i - 1], points[i]) * zoom >= too_short)
            .map(|i| {
                let middle = match curve.as_ref().and_then(|middles| middles.get(i - 1)) {
                    Some(&(x, y)) => shown.at((x, y)),
                    None => midway(points[i - 1], points[i]),
                };
                (i, middle)
            })
            .collect();
        Some(Self {
            id: element.id().to_owned(),
            handle: element.handle(),
            is_arrow: element.kind() == ElementType::Arrow,
            points,
            middles,
        })
    }

    /// The `id` of the Element the handles belong to.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Where the points of the Element are shown.
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Where the handles in the middle of segments are shown.
    pub fn middles(&self) -> impl Iterator<Item = Point> + '_ {
        self.middles.iter().map(|&(_, at)| at)
    }

    /// The handle a pointer of `kind` that went down at the Scene point `at`
    /// takes hold of, if any: the nearest of those whose grip it is in, the
    /// handles of points before those of middles.
    pub fn grab(&self, at: Point, kind: PointerKind, zoom: f64) -> Option<Grab> {
        let reach = Grip::of(kind).hit / 2.0 / zoom;
        let nearest = |candidates: &mut dyn Iterator<Item = (usize, Point)>| {
            candidates
                .map(|(index, (x, y))| (index, (at.0 - x).abs(), (at.1 - y).abs()))
                .filter(|&(_, dx, dy)| dx <= reach && dy <= reach)
                .min_by(|a, b| a.1.hypot(a.2).total_cmp(&b.1.hypot(b.2)))
                .map(|(index, ..)| index)
        };
        nearest(&mut self.points.iter().copied().enumerate())
            .map(Grab::Point)
            .or_else(|| nearest(&mut self.middles.iter().copied()).map(Grab::Middle))
    }

    /// A drag by `pointer`, which went down at `origin` on `grab`, as the
    /// action of `step`. The grabbed point joins the selected `points` (with
    /// `shift`) or, unless it is one of them already, becomes the only one;
    /// the drag of a middle first adds its point and selects it alone. All
    /// selected points move. An end of an arrow that is dragged binds to
    /// shapes within `bind_reach` of it.
    #[allow(clippy::too_many_arguments)]
    pub fn drag(
        &self,
        mut step: Step,
        scene: &Scene,
        pointer: u64,
        origin: Point,
        points: &mut PointSelection,
        grab: Grab,
        shift: bool,
        bind_reach: f64,
    ) -> PointDrag {
        let points_before = points.clone();
        match grab {
            Grab::Point(index) if points.contains(index) => {}
            Grab::Point(index) if shift => points.add(&self.id, index),
            Grab::Point(index) | Grab::Middle(index) => points.select_only(&self.id, index),
        }
        let moving = points.indices().to_vec();
        step.touch(scene, &self.id);
        let mut start = self.points.clone();
        if let Grab::Middle(index) = grab
            && let Some(&(_, at)) = self.middles.iter().find(|&&(i, _)| i == index)
        {
            start.insert(index, at);
        }
        PointDrag {
            pointer,
            id: self.id.clone(),
            handle: self.handle,
            is_arrow: self.is_arrow,
            bind_reach,
            highlight: None,
            origin,
            start,
            moving,
            offset: (0.0, 0.0),
            step,
            points_before,
        }
    }

    /// The points of the Element without `deleted`, as they are shown.
    pub fn without(&self, deleted: &[usize]) -> Vec<Point> {
        self.points
            .iter()
            .enumerate()
            .filter(|(i, _)| !deleted.contains(i))
            .map(|(_, &p)| p)
            .collect()
    }

    pub fn handle(&self) -> u32 {
        self.handle
    }
}

/// The points of a line or arrow that are selected, in the order they were.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PointSelection {
    /// The Element they belong to.
    element: String,
    indices: Vec<usize>,
}

impl PointSelection {
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    pub fn contains(&self, index: usize) -> bool {
        self.indices.contains(&index)
    }

    /// Selects the point `index` of the Element `id` alone.
    pub fn select_only(&mut self, id: &str, index: usize) {
        self.element = id.to_owned();
        self.indices = vec![index];
    }

    /// Adds the point `index` of the Element `id` to the selected ones.
    pub fn add(&mut self, id: &str, index: usize) {
        if self.element != id {
            self.select_only(id, index);
        } else if !self.contains(index) {
            self.indices.push(index);
        }
    }

    /// Takes the point `index` out of the selected ones if it is in them, else
    /// adds it.
    pub fn toggle(&mut self, id: &str, index: usize) {
        if self.element == id && self.contains(index) {
            self.indices.retain(|&i| i != index);
        } else {
            self.add(id, index);
        }
    }

    /// Selects no point. Returns whether there was one selected.
    pub fn clear(&mut self) -> bool {
        let had = !self.indices.is_empty();
        self.indices.clear();
        had
    }

    /// Keeps only the selected points that still are there: the Element must
    /// still be the only one selected and be a line or arrow, and have points
    /// at their indices. Returns whether the selected points changed.
    pub fn retain_existing(&mut self, scene: &Scene, selection: &Selection) -> bool {
        if self.indices.is_empty() {
            return false;
        }
        let before = self.indices.len();
        let count = PointHandles::of(scene, selection, PointerKind::Mouse, 1.0)
            .filter(|handles| handles.id == self.element)
            .map(|handles| handles.points.len());
        match count {
            Some(count) => self.indices.retain(|&i| i < count),
            None => self.indices.clear(),
        }
        self.indices.len() != before
    }
}

/// Points being dragged by one pointer.
#[derive(Debug)]
pub(crate) struct PointDrag {
    pointer: u64,
    id: String,
    handle: u32,
    is_arrow: bool,
    /// How far from a shape an end of an arrow is put on it, in the Scene.
    bind_reach: f64,
    /// The shape the end of the arrow that is dragged is over, if any.
    highlight: Option<String>,
    /// Where the pointer went down, in the Scene.
    origin: Point,
    /// Where the points were shown when the drag began, with the added one.
    start: Vec<Point>,
    /// The indices of the points that move.
    moving: Vec<usize>,
    /// How far they are from where they started.
    offset: Point,
    step: Step,
    /// The selected points from before the drag, which selected the grabbed
    /// one.
    points_before: PointSelection,
}

impl PointDrag {
    /// The handle of the line or arrow whose points are dragged.
    pub fn element_handle(&self) -> u32 {
        self.handle
    }

    /// The pointer that drags the points.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// Follows the pointer to `to`: the points are where they started plus
    /// how far it travelled, so dragging back puts them back. Returns whether
    /// anything moved.
    pub fn drag_to(&mut self, scene: &mut Scene, entropy: &mut Entropy, to: Point) -> bool {
        let offset = (to.0 - self.origin.0, to.1 - self.origin.1);
        if offset == self.offset {
            return false;
        }
        self.place(scene, entropy, offset);
        true
    }

    /// The shape an end of an arrow that is dragged is over, which is
    /// highlighted.
    pub fn highlight(&self) -> Option<&str> {
        self.highlight.as_deref()
    }

    /// Ends the drag where it is; the step is what undoes it.
    pub fn finish(self) -> Step {
        self.step
    }

    /// Puts the Element back as it was before the drag. Returns the selected
    /// points from before it.
    pub fn cancel(self, scene: &mut Scene, entropy: &mut Entropy) -> PointSelection {
        self.step.revert(scene, entropy);
        self.points_before
    }

    fn place(&mut self, scene: &mut Scene, entropy: &mut Entropy, offset: Point) {
        let mut shown = self.start.clone();
        for &index in &self.moving {
            if let Some(point) = shown.get_mut(index) {
                *point = (point.0 + offset.0, point.1 + offset.1);
            }
        }
        reshape(scene, entropy, self.handle, &shown);
        if self.is_arrow {
            self.bind_dragged_end(scene, entropy, &shown);
        }
        self.offset = offset;
    }

    /// Binds the end of the arrow that is dragged, if it is one end and not
    /// both, to the shape it is over, or unbinds it, and then puts the bound
    /// ends where they belong: dragging a point may move where an end that is
    /// not dragged aims.
    fn bind_dragged_end(&mut self, scene: &mut Scene, entropy: &mut Entropy, shown: &[Point]) {
        let last = shown.len() - 1;
        let (first_moves, last_moves) = (self.moving.contains(&0), self.moving.contains(&last));
        if first_moves != last_moves {
            let (end, index) = if first_moves {
                (End::Start, 0)
            } else {
                (End::End, last)
            };
            self.highlight = binding::bind_end(
                scene,
                entropy,
                &mut self.step,
                &self.id,
                end,
                shown[index],
                self.bind_reach,
            );
        }
        binding::follow(scene, entropy, &self.id);
    }
}

/// Gives the line or arrow with `handle` the points `shown`, which are where
/// they are to be shown in the Scene.
///
/// The Element turns about the middle of its box, which moves when its points
/// do, so it is placed such that no point is shown anywhere but where it was
/// asked to be: the points are turned back by the angle about the middle of
/// the box they were in, and then moved by what the middle of their new box
/// moved. Its first point is its position (as in Excalidraw's
/// `getNormalizedPoints`), so the position changes with it.
pub(crate) fn reshape(scene: &mut Scene, entropy: &mut Entropy, handle: u32, shown: &[Point]) {
    let Some(element) = scene.element(handle) else {
        return;
    };
    let angle = element.angle();
    let pivot = Frame::pivot(&element);
    let (sin, cos) = angle.sin_cos();
    let unturned: Vec<Point> = shown
        .iter()
        .map(|&(x, y)| {
            let (dx, dy) = (x - pivot.0, y - pivot.1);
            (pivot.0 + dx * cos + dy * sin, pivot.1 - dx * sin + dy * cos)
        })
        .collect();
    let (min, max) = unturned.iter().fold(
        (
            (f64::INFINITY, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(min, max), p| {
            (
                (min.0.min(p.0), min.1.min(p.1)),
                (max.0.max(p.0), max.1.max(p.1)),
            )
        },
    );
    let centre = ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
    // What the middle of the box moved, turned: `(I - R) * (pivot - centre)`.
    let (dx, dy) = (pivot.0 - centre.0, pivot.1 - centre.1);
    let shift = (dx - (dx * cos - dy * sin), dy - (dx * sin + dy * cos));
    let placed: Vec<Point> = unturned
        .iter()
        .map(|&(x, y)| (x + shift.0, y + shift.1))
        .collect();
    let Some(&origin) = placed.first() else {
        return;
    };
    let relative: Vec<Point> = placed
        .iter()
        .map(|&(x, y)| (x - origin.0, y - origin.1))
        .collect();
    scene.set_points(handle, origin, &relative, entropy.integer());
}

/// Where the points of a line or arrow are shown in the Scene.
pub(crate) fn shown_points(element: &Element) -> Vec<Point> {
    let shown = Shown::of(element);
    element.points().into_iter().map(|p| shown.at(p)).collect()
}

/// The clean path of a line or arrow (see [`linear::clean_path`]) where it is
/// shown in the Scene. A closed line's last point is its first, so its
/// closing segment is part of it.
pub(crate) fn clean_path(element: &Element) -> Vec<Point> {
    let shown = Shown::of(element);
    linear::clean_path(element)
        .into_iter()
        .map(|p| shown.at(p))
        .collect()
}

/// Where the points of an Element are shown: the Element's frame turned by its
/// angle about the middle of its box.
struct Shown {
    x: f64,
    y: f64,
    pivot: Point,
    sin: f64,
    cos: f64,
}

impl Shown {
    fn of(element: &Element) -> Self {
        let (sin, cos) = element.angle().sin_cos();
        Self {
            x: element.x(),
            y: element.y(),
            pivot: Frame::pivot(element),
            sin,
            cos,
        }
    }

    /// Where the point `(px, py)` of the Element's frame is shown.
    fn at(&self, (px, py): Point) -> Point {
        let (dx, dy) = (self.x + px - self.pivot.0, self.y + py - self.pivot.1);
        (
            self.pivot.0 + dx * self.cos - dy * self.sin,
            self.pivot.1 + dx * self.sin + dy * self.cos,
        )
    }
}

/// The middle of each cubic of a curve (a move, then cubics), at half way
/// along its parameter.
fn cubic_middles(ops: &[Op]) -> Vec<Point> {
    let mut from = (0.0, 0.0);
    let mut middles = Vec::new();
    for op in ops {
        match *op {
            Op::Move(p) | Op::Line(p) => from = p,
            Op::Cubic(c1, c2, end) => {
                // (p0 + 3 c1 + 3 c2 + p1) / 8
                middles.push((
                    (from.0 + 3.0 * c1.0 + 3.0 * c2.0 + end.0) / 8.0,
                    (from.1 + 3.0 * c1.1 + 3.0 * c2.1 + end.1) / 8.0,
                ));
                from = end;
            }
        }
    }
    middles
}

fn distance(a: Point, b: Point) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn midway(a: Point, b: Point) -> Point {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

#[cfg(test)]
mod tests {
    //! Editing the points of a selected line or arrow (ADR-0006), tested at the
    //! Editor seam with recorded pointer sequences (see `select.rs` for their
    //! format): pointer events in, saved JSON and overlay out. Expected
    //! coordinates are worked out by hand in the comments.
    //!
    //! A line or arrow that is alone in the Selection shows a handle on each of
    //! its points. Dragging one moves it, dragging the handle at the middle of a
    //! segment adds a point there, tapping selects a point, and deleting removes
    //! the selected ones.

    use crate::{Editor, PartKind};
    use pkm_base::json::Value;

    use crate::test_support as common;
    use common::{editor_with, ops, replay, saved_elements, saved_points, tap};

    const SELECTION_COLOR: u32 = 0x6965_dbff;
    const WHITE: u32 = 0xffff_ffff;

    /// A line `id` at `(x, y)` through `points`, sharp unless `curved`.
    fn line_json(id: &str, (x, y): (f64, f64), points: &str, extra: &str) -> String {
        format!(
            r#"{{"id":"{id}","type":"line","x":{x},"y":{y},"width":100,"height":100,
        "points":{points},"roughness":0,"roundness":null,"strokeWidth":2{extra}}}"#
        )
    }

    /// The line `a` of three points: displayed at (50, 50), (150, 50) and
    /// (150, 150), with the middles of its segments at (100, 50) and (150, 100).
    fn corner() -> String {
        line_json("a", (50.0, 50.0), "[[0,0],[100,0],[100,100]]", "")
    }

    /// An editor with `elements`, the first of which is selected by tapping its
    /// first segment with `kind`, which the handles are made for.
    fn selected(kind: &str, elements: &[&str]) -> Editor {
        let mut editor = editor_with(elements);
        tap(&mut editor, kind, (100.0, 50.0), 0.0);
        assert_eq!(editor.selection(), ["a"], "tapping the line selects it");
        editor
    }

    type Position = (f64, f64);

    /// A handle of the overlay: a square with its centre, side and fill colour.
    #[derive(Debug, Clone, Copy)]
    struct Square {
        x: f64,
        y: f64,
        side: f64,
        color: u32,
    }

    /// The overlay's filled squares that belong to no Element, which are the
    /// handles.
    fn handles(editor: &Editor) -> Vec<Square> {
        let overlay = editor.overlay();
        overlay
            .parts()
            .iter()
            .filter(|p| p.kind == PartKind::Fill as u32 && p.element == u32::MAX)
            .map(|part| {
                let coords: Vec<f32> = ops(&overlay, part)
                    .into_iter()
                    .flat_map(|(_, c)| c)
                    .collect();
                let xs: Vec<f64> = coords.iter().step_by(2).map(|&c| f64::from(c)).collect();
                let ys: Vec<f64> = coords
                    .iter()
                    .skip(1)
                    .step_by(2)
                    .map(|&c| f64::from(c))
                    .collect();
                let (x0, x1) = (min(&xs), max(&xs));
                let (y0, y1) = (min(&ys), max(&ys));
                Square {
                    x: (x0 + x1) / 2.0,
                    y: (y0 + y1) / 2.0,
                    side: x1 - x0,
                    color: part.color,
                }
            })
            .collect()
    }

    fn min(values: &[f64]) -> f64 {
        values.iter().copied().fold(f64::INFINITY, f64::min)
    }

    fn max(values: &[f64]) -> f64 {
        values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }

    /// The handles of the points, which are bigger than those of segments'
    /// middles, and the latter, each as `(x, y)` rounded to a hundredth.
    fn point_and_middle_handles(editor: &Editor) -> (Vec<Position>, Vec<Position>) {
        let all = handles(editor);
        let biggest = all.iter().map(|s| s.side).fold(0.0, f64::max);
        let round = |s: &Square| ((s.x * 100.0).round() / 100.0, (s.y * 100.0).round() / 100.0);
        let points = all
            .iter()
            .filter(|s| s.side == biggest)
            .map(round)
            .collect();
        let middles = all.iter().filter(|s| s.side < biggest).map(round).collect();
        (points, middles)
    }

    /// A drag of `kind` from `from` to `to`, in steps, ending at `to`.
    fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64)) {
        replay(
            editor,
            &format!(
                "1.00 down 1 {kind} {} {}\n1.05 move 1 {kind} {} {}\n1.10 move 1 {kind} {} {}\n1.15 up 1 {kind} {} {}",
                from.0,
                from.1,
                (from.0 + to.0) / 2.0,
                (from.1 + to.1) / 2.0,
                to.0,
                to.1,
                to.0,
                to.1
            ),
        );
    }

    fn line(editor: &Editor) -> Value {
        saved_elements(editor)
            .into_iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some("a"))
            .unwrap()
    }

    fn frame(element: &Value) -> (f64, f64, f64, f64) {
        let n = |key: &str| element.get(key).and_then(Value::as_f64).unwrap();
        (n("x"), n("y"), n("width"), n("height"))
    }

    // --- Handles -------------------------------------------------------------------

    #[test]
    fn a_selected_line_shows_a_handle_on_each_point_and_in_the_middle_of_each_segment() {
        let editor = selected("mouse", &[&corner()]);

        let (points, middles) = point_and_middle_handles(&editor);

        assert_eq!(points, [(50.0, 50.0), (150.0, 50.0), (150.0, 150.0)]);
        assert_eq!(middles, [(100.0, 50.0), (150.0, 100.0)]);
    }

    #[test]
    fn the_handles_of_a_mouse_are_small_and_those_of_a_finger_touch_sized() {
        let long = line_json("a", (50.0, 50.0), "[[0,0],[200,0],[200,200]]", "");
        let mouse = selected("mouse", &[&long]);
        let finger = selected("finger", &[&long]);

        let sides = |editor: &Editor| {
            let mut sides: Vec<f64> = handles(editor).iter().map(|s| s.side).collect();
            sides.sort_by(f64::total_cmp);
            sides.dedup();
            sides
        };

        // Points are 8 view points wide for a mouse and 14 for a finger; the
        // handles in the middle of segments are half as big.
        assert_eq!(sides(&mouse), [4.0, 8.0]);
        assert_eq!(sides(&finger), [7.0, 14.0]);
    }

    #[test]
    fn handles_are_as_big_on_screen_at_any_zoom() {
        let mut editor = selected("mouse", &[&corner()]);
        editor.zoom_in();
        editor.zoom_in();
        let zoom = editor.viewport().zoom();
        assert!(zoom > 1.0);

        let side = handles(&editor).iter().map(|s| s.side).fold(0.0, f64::max);

        assert!((side * zoom - 8.0).abs() < 1e-3, "{side} at {zoom}");
    }

    #[test]
    fn a_segment_too_short_on_screen_has_no_handle_in_its_middle() {
        // Segments of 20 and 100: handles in the middle only for those that are
        // four point handles (32 view points) long or more.
        let line = line_json("a", (50.0, 50.0), "[[0,0],[20,0],[120,0]]", "");
        let editor = selected("mouse", &[&line]);

        let (_, middles) = point_and_middle_handles(&editor);

        assert_eq!(middles, [(120.0, 50.0)]);
    }

    #[test]
    fn the_middle_of_a_curved_segment_is_on_the_curve() {
        // A curve through (0, 0), (100, 100) and (200, 0). Its first segment
        // is the cubic with controls (16.67, 16.67) and (66.67, 100), whose
        // middle, (p0 + 3 c1 + 3 c2 + p1) / 8, is (43.75, 56.25).
        let curved = r#"{"id":"a","type":"line","x":0,"y":0,"width":200,"height":100,
        "points":[[0,0],[100,100],[200,0]],"roughness":0,"roundness":{"type":2}}"#;
        let mut editor = editor_with(&[curved]);
        tap(&mut editor, "mouse", (43.75, 56.25), 0.0);
        assert_eq!(editor.selection(), ["a"]);

        let (_, middles) = point_and_middle_handles(&editor);

        assert_eq!(middles[0], (43.75, 56.25));
    }

    #[test]
    fn only_a_line_that_is_alone_in_the_selection_has_point_handles() {
        let other = r##"{"id":"b","type":"rectangle","x":300,"y":300,"width":20,"height":20,
        "backgroundColor":"#ffc9c9","roundness":null}"##;
        let mut editor = selected("mouse", &[&corner(), other]);
        assert!(!handles(&editor).is_empty());

        editor.select_all();

        // The box around everything has handles of its own: eight to resize and
        // one to rotate, none of them on the points.
        let (points, _) = point_and_middle_handles(&editor);
        assert_eq!(points.len(), 9);
        assert!(!points.contains(&(150.0, 50.0)));
    }

    #[test]
    fn a_line_that_is_not_selected_has_no_handles() {
        let editor = editor_with(&[&corner()]);

        assert!(handles(&editor).is_empty());
    }

    #[test]
    fn an_elbow_arrow_has_no_point_handles() {
        // The core keeps its points as they are, and would not route it again.
        let elbow = r#"{"id":"a","type":"arrow","x":50,"y":50,"width":100,"height":100,
        "points":[[0,0],[100,0],[100,100]],"roughness":0,"roundness":null,"elbowed":true}"#;
        let editor = selected("mouse", &[elbow]);

        assert!(handles(&editor).is_empty());
    }

    // --- Selecting points -----------------------------------------------------------

    #[test]
    fn tapping_a_point_selects_it_and_it_looks_selected() {
        let mut editor = selected("mouse", &[&corner()]);
        assert!(editor.selected_points().is_empty());

        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

        assert_eq!(editor.selected_points(), [1]);
        assert_eq!(editor.selection(), ["a"], "the line stays selected");
        let at = |x: f64, y: f64| {
            handles(&editor)
                .into_iter()
                .find(|s| (s.x - x).abs() < 1e-3 && (s.y - y).abs() < 1e-3 && s.side == 8.0)
                .unwrap()
        };
        assert_eq!(at(150.0, 50.0).color, SELECTION_COLOR);
        assert_eq!(at(50.0, 50.0).color, WHITE);
    }

    #[test]
    fn tapping_another_point_selects_that_one_instead_and_shift_adds_to_them() {
        let mut editor = selected("mouse", &[&corner()]);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

        tap(&mut editor, "mouse", (150.0, 150.0), 2.0);
        assert_eq!(editor.selected_points(), [2]);

        replay(
            &mut editor,
            "3 down 1 mouse 50 50 shift\n3.05 up 1 mouse 50 50 shift",
        );
        assert_eq!(editor.selected_points(), [2, 0]);

        // Shift on a selected point takes it out again.
        replay(
            &mut editor,
            "4 down 1 mouse 150 150 shift\n4.05 up 1 mouse 150 150 shift",
        );
        assert_eq!(editor.selected_points(), [0]);
    }

    #[test]
    fn tapping_a_handle_does_not_deselect_the_line_even_off_the_line() {
        let mut editor = selected("finger", &[&corner()]);

        // 15 away from the point at (50, 50), inside the finger's grip.
        tap(&mut editor, "finger", (35.0, 50.0), 1.0);

        assert_eq!(editor.selection(), ["a"]);
        assert_eq!(editor.selected_points(), [0]);
    }

    #[test]
    fn tapping_empty_canvas_clears_the_selection_and_its_points() {
        let mut editor = selected("mouse", &[&corner()]);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

        tap(&mut editor, "mouse", (500.0, 500.0), 2.0);

        assert!(editor.selection().is_empty());
        assert!(editor.selected_points().is_empty());
        // Selecting the line again starts without a point selected.
        tap(&mut editor, "mouse", (100.0, 50.0), 3.0);
        assert!(editor.selected_points().is_empty());
    }

    #[test]
    fn tapping_the_middle_of_a_segment_changes_nothing() {
        let mut editor = selected("mouse", &[&corner()]);
        let before = editor.save();

        tap(&mut editor, "mouse", (150.0, 100.0), 1.0);

        assert_eq!(editor.save(), before);
        assert_eq!(editor.selection(), ["a"]);
        assert!(editor.selected_points().is_empty());
    }

    // --- Moving points ---------------------------------------------------------------

    #[test]
    fn dragging_a_point_moves_it() {
        let mut editor = selected("mouse", &[&corner()]);

        drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));

        // The second point is now at (170, 80): (120, 30) from the line's
        // position, and the others have not moved.
        let moved = line(&editor);
        assert_eq!(
            saved_points(&moved),
            [(0.0, 0.0), (120.0, 30.0), (100.0, 100.0)]
        );
        assert_eq!(frame(&moved), (50.0, 50.0, 120.0, 100.0));
        assert_eq!(
            editor.selected_points(),
            [1],
            "the dragged point is selected"
        );
    }

    #[test]
    fn dragging_the_first_point_moves_the_line_s_position_with_it() {
        let mut editor = selected("mouse", &[&corner()]);

        drag(&mut editor, "mouse", (50.0, 50.0), (30.0, 40.0));

        // The first point is at (30, 40) and so is the position of the line; the
        // others, still at (150, 50) and (150, 150), are relative to it.
        let moved = line(&editor);
        assert_eq!(
            saved_points(&moved),
            [(0.0, 0.0), (120.0, 10.0), (120.0, 110.0)]
        );
        assert_eq!(frame(&moved), (30.0, 40.0, 120.0, 110.0));
    }

    #[test]
    fn a_point_follows_how_far_the_pointer_travelled_not_where_it_is() {
        let mut editor = selected("finger", &[&corner()]);

        // The finger took hold at (165, 60), 15 and 10 from the point at
        // (150, 50), and moves by (20, 30).
        drag(&mut editor, "finger", (165.0, 60.0), (185.0, 90.0));

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (120.0, 30.0), (100.0, 100.0)]
        );
    }

    #[test]
    fn dragging_moves_all_selected_points_together() {
        let mut editor = selected("mouse", &[&corner()]);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
        replay(
            &mut editor,
            "2 down 1 mouse 150 150 shift\n2.05 up 1 mouse 150 150 shift",
        );
        assert_eq!(editor.selected_points(), [1, 2]);

        drag(&mut editor, "mouse", (150.0, 150.0), (160.0, 170.0));

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (110.0, 20.0), (110.0, 120.0)]
        );
    }

    #[test]
    fn dragging_the_line_between_its_points_still_moves_it() {
        let mut editor = selected("mouse", &[&corner()]);

        // (120, 50) is on the first segment, away from the handles.
        drag(&mut editor, "mouse", (120.0, 50.0), (130.0, 70.0));

        assert_eq!(frame(&line(&editor)), (60.0, 70.0, 100.0, 100.0));
        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
        );
    }

    #[test]
    fn a_dragged_point_updates_the_overlay() {
        let mut editor = selected("mouse", &[&corner()]);
        let before = editor.overlay_revision();

        drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));

        assert_ne!(editor.overlay_revision(), before);
        let (points, _) = point_and_middle_handles(&editor);
        assert_eq!(points, [(50.0, 50.0), (170.0, 80.0), (150.0, 150.0)]);
    }

    #[test]
    fn a_turned_line_keeps_its_other_points_where_they_are_when_one_is_moved() {
        // A line from (0, 0) to (100, 0), turned a quarter turn about the middle
        // of its points, (50, 0): it is displayed from (50, -50) to (50, 50).
        let turned = r#"{"id":"a","type":"line","x":0,"y":0,"width":100,"height":0,
        "angle":1.5707963267948966,"points":[[0,0],[100,0]],"roughness":0,"roundness":null}"#;
        let mut editor = editor_with(&[turned]);
        tap(&mut editor, "mouse", (50.0, 0.0), 0.0);
        let (points, _) = point_and_middle_handles(&editor);
        assert_eq!(points.len(), 2);
        assert!((points[0].0 - 50.0).abs() < 1e-2 && (points[0].1 + 50.0).abs() < 1e-2);
        assert!((points[1].0 - 50.0).abs() < 1e-2 && (points[1].1 - 50.0).abs() < 1e-2);

        // Drag the lower end down by 50.
        drag(&mut editor, "mouse", (50.0, 50.0), (50.0, 100.0));

        let (points, _) = point_and_middle_handles(&editor);
        assert!(
            (points[0].0 - 50.0).abs() < 1e-2 && (points[0].1 + 50.0).abs() < 1e-2,
            "{points:?}"
        );
        assert!(
            (points[1].0 - 50.0).abs() < 1e-2 && (points[1].1 - 100.0).abs() < 1e-2,
            "{points:?}"
        );
    }

    // --- Adding points ----------------------------------------------------------------

    #[test]
    fn dragging_the_middle_of_a_segment_adds_a_point_there() {
        let mut editor = selected("mouse", &[&corner()]);

        drag(&mut editor, "mouse", (100.0, 50.0), (100.0, 20.0));

        // A point between the first and second, now at (100, 20), which is
        // (50, -30) from the line's position.
        let edited = line(&editor);
        assert_eq!(
            saved_points(&edited),
            [(0.0, 0.0), (50.0, -30.0), (100.0, 0.0), (100.0, 100.0)]
        );
        assert_eq!(frame(&edited), (50.0, 50.0, 100.0, 130.0));
        assert_eq!(editor.selected_points(), [1], "the new point is selected");
    }

    #[test]
    fn a_two_point_line_gets_a_third_point_by_dragging_its_middle() {
        let two = line_json("a", (0.0, 0.0), "[[0,0],[200,0]]", "");
        let mut editor = editor_with(&[&two]);
        tap(&mut editor, "finger", (50.0, 0.0), 0.0);

        drag(&mut editor, "finger", (100.0, 0.0), (100.0, 80.0));

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (100.0, 80.0), (200.0, 0.0)]
        );
    }

    #[test]
    fn undoing_an_added_point_takes_it_away_again() {
        let mut editor = selected("mouse", &[&corner()]);
        drag(&mut editor, "mouse", (100.0, 50.0), (100.0, 20.0));

        editor.undo();

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
        );
        assert!(editor.selected_points().is_empty(), "the point is gone");
    }

    // --- Deleting points ----------------------------------------------------------------

    #[test]
    fn deleting_the_selected_point_removes_it() {
        let mut editor = selected("mouse", &[&corner()]);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

        editor.delete_selection();

        assert_eq!(saved_points(&line(&editor)), [(0.0, 0.0), (100.0, 100.0)]);
        assert_eq!(frame(&line(&editor)), (50.0, 50.0, 100.0, 100.0));
        assert_eq!(
            editor.selected_points(),
            [0],
            "the point before is selected"
        );
    }

    #[test]
    fn deleting_the_first_point_moves_the_position_to_the_next() {
        let mut editor = selected("mouse", &[&corner()]);
        tap(&mut editor, "mouse", (50.0, 50.0), 1.0);

        editor.delete_selection();

        // The points left are at (150, 50) and (150, 150).
        let left = line(&editor);
        assert_eq!(saved_points(&left), [(0.0, 0.0), (0.0, 100.0)]);
        assert_eq!((frame(&left).0, frame(&left).1), (150.0, 50.0));
    }

    #[test]
    fn deleting_leaves_a_line_of_two_points_or_nothing() {
        let two = line_json("a", (50.0, 50.0), "[[0,0],[100,0]]", "");
        let mut editor = editor_with(&[&two]);
        tap(&mut editor, "mouse", (100.0, 50.0), 0.0);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
        assert_eq!(editor.selected_points(), [1]);

        editor.delete_selection();

        // A line needs two points: with one gone there is nothing left.
        assert_eq!(editor.element_count(), 0);
        assert!(editor.selection().is_empty());
        assert!(editor.selected_points().is_empty());
    }

    #[test]
    fn deleting_without_a_selected_point_deletes_the_line() {
        let mut editor = selected("mouse", &[&corner()]);

        editor.delete_selection();

        assert_eq!(editor.element_count(), 0);
        assert!(editor.selection().is_empty());
    }

    #[test]
    fn tapping_the_line_away_from_its_points_unselects_them() {
        let mut editor = selected("mouse", &[&corner()]);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
        assert_eq!(editor.selected_points(), [1]);

        // On the second segment, between its handles.
        tap(&mut editor, "mouse", (150.0, 120.0), 2.0);

        assert_eq!(editor.selection(), ["a"]);
        assert!(editor.selected_points().is_empty());
    }

    #[test]
    fn deleting_an_arrow_by_its_points_deletes_its_label() {
        let arrow = r#"{"id":"a","type":"arrow","x":50,"y":50,"width":100,"height":0,
        "points":[[0,0],[100,0]],"roughness":0,"roundness":null,"strokeWidth":2,
        "boundElements":[{"id":"t","type":"text"}]}"#;
        let label = r#"{"id":"t","type":"text","x":90,"y":40,"width":20,"height":20,
        "text":"Hi","originalText":"Hi","containerId":"a","fontSize":16,
        "textAlign":"center","verticalAlign":"middle","autoResize":true}"#;
        let mut editor = editor_with(&[arrow, label]);
        tap(&mut editor, "mouse", (70.0, 50.0), 0.0);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
        assert_eq!(editor.selected_points(), [1]);

        editor.delete_selection();

        assert_eq!(editor.element_count(), 0, "the label goes with its arrow");
    }

    #[test]
    fn undoing_a_deleted_point_brings_it_back() {
        let mut editor = selected("mouse", &[&corner()]);
        tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
        editor.delete_selection();

        editor.undo();

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
        );
        editor.redo();
        assert_eq!(saved_points(&line(&editor)), [(0.0, 0.0), (100.0, 100.0)]);
    }

    // --- History and interruptions -----------------------------------------------------

    #[test]
    fn a_whole_drag_of_a_point_is_one_undo_step() {
        let mut editor = selected("mouse", &[&corner()]);

        drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));
        editor.undo();

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
        );
        assert!(!editor.can_undo());
        editor.redo();
        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (120.0, 30.0), (100.0, 100.0)]
        );
    }

    #[test]
    fn a_second_finger_while_dragging_a_point_puts_it_back() {
        let mut editor = selected("finger", &[&corner()]);

        replay(
            &mut editor,
            "1 down 1 finger 150 50\n1.1 move 1 finger 170 70\n1.2 move 1 finger 180 80\n1.3 down 2 finger 400 400",
        );

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
        );
        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn cancelling_a_drag_of_a_point_puts_it_back() {
        let mut editor = selected("pencil", &[&corner()]);

        replay(
            &mut editor,
            "1 down 1 pencil 150 50\n1.1 move 1 pencil 170 70\n1.2 move 1 pencil 180 80\n1.3 cancel 1 pencil 180 80",
        );

        assert_eq!(
            saved_points(&line(&editor)),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
        );
        assert!(!editor.can_undo());
    }

    #[test]
    fn undo_does_nothing_while_a_point_is_being_dragged() {
        let mut editor = selected("mouse", &[&corner()]);
        drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));
        replay(
            &mut editor,
            "2 down 1 mouse 150 150\n2.1 move 1 mouse 170 170\n2.2 move 1 mouse 180 180",
        );

        editor.undo();

        let (points, _) = point_and_middle_handles(&editor);
        assert_eq!(points[2], (180.0, 180.0));
    }

    #[test]
    fn the_geometry_of_an_edited_line_changes_with_its_points() {
        let mut editor = selected("mouse", &[&corner()]);
        let before = editor.revision();

        drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));

        assert_ne!(editor.revision(), before);
    }
}
