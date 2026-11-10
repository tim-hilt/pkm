//! Building the JSON the state files hold, in the layout every saved Book and
//! Library already has: compact, members sorted by key.

use pkm_base::json::{Object, Value};

/// An object with `members`, sorted by key.
pub(crate) fn object<K: Into<String>>(members: impl IntoIterator<Item = (K, Value)>) -> Value {
    let mut members: Vec<(String, Value)> =
        members.into_iter().map(|(k, v)| (k.into(), v)).collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));
    let mut object = Object::new();
    for (key, value) in members {
        object.insert(key, value);
    }
    Value::Object(object)
}

pub(crate) fn number(n: impl Into<u64>) -> Value {
    Value::Number(n.into() as f64)
}

pub(crate) fn string(text: &str) -> Value {
    Value::String(text.to_owned())
}

/// The JSON document in `bytes`; none if they aren't one.
pub(crate) fn parse(bytes: &[u8]) -> Option<Value> {
    pkm_base::json::parse(std::str::from_utf8(bytes).ok()?).ok()
}
