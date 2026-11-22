//! Basics shared by every Context and the cloud crate, written out because
//! the project keeps its dependencies at nothing (ADR-0009, ADR-0017).

pub mod base64;
pub mod clock;
pub mod files;
pub mod front_matter;
pub mod highlight_colors;
pub mod images;
pub mod item_id;
pub mod json;
pub mod line_merge;
pub mod markdown;
pub mod markup;
pub mod random;
pub mod set_merge;
pub mod sha256;
pub mod tag;
pub mod tag_store;
pub mod tags;
