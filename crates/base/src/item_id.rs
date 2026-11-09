//! Item IDs (ADR-0019): the random identifier an Item keeps for life.
//!
//! Twelve lowercase base32 characters (`a-z`, `2-7`): 60 bits, enough that two
//! Items on one Device or across a user's Devices never meet by chance, short
//! enough to read in a link (`/drawings/k3j7x2m4q5ab`).

use crate::random::Entropy;
use std::collections::HashSet;
use std::fmt;

const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
const LENGTH: usize = 12;

/// A well-formed Item ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemId(String);

impl ItemId {
    /// A new, random Item ID.
    pub fn new(entropy: &mut Entropy) -> Self {
        let mut bits = entropy.bits();
        let text = (0..LENGTH)
            .map(|_| {
                let character = ALPHABET[(bits & 31) as usize];
                bits >>= 5;
                char::from(character)
            })
            .collect();
        Self(text)
    }

    /// The Item ID that `text` is, or `None` if it is not one.
    pub fn parse(text: &str) -> Option<Self> {
        let well_formed = text.len() == LENGTH && text.bytes().all(|b| ALPHABET.contains(&b));
        well_formed.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which Items need a new Item ID, given what they hold now, oldest first
/// (`""` or anything that is no Item ID: holds none): an Item without one
/// gets one, and of several that hold the same one all but the oldest do (a
/// copy made outside the app keeps the original's). `None` keeps what it has.
pub fn claim<S: AsRef<str>>(oldest_first: &[S]) -> Vec<Option<ItemId>> {
    let mut entropy = Entropy::from_system();
    let mut kept = HashSet::new();
    oldest_first
        .iter()
        .map(|held| {
            let first_holder = ItemId::parse(held.as_ref()).is_some_and(|id| kept.insert(id));
            (!first_holder).then(|| ItemId::new(&mut entropy))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_id_is_twelve_lowercase_base32_characters() {
        let id = ItemId::new(&mut Entropy::from_system());
        assert_eq!(id.as_str().len(), 12);
        assert!(
            id.as_str()
                .bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
        );
    }

    #[test]
    fn ten_thousand_new_ids_are_all_different() {
        let mut entropy = Entropy::from_system();
        let ids: HashSet<ItemId> = (0..10_000).map(|_| ItemId::new(&mut entropy)).collect();
        assert_eq!(ids.len(), 10_000);
    }

    #[test]
    fn parsing_takes_what_new_makes_and_nothing_else() {
        let id = ItemId::new(&mut Entropy::from_system());
        assert_eq!(ItemId::parse(id.as_str()), Some(id));
        assert!(ItemId::parse("abcdefghijkl").is_some());
        for bad in [
            "",
            "abcdefghijk",
            "abcdefghijklm",
            "ABCDEFGHIJKL",
            "abcdefghijk1",
            "abcdefghijk-",
            "abcdefghijké",
        ] {
            assert_eq!(ItemId::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn claiming_gives_ids_to_items_without_one_and_keeps_the_rest() {
        let claimed = claim(&["abcdefghijkl", "", "not an id", "234567abcdef"]);
        assert_eq!(claimed[0], None);
        assert_eq!(claimed[3], None);
        assert!(claimed[1].is_some() && claimed[2].is_some());
        assert_ne!(claimed[1], claimed[2]);
    }

    #[test]
    fn of_two_items_with_one_id_the_older_keeps_it() {
        let claimed = claim(&["abcdefghijkl", "abcdefghijkl", "abcdefghijkl"]);
        assert_eq!(claimed[0], None);
        let (second, third) = (claimed[1].as_ref().unwrap(), claimed[2].as_ref().unwrap());
        assert_ne!(second.as_str(), "abcdefghijkl");
        assert_ne!(second, third);
    }
}
