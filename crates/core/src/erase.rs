//! Erasing with the eraser Tool (ADR-0004, ADR-0006), as Excalidraw does.
//!
//! While the pointer is down, every step of it from one point to the next
//! *touches* the Elements it passes (see [`hit_test::crossed_by`]). Touched
//! Elements are not deleted at once but previewed faded, so that the pointer
//! can still be taken back by cancelling; they are deleted when the pointer
//! lifts, all as one undo step. A press that never moved is a tap, which
//! touches every Element under it.
//!
//! Erasing an Element takes with it what belongs to it: the Elements of its
//! outermost group, a label and the container of a label. Arrows bound to an
//! erased shape are unbound from it, and stay.

use std::collections::HashSet;

use crate::binding;
use crate::history::Step;
use crate::hit_test::{self, Reach};
use crate::input::{PointerEvent, PointerKind};
use crate::path::Point;
use crate::random::Entropy;
use crate::scene::Scene;

/// An erase by one pointer, from going down to lifting.
#[derive(Debug)]
pub(crate) struct Erasing {
    pointer: u64,
    kind: PointerKind,
    /// Where the pointer went down, in Scene coordinates.
    start: Point,
    /// Where the eraser path is now: the next step begins here.
    last: Point,
    /// The ids of the Elements to delete on release.
    pending: HashSet<String>,
    step: Step,
}

impl Erasing {
    /// Starts an erase with `event`, the pointer going down at `at` in the
    /// Scene, as the action of `step`.
    pub fn begin(step: Step, event: PointerEvent, at: Point) -> Self {
        Self {
            pointer: event.id,
            kind: event.kind,
            start: at,
            last: at,
            pending: HashSet::new(),
            step,
        }
    }

    /// The pointer that erases.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// Whether the Element `id` is previewed as about to be erased.
    pub fn is_pending(&self, id: &str) -> bool {
        self.pending.contains(id)
    }

    /// Takes the eraser path on to `to` in the Scene, in a view at `zoom`.
    /// Returns whether more Elements are about to be erased.
    pub fn drag_to(&mut self, scene: &Scene, to: Point, zoom: f64) -> bool {
        let from = std::mem::replace(&mut self.last, to);
        if from == to {
            return false;
        }
        let touched: Vec<&str> = scene
            .elements()
            .filter(|e| !e.is_deleted() && !e.is_locked() && !self.pending.contains(e.id()))
            .filter(|e| hit_test::crossed_by(e, from, to, zoom))
            .map(|e| e.id())
            .collect();
        let before = self.pending.len();
        for id in touched {
            self.add_with_dependents(scene, id);
        }
        self.pending.len() != before
    }

    /// Ends the erase with the pointer lifting at `at` in the Scene: deletes
    /// what is previewed as about to be erased. The step is what undoes it;
    /// it holds no change if nothing was touched.
    pub fn finish(
        mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        at: Point,
        zoom: f64,
    ) -> Step {
        self.drag_to(scene, at, zoom);
        if self.start == at {
            // A tap touches what is under it.
            let reach = Reach::new(self.kind, zoom);
            let tapped: Vec<&str> = hit_test::all_at(scene, at, reach).map(|e| e.id()).collect();
            for id in tapped {
                self.add_with_dependents(scene, id);
            }
        }
        let Self {
            pending, mut step, ..
        } = self;
        // In z-order, so that what is done does not depend on hashing.
        let doomed: Vec<String> = scene
            .elements()
            .filter(|e| pending.contains(e.id()))
            .map(|e| e.id().to_owned())
            .collect();
        for id in doomed {
            erase(scene, entropy, &mut step, &id);
        }
        step
    }

    /// Adds the Element `id` to those about to be erased, with the Elements
    /// that go with it.
    fn add_with_dependents(&mut self, scene: &Scene, id: &str) {
        self.pending.insert(id.to_owned());
        let Some(element) = scene.element_by_id(id) else {
            return;
        };
        let group = element.outermost_group();
        // A label goes with its container, and a container with its labels.
        let container = element.container_id();
        let dependents = scene.elements().filter(|e| {
            !e.is_deleted()
                && (group.is_some_and(|g| e.is_in_group(g))
                    || container == Some(e.id())
                    || e.container_id() == Some(id))
        });
        let ids: Vec<String> = dependents.map(|e| e.id().to_owned()).collect();
        self.pending.extend(ids);
    }
}

/// Deletes the Element `id` as part of `step`, unbinding what was bound to it.
fn erase(scene: &mut Scene, entropy: &mut Entropy, step: &mut Step, id: &str) {
    binding::unbind_all(scene, entropy, step, id);
    binding::unbind_from(scene, entropy, step, id);
    step.touch(scene, id);
    scene.mark_deleted(id, entropy.integer());
}
