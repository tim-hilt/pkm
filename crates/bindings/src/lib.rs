//! UniFFI surface of the core (ADR-0007). Foreign clients see exactly what is
//! exported here; the logic itself lives in `workingtitle-core`.

use std::sync::Arc;

uniffi::setup_scaffolding!("WorkingTitleCore");

/// Handle to the core's editor; see [`workingtitle_core::Editor`].
#[derive(uniffi::Object)]
pub struct Editor {
    inner: workingtitle_core::Editor,
}

#[uniffi::export]
impl Editor {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            inner: workingtitle_core::Editor::new(),
        }
    }

    /// Number of Elements in the Scene.
    pub fn element_count(&self) -> u64 {
        self.inner.element_count()
    }

    /// The paths to draw for the Scene; see [`workingtitle_core::Editor::geometry`].
    pub fn geometry(&self) -> Arc<Geometry> {
        Arc::new(Geometry {
            inner: self.inner.geometry(),
        })
    }
}

/// Zero-copy geometry (ADR-0007): the format is documented in
/// [`workingtitle_core::geometry`].
///
/// Only sizes and buffer addresses cross UniFFI; the data itself stays where
/// it is and the client reads it in place. An address is valid exactly as long
/// as the client holds this object, and the buffers never change.
#[derive(uniffi::Object)]
pub struct Geometry {
    inner: workingtitle_core::Geometry,
}

#[uniffi::export]
impl Geometry {
    /// Number of [`workingtitle_core::Part`] records at `parts_address`.
    pub fn part_count(&self) -> u64 {
        self.inner.parts().len() as u64
    }

    /// Size in bytes of one part record, so clients can check that their
    /// mirror of the record has the same layout.
    pub fn part_stride(&self) -> u64 {
        size_of::<workingtitle_core::Part>() as u64
    }

    pub fn parts_address(&self) -> u64 {
        self.inner.parts().as_ptr() as u64
    }

    /// Number of bytes (one verb each) at `verbs_address`.
    pub fn verb_count(&self) -> u64 {
        self.inner.verbs().len() as u64
    }

    pub fn verbs_address(&self) -> u64 {
        self.inner.verbs().as_ptr() as u64
    }

    /// Number of `f32` operands at `coords_address`.
    pub fn coord_count(&self) -> u64 {
        self.inner.coords().len() as u64
    }

    pub fn coords_address(&self) -> u64 {
        self.inner.coords().as_ptr() as u64
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_addresses_point_at_the_cores_buffers_without_copying() {
        let geometry = Editor::new().geometry();

        assert_eq!(geometry.part_count(), 1);
        assert_eq!(geometry.part_stride(), 36);
        assert_eq!(geometry.verb_count(), 9);
        assert_eq!(geometry.coord_count(), 34);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let parts = geometry.parts_address() as *const workingtitle_core::Part;
            assert_eq!((*parts).width, 2.0);
            let verbs = geometry.verbs_address() as *const u8;
            assert_eq!(*verbs, workingtitle_core::Verb::Move as u8);
            let coords = geometry.coords_address() as *const f32;
            assert_eq!(*coords, 82.0);
        }
    }
}
