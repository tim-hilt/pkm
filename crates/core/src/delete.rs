//! Deleting the Selection (the `delete` command, Excalidraw's `actionDeleteSelected`).
//!
//! A deleted Element stays in the Scene, marked deleted, as on excalidraw.com.
//! The Elements that belong to it go with it, as with the eraser: the label of
//! a container. Arrows bound to a deleted shape are unbound from it, and
//! stay.

use crate::erase;
use crate::history::Step;
use crate::random::Entropy;
use crate::scene::Scene;
use crate::selection::Selection;

/// Deletes the Elements of `selection`, and their labels, as part of `step`.
pub(crate) fn delete(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
) {
    // In z-order, so that what is done does not depend on the Selection's.
    let doomed: Vec<String> = scene
        .elements()
        .filter(|e| !e.is_deleted() && selection.covers(e))
        .map(|e| e.id().to_owned())
        .collect();
    for id in &doomed {
        erase::erase(scene, entropy, step, id);
    }
}
