//! Merging what two Devices have kept of the same Book: its state file. The
//! Sync engine (`pkm-cloud`) asks for these bytes; the rules live here, beside
//! the files they decide about.
//!
//! Bytes that are no state are no Item: `merge` and `same_item` say none, so
//! Sync leaves them as they are instead of overwriting them. That includes the
//! Preferences older versions kept here: they now live in their own file
//! (`pkm_base::highlight_colors`, ADR-0034).

use crate::state::BookState;

/// What `local` and `remote` merge to, given the Item's bytes at the last Sync
/// (none if there was none); none if either isn't a state or they are of
/// different Books.
pub fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    let _ = base;
    if local == remote {
        return Some(local.to_vec());
    }
    let (left, right) = (BookState::decode(local)?, BookState::decode(remote)?);
    (left.id == right.id).then(|| left.merged(&right).encode())
}

/// Whether `bytes` are a state file.
pub fn is_item(bytes: &[u8]) -> bool {
    BookState::decode(bytes).is_some()
}

/// Whether `a` and `b` are the same Book's state. None if either isn't one,
/// or they are different Books: then they are left alone.
pub fn same_item(a: &[u8], b: &[u8]) -> Option<bool> {
    let (left, right) = (BookState::decode(a)?, BookState::decode(b)?);
    (left.id == right.id).then_some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE_LEFT: &str = r#"{"changes":{"highlight_comments":{"h":1}},"id":"aaaaaaaaaaaa","file_name":"a.pdf","sha256":"ff","imported":1,"markup":{"highlights":[{"id":"h","anchor":{"epub":{"start_spine":0,"start_offset":1,"end_spine":0,"end_offset":2}},"quoted_text":"t","color":1,"created":1,"comment":"left"}],"page_comments":[]},"progress":"e 0 1 5"}"#;

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
    fn the_old_preferences_file_is_no_state() {
        let preferences = br#"{"custom_colors":{},"default_color":"Green"}"#;
        assert!(!is_item(preferences));
        assert_eq!(merge(None, STATE_LEFT.as_bytes(), preferences), None);
    }
}
