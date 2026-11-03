//! Moving the Selection by dragging it (ADR-0006).
//!
//! The Elements follow the pointer one-to-one: wherever the pointer is, they
//! are where they started plus how far it travelled from where it went down,
//! so dragging back puts them back. Every step is a new version of each moved
//! Element, as on excalidraw.com, which is what makes the move part of the
//! saved Drawing.

use crate::random::Entropy;
use crate::scene::{ElementType, Scene};
use crate::selection::Selection;

/// A drag of the Selection by one pointer.
#[derive(Debug)]
pub(crate) struct MoveGesture {
    pointer: u64,
    /// Where the pointer went down, in Scene coordinates.
    origin: (f64, f64),
    /// The Elements that move, by handle, with where each started.
    moving: Vec<(u32, (f64, f64))>,
    /// How far they are from where they started.
    offset: (f64, f64),
}

impl MoveGesture {
    /// Starts moving the Selection with `pointer`, which went down at
    /// `origin`. The labels of selected Elements move with them, although
    /// they are not selected themselves.
    pub fn begin(scene: &Scene, selection: &Selection, pointer: u64, origin: (f64, f64)) -> Self {
        let moving = scene
            .elements()
            .filter(|e| !e.is_deleted() && !e.is_locked())
            .filter(|e| {
                selection.contains(e.id())
                    || (e.kind() == ElementType::Text
                        && e.container_id().is_some_and(|id| selection.contains(id)))
            })
            .map(|e| (e.handle(), (e.x(), e.y())))
            .collect();
        Self {
            pointer,
            origin,
            moving,
            offset: (0.0, 0.0),
        }
    }

    /// The pointer that moves the Selection.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// Follows the pointer to `to`. Returns whether anything moved.
    pub fn drag_to(&mut self, scene: &mut Scene, entropy: &mut Entropy, to: (f64, f64)) -> bool {
        let offset = (to.0 - self.origin.0, to.1 - self.origin.1);
        if offset == self.offset {
            return false;
        }
        self.place(scene, entropy, offset);
        true
    }

    /// Puts the Elements back where they started.
    pub fn cancel(mut self, scene: &mut Scene, entropy: &mut Entropy) {
        if self.offset != (0.0, 0.0) {
            self.place(scene, entropy, (0.0, 0.0));
        }
    }

    fn place(&mut self, scene: &mut Scene, entropy: &mut Entropy, offset: (f64, f64)) {
        for &(handle, (x, y)) in &self.moving {
            scene.set_position(handle, (x + offset.0, y + offset.1), entropy.integer());
        }
        self.offset = offset;
    }
}
