//! Basics shared by every Context and the cloud crate, written out because
//! the project keeps its dependencies at nothing (ADR-0009, ADR-0017).

pub mod base64;
pub mod clock;
pub mod device;
pub mod front_matter;
pub mod item_id;
pub mod json;
pub mod random;
pub mod sha256;
pub mod tag;
