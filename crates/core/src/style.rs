//! The current style: what new Elements look like (Excalidraw's
//! `currentItem...` app state).

/// Excalidraw's defaults (`DEFAULT_ELEMENT_PROPS`). The properties panel will
/// change them; until then they are what every new Element gets.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Style {
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: &'static str,
    pub stroke_width: f64,
    pub stroke_style: &'static str,
    pub roughness: f64,
    /// `0.0..=100.0`.
    pub opacity: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke_color: "#1e1e1e".into(),
            background_color: "transparent".into(),
            fill_style: "solid",
            stroke_width: 2.0,
            stroke_style: "solid",
            roughness: 1.0,
            opacity: 100.0,
        }
    }
}
