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
use crate::random::Entropy;
use crate::rough::Op;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use crate::transform_handles::{Grip, targets};

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
