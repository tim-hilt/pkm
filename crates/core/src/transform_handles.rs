//! Transform handles: the grips around the Selection that resize and rotate
//! it (`transformHandles.ts` and `resizeTest.ts` at the pinned commit).
//!
//! A [`TransformBox`] is the box the Selection is transformed as a whole: that
//! of the Element if it is alone (turned with it), or else the box around all
//! of them (never turned). It has eight resize handles, at its corners and at
//! the middle of its sides, and a rotation handle above its top. The handles
//! straddle the selection frame (see [`crate::overlay`]), their centres just
//! beyond it, and are as big as the pointer needs them to be: a finger is a
//! lot less precise than a mouse.
//!
//! Unlike Excalidraw on a desktop, resizing from the sides of the box (rather
//! than from the handles on them) is not offered: it would make every drag
//! along the edge of a hollow Element a resize, and a finger has handles.

use crate::frame::Frame;
use crate::input::PointerKind;
use crate::path::Point;
use crate::scene::{Element, Scene};
use crate::selection::Selection;

/// How far in view points the selection frame keeps from the box (Excalidraw's
/// `DEFAULT_TRANSFORM_HANDLE_SPACING * 2`); images are framed on their edges.
pub(crate) const FRAME_PADDING: f64 = 4.0;

/// Which grip of a [`TransformBox`] a pointer holds. The letters are the
/// compass directions of the handle on the box when it is not turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
    Rotation,
}

impl Handle {
    const ALL: [Handle; 9] = [
        Handle::NW,
        Handle::NE,
        Handle::SW,
        Handle::SE,
        Handle::N,
        Handle::S,
        Handle::W,
        Handle::E,
        Handle::Rotation,
    ];

    pub fn is_east(self) -> bool {
        matches!(self, Handle::E | Handle::NE | Handle::SE)
    }

    pub fn is_west(self) -> bool {
        matches!(self, Handle::W | Handle::NW | Handle::SW)
    }

    pub fn is_north(self) -> bool {
        matches!(self, Handle::N | Handle::NE | Handle::NW)
    }

    pub fn is_south(self) -> bool {
        matches!(self, Handle::S | Handle::SE | Handle::SW)
    }

    /// Whether the handle is on a corner of the box.
    pub fn is_corner(self) -> bool {
        matches!(self, Handle::NE | Handle::NW | Handle::SE | Handle::SW)
    }
}

/// Excalidraw's `DEFAULT_COLLISION_THRESHOLD`, in view points.
const COLLISION_THRESHOLD: f64 = 8.0;

/// How far in view points a handle's centre keeps beyond the frame, which
/// itself keeps [`FRAME_PADDING`] from the box (Excalidraw's
/// `DEFAULT_TRANSFORM_HANDLE_SPACING`).
const HANDLE_SPACING: f64 = 2.0;
/// How far above the frame the rotation handle is centred beyond that
/// (Excalidraw's `ROTATION_RESIZE_HANDLE_GAP`).
const ROTATION_HANDLE_GAP: f64 = 16.0;

/// How big the handles are for one kind of pointer, in view points
/// (Excalidraw's `transformHandleSizes`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Grip {
    /// The side of the square that is drawn.
    pub visual: f64,
    /// The side of the square that takes hold: Apple's 44 points for a
    /// finger, which is a lot less precise than a mouse.
    pub hit: f64,
}

impl Grip {
    pub fn of(kind: PointerKind) -> Self {
        match kind {
            PointerKind::Mouse => Grip {
                visual: 8.0,
                hit: 8.0,
            },
            PointerKind::Pencil => Grip {
                visual: 16.0,
                hit: 16.0,
            },
            PointerKind::Finger => Grip {
                visual: 28.0,
                hit: 44.0,
            },
        }
    }
}

/// The least width (or height) of a box for the handles on its sides to be
/// offered: Excalidraw's five handles' worth at the size they are drawn at
/// for a mouse, so that they do not crowd the corners.
const SIDE_HANDLES_FROM: f64 = 5.0 * 8.0;

/// The Elements a transform acts on: those of the Selection that are shown,
/// not locked, and not labels, which follow their containers.
pub(crate) fn targets<'a>(
    scene: &'a Scene,
    selection: &'a Selection,
) -> impl Iterator<Item = Element<'a>> {
    scene.elements().filter(|e| {
        selection.contains(e.id())
            && !e.is_deleted()
            && !e.is_locked()
            && e.container_id().is_none()
    })
}

/// The box a Selection is resized and rotated as.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TransformBox {
    /// Where the box is when not turned.
    frame: Frame,
    /// Clockwise about the middle of `frame`.
    angle: f64,
    /// In view points: how far the selection frame (and so the handles) is
    /// from `frame`.
    padding: f64,
}

impl TransformBox {
    /// The box of the Selection; `None` if there is nothing to transform.
    /// A line or arrow alone has no box but its points (see
    /// [`crate::hit_test`]), and neither has anything but Elements to
    /// transform.
    pub fn of(scene: &Scene, selection: &Selection) -> Option<Self> {
        let mut targets = targets(scene, selection);
        let first = targets.next()?;
        let Some(second) = targets.next() else {
            if first.kind().is_linear() {
                return None;
            }
            return Some(Self {
                frame: Frame::of(&first),
                angle: first.angle(),
                // Images are framed on their edges.
                padding: if first.kind() == crate::scene::ElementType::Image {
                    0.0
                } else {
                    FRAME_PADDING
                },
            });
        };
        let frame = [first, second]
            .into_iter()
            .chain(targets)
            .map(|e| Frame::shown(&e))
            .reduce(Frame::union)?;
        Some(Self::around(frame))
    }

    /// The box of several Elements whose bounds together are `frame`.
    pub fn around(frame: Frame) -> Self {
        Self {
            frame,
            angle: 0.0,
            padding: FRAME_PADDING,
        }
    }

    pub fn frame(&self) -> Frame {
        self.frame
    }

    /// Whether `point` (in the Scene) is inside the selection frame, or
    /// within Excalidraw's `DEFAULT_COLLISION_THRESHOLD` (8 view points, at
    /// least 1 unit) of it, in a view at `zoom`.
    pub fn encloses(&self, point: Point, zoom: f64) -> bool {
        let margin = self.padding / zoom + (COLLISION_THRESHOLD / zoom).max(1.0);
        self.frame.contains_turned(point, self.angle, margin)
    }

    pub fn angle(&self) -> f64 {
        self.angle
    }

    /// The handles to draw in a view at `zoom`, with the centre of each in
    /// the Scene.
    pub fn handles(&self, zoom: f64) -> Vec<(Handle, Point)> {
        let (width, height) = (self.frame.width(), self.frame.height());
        let min_side = SIDE_HANDLES_FROM / zoom;
        Handle::ALL
            .into_iter()
            .filter(|handle| match handle {
                Handle::N | Handle::S => width.abs() > min_side,
                Handle::W | Handle::E => height.abs() > min_side,
                _ => true,
            })
            .map(|handle| (handle, self.to_scene(self.centre_of(handle, zoom))))
            .collect()
    }

    /// The handle a pointer of `kind` that went down at `point` takes hold of,
    /// if any: of those whose grip it is in, the nearest.
    pub fn grab(&self, point: Point, kind: PointerKind, zoom: f64) -> Option<Handle> {
        let grip = Grip::of(kind);
        let (x, y) = self.frame.unturned(point, self.angle);
        let reach = grip.hit / 2.0 / zoom;
        self.handles(zoom)
            .into_iter()
            .map(|(handle, _)| (handle, self.centre_of(handle, zoom)))
            .map(|(handle, (cx, cy))| (handle, (x - cx).abs(), (y - cy).abs()))
            .filter(|&(_, dx, dy)| dx <= reach && dy <= reach)
            .min_by(|a, b| a.1.hypot(a.2).total_cmp(&b.1.hypot(b.2)))
            .map(|(handle, ..)| handle)
    }

    /// How far `point` is from the part of the box that `handle` stands for
    /// (a corner or the middle of a side; nothing for rotation), in the
    /// Scene. A pointer that took hold of the handle is not on the box, but a
    /// little outside it; resizing follows the pointer less that distance, so
    /// that the box does not jump to the pointer.
    pub fn grab_offset(&self, handle: Handle, point: Point) -> Point {
        if handle == Handle::Rotation {
            return (0.0, 0.0);
        }
        let (x, y) = self.frame.unturned(point, self.angle);
        let (ax, ay) = self.anchor_of(handle);
        let (sin, cos) = self.angle.sin_cos();
        let (dx, dy) = (x - ax, y - ay);
        (dx * cos - dy * sin, dx * sin + dy * cos)
    }

    /// The part of the box `handle` stands for, when the box is not turned.
    fn anchor_of(&self, handle: Handle) -> Point {
        let (cx, cy) = self.frame.centre();
        let x = if handle.is_west() {
            self.frame.min.0
        } else if handle.is_east() {
            self.frame.max.0
        } else {
            cx
        };
        let y = if handle.is_north() {
            self.frame.min.1
        } else if handle.is_south() {
            self.frame.max.1
        } else {
            cy
        };
        (x, y)
    }

    /// The centre of the handle, when the box is not turned: just beyond
    /// the selection frame, or for rotation above it.
    fn centre_of(&self, handle: Handle, zoom: f64) -> Point {
        let spacing = (self.padding + HANDLE_SPACING) / zoom;
        let (x, y) = self.anchor_of(handle);
        let x = x + if handle.is_east() {
            spacing
        } else if handle.is_west() {
            -spacing
        } else {
            0.0
        };
        let y = y + if handle.is_south() {
            spacing
        } else if handle.is_north() {
            -spacing
        } else {
            0.0
        };
        if handle == Handle::Rotation {
            (
                x,
                self.frame.min.1 - (self.padding + HANDLE_SPACING + ROTATION_HANDLE_GAP) / zoom,
            )
        } else {
            (x, y)
        }
    }

    /// Turns a point of the unturned box into the Scene.
    fn to_scene(self, (x, y): Point) -> Point {
        let (cx, cy) = self.frame.centre();
        let (sin, cos) = self.angle.sin_cos();
        let (dx, dy) = (x - cx, y - cy);
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    }
}
