//! The reordering itself: a port of Excalidraw's `zindex.ts` at the pinned
//! commit, on a stack of [`Layer`]s instead of Elements, so that it can be
//! read next to the original.
//!
//! With a group gone into (`editingGroupId`, see [`crate::group`]) an Element
//! moves among the Elements of that group only, and its inner groups are
//! the units passed. Not ported is selecting a frame, which would take its children along, because
//! the core has no such thing.

use std::collections::{HashMap, HashSet};

use crate::index_sync;

/// What the reordering needs to know about an Element, whatever its type.
#[derive(Debug, Clone, Default)]
pub(super) struct Layer {
    pub id: String,
    pub deleted: bool,
    /// Innermost group first.
    pub groups: Vec<String>,
    pub frame: Option<String>,
    pub is_frame: bool,
    /// The container of a label.
    pub container: Option<String>,
    /// The label of a container.
    pub label: Option<String>,
    pub index: Option<String>,
}

/// Which way an Element moves: towards the start of the stack (the back of
/// the Canvas) or its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Back,
    Front,
}

/// The Elements in z-order, bottom first, which is the order of the file.
pub(super) struct Stack<'a> {
    layers: &'a [Layer],
    /// The group that was gone into, if any.
    editing_group: Option<&'a str>,
    by_id: HashMap<&'a str, usize>,
    /// The layers (as positions in `layers`) in their current order.
    order: Vec<usize>,
    /// The current index of each of `layers`.
    indices: Vec<Option<String>>,
}

/// What reordering made of the stack.
pub(super) struct Outcome {
    /// The positions the layers had before, bottom first.
    pub order: Vec<usize>,
    /// The layers (by position before) whose index changed, with the new one.
    pub indices: Vec<(usize, String)>,
}

impl<'a> Stack<'a> {
    /// The stack of `layers`, bottom first, with `editing_group` gone into.
    pub fn new(layers: &'a [Layer], editing_group: Option<&'a str>) -> Self {
        Self {
            layers,
            editing_group,
            by_id: layers
                .iter()
                .enumerate()
                .map(|(position, layer)| (layer.id.as_str(), position))
                .collect(),
            order: (0..layers.len()).collect(),
            indices: layers.iter().map(|l| l.index.clone()).collect(),
        }
    }

    /// What became of the stack, or `None` if no layer changed its place.
    pub fn into_outcome(self) -> Option<Outcome> {
        if self
            .order
            .iter()
            .enumerate()
            .all(|(place, &layer)| place == layer)
        {
            return None;
        }
        let indices = self
            .indices
            .into_iter()
            .enumerate()
            .zip(self.layers)
            .filter(|((_, now), layer)| *now != layer.index)
            .filter_map(|((position, now), _)| Some((position, now?)))
            .collect();
        Some(Outcome {
            order: self.order,
            indices,
        })
    }

    /// `bringForward` and `sendBackward`: moves the layers flagged in
    /// `wanted` (by position in the layers the stack was made of) one place, past
    /// the next shown one (a whole group, frame or label together with its
    /// container counts as one).
    pub fn shift_by_one(&mut self, direction: Direction, wanted: &[bool]) {
        let to_move = self.places_to_move(wanted);
        let moved: Vec<usize> = to_move.iter().map(|&place| self.order[place]).collect();
        let mut runs = contiguous_runs(&to_move);
        if direction == Direction::Front {
            runs.reverse();
        }
        let selected_frames: HashSet<&str> = to_move
            .iter()
            .filter(|&&place| self.at(place).is_frame)
            .map(|&place| self.at(place).id.as_str())
            .collect();

        let mut order = self.order.clone();
        for run in runs {
            let (leading, trailing) = (run[0], run[run.len() - 1]);
            let boundary = match direction {
                Direction::Back => leading,
                Direction::Front => trailing,
            };
            let containing_frame = if run.iter().any(|&place| {
                self.layer_in(&order, place)
                    .frame
                    .as_deref()
                    .is_some_and(|f| selected_frames.contains(f))
            }) {
                None
            } else {
                self.layer_in(&order, boundary).frame.as_deref()
            };
            let Some(target) = self.target_place(&order, boundary, direction, containing_frame)
            else {
                continue;
            };
            let (before, between, after) = match direction {
                Direction::Back if target < leading => (
                    &order[..target],
                    &order[target..leading],
                    &order[trailing + 1..],
                ),
                Direction::Front if target > trailing => (
                    &order[..leading],
                    &order[trailing + 1..=target],
                    &order[target + 1..],
                ),
                _ => continue,
            };
            let moving = &order[leading..=trailing];
            order = match direction {
                Direction::Back => [before, moving, between, after].concat(),
                Direction::Front => [before, between, moving, after].concat(),
            };
        }
        self.order = order;
        self.sync_indices(&moved);
    }

    /// `sendToBack` and `bringToFront`: moves the layers flagged in `wanted` past everything
    /// else, to the back or the front, frame children within their frame.
    pub fn shift_to_end(&mut self, direction: Direction, wanted: &[bool]) {
        let selected_frames: HashSet<&str> = self
            .layers
            .iter()
            .enumerate()
            .filter(|&(position, layer)| wanted[position] && layer.is_frame)
            .map(|(_, layer)| layer.id.as_str())
            .collect();
        let mut regular = vec![false; wanted.len()];
        let mut in_frames: Vec<(&str, Vec<bool>)> = Vec::new();
        for &position in &self.order {
            if !wanted[position] {
                continue;
            }
            let layer = &self.layers[position];
            match layer.frame.as_deref() {
                Some(frame) if !layer.is_frame && !selected_frames.contains(frame) => {
                    let entry = match in_frames.iter().position(|(f, _)| *f == frame) {
                        Some(entry) => entry,
                        None => {
                            in_frames.push((frame, vec![false; wanted.len()]));
                            in_frames.len() - 1
                        }
                    };
                    in_frames[entry].1[position] = true;
                }
                _ => regular[position] = true,
            }
        }
        for (frame, children) in in_frames {
            self.shift_to_end_within(direction, Some(frame), &children);
        }
        self.shift_to_end_within(direction, None, &regular);
    }

    fn shift_to_end_within(&mut self, direction: Direction, frame: Option<&str>, wanted: &[bool]) {
        let to_move = self.places_to_move(wanted);
        let (Some(&first), Some(&last)) = (to_move.first(), to_move.last()) else {
            return;
        };
        let is_of_frame = |layer: &Layer| {
            frame.is_some_and(|f| layer.frame.as_deref() == Some(f) || layer.id == f)
        };
        let places = || self.order.iter().map(|&l| &self.layers[l]);
        let in_editing_group = |layer: &Layer| {
            self.editing_group
                .is_some_and(|g| layer.groups.iter().any(|id| id == g))
        };
        // The first and last place the move may reach: those of the frame,
        // or else of the group gone into, or else of the whole stack.
        let (lowest, highest) = if frame.is_some() {
            (
                places().position(is_of_frame),
                places().rposition(is_of_frame),
            )
        } else if self.editing_group.is_some() {
            (
                places().position(in_editing_group),
                places().rposition(in_editing_group),
            )
        } else {
            (Some(0), self.order.len().checked_sub(1))
        };
        let (leading, trailing) = match direction {
            Direction::Back => (lowest, Some(last)),
            Direction::Front => (Some(first), highest),
        };
        // A frame without a first layer begins at the bottom.
        let (Some(leading), Some(trailing)) = (leading, trailing) else {
            return;
        };
        if leading > trailing || first < leading || last > trailing {
            return;
        }
        let mut is_moving = vec![false; self.order.len()];
        for &place in &to_move {
            is_moving[place] = true;
        }
        let moving: Vec<usize> = to_move.iter().map(|&p| self.order[p]).collect();
        let displaced: Vec<usize> = (leading..=trailing)
            .filter(|&place| !is_moving[place])
            .map(|place| self.order[place])
            .collect();
        let (before, after) = (&self.order[..leading], &self.order[trailing + 1..]);
        let order = match direction {
            Direction::Back => [before, &moving, &displaced, after].concat(),
            Direction::Front => [before, &displaced, &moving, after].concat(),
        };
        self.order = order;
        self.sync_indices(&moving);
    }

    /// `getIndicesToMove`: the places of the layers flagged in `wanted`, and
    /// of the deleted ones between them, which move along.
    fn places_to_move(&self, wanted: &[bool]) -> Vec<usize> {
        let mut selected = Vec::new();
        let mut deleted = Vec::new();
        let mut include_deleted_at = None;
        for place in 0..self.order.len() {
            if wanted[self.order[place]] {
                selected.append(&mut deleted);
                selected.push(place);
                include_deleted_at = Some(place + 1);
            } else if self.at(place).deleted && include_deleted_at == Some(place) {
                include_deleted_at = Some(place + 1);
                deleted.push(place);
            } else {
                deleted.clear();
            }
        }
        selected
    }

    /// `getTargetIndex`: the place a run of layers is moved to, past the
    /// next shown layer from `boundary` that is not in the way, `None` if
    /// there is none.
    fn target_place(
        &self,
        order: &[usize],
        boundary: usize,
        direction: Direction,
        containing_frame: Option<&str>,
    ) -> Option<usize> {
        // Inside the group gone into, the closest member of it, whatever
        // lies in between (as Excalidraw does, for legacy reasons).
        let counts = |place: usize| {
            let layer = self.layer_in(order, place);
            !layer.deleted
                && match (containing_frame, self.editing_group) {
                    (Some(frame), _) => layer.frame.as_deref() == Some(frame),
                    (None, Some(editing)) => layer.groups.iter().any(|g| g == editing),
                    (None, None) => true,
                }
        };
        let candidate = match direction {
            Direction::Back => {
                let from = boundary.saturating_sub(1).min(order.len().checked_sub(1)?);
                (0..=from).rev().find(|&place| counts(place))?
            }
            Direction::Front => (boundary + 1..order.len()).find(|&place| counts(place))?,
        };
        let next = self.layer_in(order, candidate);
        if let Some(editing) = self.editing_group {
            let source = self.layer_in(order, boundary);
            if source.groups == next.groups {
                return self
                    .place_with_label(order, candidate, direction)
                    .or(Some(candidate));
            }
            if !next.groups.iter().any(|g| g == editing) {
                // Never out of the group gone into.
                return None;
            }
        }
        let places_where = |test: &dyn Fn(&Layer) -> bool| -> Vec<usize> {
            (0..order.len())
                .filter(|&place| test(self.layer_in(order, place)))
                .collect()
        };
        let edge = |places: Vec<usize>| match direction {
            Direction::Back => places.first().copied(),
            Direction::Front => places.last().copied(),
        };

        // Frames and what is in them are passed as a whole.
        if containing_frame.is_none() && (next.frame.is_some() || next.is_frame) {
            let frame = next.frame.as_deref().unwrap_or(&next.id);
            return edge(places_where(&|l| {
                l.frame.as_deref() == Some(frame) || l.id == frame
            }));
        }
        // A group is passed as a whole: the outermost one, or inside the
        // group gone into, the one just below it.
        let group = match self.editing_group {
            Some(editing) => next
                .groups
                .iter()
                .position(|g| g == editing)
                .and_then(|at| at.checked_sub(1))
                .map(|below| &next.groups[below]),
            None => next.groups.last(),
        };
        let Some(group) = group else {
            return if next.groups.is_empty() {
                self.place_with_label(order, candidate, direction)
                    .or(Some(candidate))
            } else {
                Some(candidate)
            };
        };
        edge(places_where(&|l| l.groups.contains(group))).or(Some(candidate))
    }

    /// `getTargetIndexAccountingForBinding`: where a layer is in the way
    /// together with its label or container, which is one unit.
    fn place_with_label(
        &self,
        order: &[usize],
        place: usize,
        direction: Direction,
    ) -> Option<usize> {
        let layer = self.layer_in(order, place);
        let partner = match (&layer.container, &layer.label) {
            (Some(container), _) => container,
            (None, Some(label)) => label,
            (None, None) => return None,
        };
        let partner_layer = *self.by_id.get(partner.as_str())?;
        let partner = order.iter().position(|&l| l == partner_layer)?;
        Some(match direction {
            Direction::Back => partner.min(place),
            Direction::Front => partner.max(place),
        })
    }

    /// Gives the layers that were moved indices that fit their new places,
    /// see [`index_sync::sync_moved`].
    fn sync_indices(&mut self, moved: &[usize]) {
        let mut flags = vec![false; self.order.len()];
        for &layer in moved {
            flags[layer] = true;
        }
        let by_place: Vec<Option<&str>> = self
            .order
            .iter()
            .map(|&layer| self.indices[layer].as_deref())
            .collect();
        let moved_by_place: Vec<bool> = self.order.iter().map(|&layer| flags[layer]).collect();
        let updates = index_sync::sync_moved(&by_place, &moved_by_place);
        for (place, index) in updates {
            self.indices[self.order[place]] = Some(index);
        }
    }

    fn at(&self, place: usize) -> &Layer {
        &self.layers[self.order[place]]
    }

    fn layer_in(&self, order: &[usize], place: usize) -> &Layer {
        &self.layers[order[place]]
    }
}

/// `toContiguousGroups`: `places`, ascending, split where they skip one.
fn contiguous_runs(places: &[usize]) -> Vec<Vec<usize>> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for &place in places {
        match runs.last_mut() {
            Some(run) if run.last().is_some_and(|&last| last + 1 == place) => run.push(place),
            _ => runs.push(vec![place]),
        }
    }
    runs
}
