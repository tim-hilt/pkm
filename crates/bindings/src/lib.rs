//! UniFFI surface of the core (ADR-0007). Foreign clients see exactly what is
//! exported here; the logic itself lives in `workingtitle-core`.

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
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
