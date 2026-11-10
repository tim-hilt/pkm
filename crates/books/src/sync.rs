//! Merging what two Devices have kept of the same Book: its state file, or
//! the Preferences. The Sync engine (`pkm-cloud`) asks for these bytes; the
//! rules live here, beside the files they decide about.
//!
//! Bytes that are no state and no Preferences are no Item: `merge` and
//! `same_item` say none, so Sync leaves them as they are instead of
//! overwriting them.

use crate::json;
use crate::preferences::Preferences;
use crate::state::BookState;

enum Stored {
    Book(Box<BookState>),
    Preferences(Preferences),
}

fn decode(bytes: &[u8]) -> Option<Stored> {
    if let Some(state) = BookState::decode(bytes) {
        return Some(Stored::Book(Box::new(state)));
    }
    let json = json::parse(bytes)?;
    let preferences = json.get("custom_colors").is_some() || json.get("default_color").is_some();
    preferences.then(|| Stored::Preferences(Preferences::decode(Some(bytes))))
}

/// What `local` and `remote` merge to, given the Item's bytes at the last Sync
/// (none if there was none); none if either isn't a state or the Preferences,
/// or they are not of the same kind.
pub fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    let _ = base;
    if local == remote {
        return Some(local.to_vec());
    }
    match (decode(local)?, decode(remote)?) {
        (Stored::Book(left), Stored::Book(right)) if left.id == right.id => {
            Some(left.merged(&right).encode())
        }
        (Stored::Preferences(left), Stored::Preferences(right)) => {
            Some(left.merged(&right).encode())
        }
        _ => None,
    }
}

/// Whether `bytes` are a state file or the Preferences.
pub fn is_item(bytes: &[u8]) -> bool {
    decode(bytes).is_some()
}

/// Whether `a` and `b` are the same Book state or both Preferences. None if
/// either isn't one, or they are different kinds: then they are left alone.
pub fn same_item(a: &[u8], b: &[u8]) -> Option<bool> {
    match (decode(a)?, decode(b)?) {
        (Stored::Book(left), Stored::Book(right)) => (left.id == right.id).then_some(true),
        (Stored::Preferences(_), Stored::Preferences(_)) => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE_LEFT: &str = r#"{"changes":{"highlight_comments":{"h":1}},"id":"aaaaaaaaaaaa","file_name":"a.pdf","sha256":"ff","imported":1,"markup":{"highlights":[{"id":"h","anchor":{"epub":{"start_spine":0,"start_offset":1,"end_spine":0,"end_offset":2}},"quoted_text":"t","color":1,"created":1,"comment":"left"}],"page_comments":[]},"progress":"e 0 1 5"}"#;
    const PREFERENCES_LEFT: &str =
        r#"{"custom_colors":{"0000000000001-a":32896},"default_color":"Green"}"#;
    const PREFERENCES_RIGHT: &str =
        r#"{"custom_colors":{"0000000000002-b":9849600},"default_color":"Blue"}"#;

    #[test]
    fn states_merge_and_bytes_that_are_no_state_are_left_alone() {
        let right = STATE_LEFT
            .replace(r#""left""#, r#""right""#)
            .replace(r#""h":1"#, r#""h":2"#);
        let merged = merge(None, STATE_LEFT.as_bytes(), right.as_bytes()).unwrap();
        let merged = String::from_utf8(merged).unwrap();
        assert!(merged.contains(r#""comment":"right""#));
        assert!(!merged.contains(r#""comment":"left""#));
        assert_eq!(merge(None, STATE_LEFT.as_bytes(), b"garbage"), None);
        assert_eq!(same_item(STATE_LEFT.as_bytes(), b"garbage"), None);
        assert!(is_item(STATE_LEFT.as_bytes()));
        assert!(!is_item(b"garbage"));
    }

    #[test]
    fn preferences_merge_and_their_removals_win() {
        let merged = merge(
            None,
            PREFERENCES_LEFT.as_bytes(),
            PREFERENCES_RIGHT.as_bytes(),
        )
        .unwrap();
        let merged = String::from_utf8(merged).unwrap();
        assert!(merged.contains("0000000000001-a"));
        assert!(merged.contains("0000000000002-b"));
        assert!(merged.contains(r#""default_color":"Blue""#));
        assert_eq!(
            same_item(PREFERENCES_LEFT.as_bytes(), PREFERENCES_RIGHT.as_bytes()),
            Some(true)
        );
        assert_eq!(
            merge(None, STATE_LEFT.as_bytes(), PREFERENCES_LEFT.as_bytes()),
            None
        );
    }
}
