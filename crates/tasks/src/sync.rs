//! Merging what two Devices have kept of the same Task (ADR-0021). The Sync
//! engine (`pkm-cloud`) asks for these bytes; the rules live here, beside the
//! file format they decide about.
//!
//! Three-way against the file at the last Sync: a property changed on one side
//! takes that side, `tags` and `blocked-by` merge as sets, and one changed on
//! both sides takes the side with the newer `updated`. Without a base,
//! differing properties take the newer side's. Bytes that are no Task are no
//! Item: `merge` and `same_item` say none.

use crate::store::is_task;
use pkm_base::front_matter::{Document, Value};
use std::cmp::Ordering;

/// Properties that are sets of items: both sides' additions are kept.
const SETS: [&str; 2] = ["tags", "blocked-by"];
const UPDATED: &str = "updated";

/// What `local` and `remote` merge to, given the Item's bytes at the last Sync
/// (none if there was none); none if either isn't a Task or they differ in id.
pub fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    if local == remote {
        return Some(local.to_vec());
    }
    let (left, right) = (parse(local)?, parse(remote)?);
    if left.scalar("id") != right.scalar("id") {
        return None;
    }
    // An unreadable base is as good as none.
    let base = base.and_then(parse);
    // Both Devices must pick the same winner, whichever side they call local.
    let newer = match updated(&left).cmp(updated(&right)) {
        Ordering::Equal => local.max(remote) == local,
        order => order == Ordering::Greater,
    };
    let (winner, loser) = if newer {
        (&left, &right)
    } else {
        (&right, &left)
    };
    let mut merged = winner.clone();
    let keys: Vec<&str> = winner.keys().chain(loser.keys()).collect();
    for key in keys {
        let sides = Sides {
            base: base.as_ref().map(|b| b.get(key)),
            winner: winner.get(key),
            loser: loser.get(key),
        };
        match sides.merged(key) {
            Some(value) => merged.set(key, value),
            None => merged.remove(key),
        }
    }
    merged.body = pick(base.as_ref().map(|b| &b.body), &winner.body, &loser.body).clone();
    Some(merged.render().into_bytes())
}

/// Whether `bytes` are a Task.
pub fn is_item(bytes: &[u8]) -> bool {
    parse(bytes).is_some()
}

/// Whether two Tasks are one Task: they share an id. None if either isn't a Task.
pub fn same_item(a: &[u8], b: &[u8]) -> Option<bool> {
    Some(parse(a)?.scalar("id") == parse(b)?.scalar("id"))
}

fn parse(bytes: &[u8]) -> Option<Document> {
    let file = Document::parse(std::str::from_utf8(bytes).ok()?)?;
    is_task(&file).then_some(file)
}

/// The value of one field of the winner and loser: the winner's, unless only
/// the loser changed it since `base`.
fn pick<'a, T: PartialEq>(base: Option<&T>, winner: &'a T, loser: &'a T) -> &'a T {
    if base == Some(winner) { loser } else { winner }
}

fn items(value: Option<&Value>) -> &[String] {
    match value {
        Some(Value::List(items)) => items,
        _ => &[],
    }
}

fn updated(file: &Document) -> &str {
    file.scalar(UPDATED).unwrap_or("")
}

/// One property on the three versions; `base` is `None` without a base file.
struct Sides<'a> {
    base: Option<Option<&'a Value>>,
    /// The side with the newer `updated`.
    winner: Option<&'a Value>,
    loser: Option<&'a Value>,
}

impl Sides<'_> {
    fn merged(&self, key: &str) -> Option<Value> {
        if key == UPDATED {
            return self.winner.cloned(); // the newer one
        }
        if SETS.contains(&key) {
            return Some(Value::List(
                self.merged_set(items(self.winner), items(self.loser)),
            ));
        }
        pick(self.base.as_ref(), &self.winner, &self.loser).cloned()
    }

    fn merged_set(&self, winner: &[String], loser: &[String]) -> Vec<String> {
        let base = self.base.map(items);
        // Kept: on both sides, or added on one (everything, without a base).
        let kept = |item: &String, other: &[String]| {
            other.contains(item) || base.is_none_or(|b| !b.contains(item))
        };
        let mut items: Vec<String> = winner.iter().filter(|i| kept(i, loser)).cloned().collect();
        for item in loser {
            if kept(item, winner) && !items.contains(item) {
                items.push(item.clone());
            }
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(title: &str, priority: u8, tags: &[&str], blocked: &[&str], updated: &str) -> Vec<u8> {
        let list = |key: &str, items: &[&str]| {
            let lines: String = items.iter().map(|i| format!("  - {i}\n")).collect();
            if items.is_empty() {
                String::new()
            } else {
                format!("{key}:\n{lines}")
            }
        };
        format!(
            "---\nid: aaaaaaaaaaaa\ntitle: {title}\npriority: {priority}\nstate: active\norder: 1\n{}{}updated: {updated}\n---\n",
            list("tags", tags),
            list("blocked-by", blocked),
        )
        .into_bytes()
    }

    const T0: &str = "2024-01-01T00:00:00Z";
    const T1: &str = "2024-01-02T00:00:00Z";
    const T2: &str = "2024-01-03T00:00:00Z";

    fn merged(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Document {
        let bytes = merge(base, local, remote).unwrap();
        // Whichever Device merges, the result is the same.
        assert_eq!(merge(base, remote, local).unwrap(), bytes);
        Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap()
    }

    #[test]
    fn different_fields_edited_on_two_devices_are_both_kept() {
        let base = task("Milk", 4, &[], &[], T0);
        let local = task("Oat milk", 4, &[], &[], T1);
        let remote = task("Milk", 1, &[], &[], T2);
        let file = merged(Some(&base), &local, &remote);
        assert_eq!(file.scalar("title"), Some("Oat milk"));
        assert_eq!(file.scalar("priority"), Some("1"));
        assert_eq!(file.scalar("updated"), Some(T2));
    }

    #[test]
    fn the_same_field_edited_on_both_takes_the_newer_edit() {
        let base = task("Milk", 4, &[], &[], T0);
        let older = task("Oat milk", 4, &[], &[], T1);
        let newer = task("Soy milk", 4, &[], &[], T2);
        assert_eq!(
            merged(Some(&base), &older, &newer).scalar("title"),
            Some("Soy milk")
        );
    }

    #[test]
    fn tags_and_blocked_by_added_on_two_devices_are_both_kept() {
        let base = task("Milk", 4, &["home"], &["bbbbbbbbbbbb"], T0);
        let local = task(
            "Milk",
            4,
            &["home", "shop"],
            &["bbbbbbbbbbbb", "cccccccccccc"],
            T1,
        );
        let remote = task(
            "Milk",
            4,
            &["home", "food"],
            &["bbbbbbbbbbbb", "dddddddddddd"],
            T2,
        );
        let file = merged(Some(&base), &local, &remote);
        let sorted = |key| {
            let mut items = file.list(key).unwrap().to_vec();
            items.sort();
            items
        };
        assert_eq!(sorted("tags"), ["food", "home", "shop"]);
        assert_eq!(
            sorted("blocked-by"),
            ["bbbbbbbbbbbb", "cccccccccccc", "dddddddddddd"]
        );
    }

    #[test]
    fn a_tag_removed_on_one_device_stays_removed() {
        let base = task("Milk", 4, &["home", "shop"], &[], T0);
        let local = task("Milk", 4, &["home"], &[], T1);
        let remote = task("Milk", 1, &["home", "shop"], &[], T2);
        let file = merged(Some(&base), &local, &remote);
        assert_eq!(file.list("tags"), Some(&["home".to_owned()][..]));
        assert_eq!(file.scalar("priority"), Some("1"));
    }

    #[test]
    fn without_a_base_differing_fields_take_the_newer_and_sets_unite() {
        let local = task("Oat milk", 4, &["a"], &[], T1);
        let remote = task("Soy milk", 2, &["b"], &[], T2);
        let file = merged(None, &local, &remote);
        assert_eq!(file.scalar("title"), Some("Soy milk"));
        assert_eq!(file.scalar("priority"), Some("2"));
        assert_eq!(file.list("tags").unwrap().len(), 2);
    }

    #[test]
    fn the_body_follows_the_side_that_changed_it() {
        let base = b"---\nid: aaaaaaaaaaaa\ntitle: Milk\nstate: active\nupdated: 2024-01-01T00:00:00Z\n---\nOne\n".to_vec();
        let local = b"---\nid: aaaaaaaaaaaa\ntitle: Milk\nstate: active\nupdated: 2024-01-05T00:00:00Z\n---\nOne\n".to_vec();
        let remote = b"---\nid: aaaaaaaaaaaa\ntitle: Milk\nstate: active\nupdated: 2024-01-02T00:00:00Z\n---\nTwo\n".to_vec();
        assert_eq!(merged(Some(&base), &local, &remote).body, "Two\n");
    }

    #[test]
    fn unknown_properties_survive() {
        let base = task("Milk", 4, &[], &[], T0);
        let mut local = String::from_utf8(task("Milk", 4, &[], &[], T1)).unwrap();
        local = local.replace("order: 1\n", "order: 1\nx-extra: kept\n");
        let remote = task("Milk", 2, &[], &[], T2);
        let file = merged(Some(&base), local.as_bytes(), &remote);
        assert_eq!(file.scalar("x-extra"), Some("kept"));
    }

    #[test]
    fn files_that_are_no_tasks_are_no_items() {
        let good = task("Milk", 4, &[], &[], T0);
        for bad in [
            &b"just text"[..],
            b"---\ntitle: x\n---\n",
            b"---\nid: a\nstate: weird\ntitle: t\n---\n",
            b"\xff\xfe",
        ] {
            assert!(!is_item(bad));
            assert_eq!(merge(None, &good, bad), None);
            assert_eq!(merge(None, bad, &good), None);
            assert_eq!(same_item(&good, bad), None);
        }
        assert!(is_item(&good));
    }

    #[test]
    fn tasks_of_other_ids_neither_merge_nor_are_the_same() {
        let a = task("Milk", 4, &[], &[], T0);
        let b = String::from_utf8(a.clone())
            .unwrap()
            .replace("aaaaaaaaaaaa", "bbbbbbbbbbbb");
        assert_eq!(merge(None, &a, b.as_bytes()), None);
        assert_eq!(same_item(&a, b.as_bytes()), Some(false));
        assert_eq!(same_item(&a, &a), Some(true));
    }
}
