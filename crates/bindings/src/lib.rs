//! UniFFI surface of the core (ADR-0007). Foreign clients see exactly what is
//! exported here; the logic itself lives in `workingtitle-core`.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

uniffi::setup_scaffolding!("WorkingTitleCore");

/// Why a file could not be opened as a Drawing, in words fit for a user.
#[derive(Debug, uniffi::Error)]
pub enum LoadError {
    Invalid { message: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let LoadError::Invalid { message } = self;
        f.write_str(message)
    }
}

impl From<workingtitle_core::LoadError> for LoadError {
    fn from(error: workingtitle_core::LoadError) -> Self {
        LoadError::Invalid {
            message: error.to_string(),
        }
    }
}

/// Handle to the core's editor; see [`workingtitle_core::Editor`].
///
/// UniFFI objects are shared, so the editor sits behind a lock; clients drive
/// it from one thread and never contend for it.
#[derive(uniffi::Object)]
pub struct Editor {
    inner: Mutex<workingtitle_core::Editor>,
}

impl Editor {
    fn editor(&self) -> MutexGuard<'_, workingtitle_core::Editor> {
        // A panic while holding the lock cannot leave the editor half-edited
        // in a way that matters more than carrying on does.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[uniffi::export]
impl Editor {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(workingtitle_core::Editor::new()),
        }
    }

    /// Replaces the Scene with the Drawing in `bytes` (a `.excalidraw` file's
    /// content); see [`workingtitle_core::Editor::load`].
    pub fn load(&self, bytes: Vec<u8>) -> Result<(), LoadError> {
        Ok(self.editor().load(&bytes)?)
    }

    /// The Scene as `.excalidraw` file content.
    pub fn save(&self) -> Vec<u8> {
        self.editor().save()
    }

    /// Number of Elements shown in the Scene.
    pub fn element_count(&self) -> u64 {
        self.editor().element_count()
    }

    /// The paths to draw for the Scene; see [`workingtitle_core::Editor::geometry`].
    pub fn geometry(&self) -> Arc<Geometry> {
        Arc::new(Geometry {
            inner: self.editor().geometry(),
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
        let editor = Editor::new();
        editor
            .load(
                br#"{"type":"excalidraw","elements":[
                    {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40}]}"#
                    .to_vec(),
            )
            .unwrap();
        let geometry = editor.geometry();

        assert_eq!(geometry.part_count(), 1);
        assert_eq!(geometry.part_stride(), 36);
        assert_eq!(geometry.verb_count(), 5);
        assert_eq!(geometry.coord_count(), 10);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let parts = geometry.parts_address() as *const workingtitle_core::Part;
            assert_eq!((*parts).width, 2.0);
            let verbs = geometry.verbs_address() as *const u8;
            assert_eq!(*verbs, workingtitle_core::Verb::Move as u8);
            let coords = geometry.coords_address() as *const f32;
            assert_eq!(*coords, 10.0);
        }
    }

    #[test]
    fn load_errors_carry_the_cores_message() {
        let error = Editor::new().load(b"[]".to_vec()).unwrap_err();

        assert!(error.to_string().contains("not an Excalidraw Drawing"));
    }

    #[test]
    fn loaded_bytes_are_saved_back() {
        let editor = Editor::new();
        let bytes = Editor::new().save();
        editor.load(bytes.clone()).unwrap();

        assert_eq!(editor.save(), bytes);
    }
}
