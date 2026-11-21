//! Building the JSON the state files hold, in the layout every saved Book and
//! Library already has: compact, members sorted by key (`pkm_base::json`).

pub(crate) use pkm_base::json::{number, object, parse_bytes as parse, string};
