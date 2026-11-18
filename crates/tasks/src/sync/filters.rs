//! Merging what two Devices have kept of the same Filter (ADR-0024). Fields
//! merge as the Tasks' do (see the parent module); `places` merges per Task
//! ID: a place changed on one side keeps that side's value, one changed on
//! both takes the newer side's.

use super::{Kind, merge_kind};
use crate::store::is_filter;
use pkm_base::front_matter::Document;

const FILTER: Kind = Kind {
    parse,
    sets: &[],
    maps: &["places"],
};

/// What `local` and `remote` merge to, given the Filter's bytes at the last
/// Sync (none if there was none); none if either isn't a Filter or they
/// differ in id.
pub fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    merge_kind(&FILTER, base, local, remote)
}

/// Whether `bytes` are a Filter.
pub fn is_item(bytes: &[u8]) -> bool {
    parse(bytes).is_some()
}

/// Whether two Filters are one Filter: they share an id. None if either isn't one.
pub fn same_item(a: &[u8], b: &[u8]) -> Option<bool> {
    Some(parse(a)?.scalar("id") == parse(b)?.scalar("id"))
}

fn parse(bytes: &[u8]) -> Option<Document> {
    let file = Document::parse(std::str::from_utf8(bytes).ok()?)?;
    is_filter(&file).then_some(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "2024-01-01T00:00:00Z";
    const T1: &str = "2024-01-02T00:00:00Z";
    const T2: &str = "2024-01-03T00:00:00Z";

    fn filter(title: &str, query: &str, places: &[&str], updated: &str) -> Vec<u8> {
        let lines: String = places.iter().map(|p| format!("  - {p}\n")).collect();
        let places = if places.is_empty() {
            String::new()
        } else {
            format!("places:\n{lines}")
        };
        format!(
            "---\nid: aaaaaaaaaaaa\ntitle: {title}\nquery: {query}\nfavorite: false\nposition: 1\n{places}updated: {updated}\n---\n"
        )
        .into_bytes()
    }

    fn merged(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Document {
        let bytes = merge(base, local, remote).unwrap();
        // Whichever Device merges, the result is the same.
        assert_eq!(merge(base, remote, local).unwrap(), bytes);
        Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap()
    }

    fn places(file: &Document) -> Vec<&str> {
        file.list("places")
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect()
    }

    const A: &str = "bbbbbbbbbbbb";
    const B: &str = "cccccccccccc";

    #[test]
    fn different_fields_edited_on_two_devices_are_both_kept() {
        let base = filter("Work", "@work", &[], T0);
        let local = filter("Job", "@work", &[], T1);
        let remote = filter("Work", "@work & !@wait", &[], T2);
        let file = merged(Some(&base), &local, &remote);
        assert_eq!(file.scalar("title"), Some("Job"));
        assert_eq!(file.scalar("query"), Some("@work & !@wait"));
        assert_eq!(file.scalar("updated"), Some(T2));
    }

    #[test]
    fn the_same_field_edited_on_both_takes_the_newer_edit() {
        let base = filter("Work", "@work", &[], T0);
        let older = filter("Job", "@work", &[], T1);
        let newer = filter("Office", "@work", &[], T2);
        assert_eq!(
            merged(Some(&base), &older, &newer).scalar("title"),
            Some("Office")
        );
    }

    #[test]
    fn moves_of_different_tasks_on_two_devices_both_survive() {
        let base = filter("Work", "@work", &[&format!("{A} 1"), &format!("{B} 2")], T0);
        let local = filter("Work", "@work", &[&format!("{A} 3"), &format!("{B} 2")], T1);
        let remote = filter(
            "Work",
            "@work",
            &[&format!("{A} 1"), &format!("{B} 0.5")],
            T2,
        );
        let file = merged(Some(&base), &local, &remote);
        assert_eq!(places(&file), [format!("{A} 3"), format!("{B} 0.5")]);
    }

    #[test]
    fn a_task_moved_on_both_takes_the_newer_side() {
        let base = filter("Work", "@work", &[&format!("{A} 1")], T0);
        let older = filter("Work", "@work", &[&format!("{A} 3")], T1);
        let newer = filter("Work", "@work", &[&format!("{A} 0.5")], T2);
        assert_eq!(
            places(&merged(Some(&base), &older, &newer)),
            [format!("{A} 0.5")]
        );
    }

    #[test]
    fn a_place_dropped_on_one_side_stays_dropped_unless_the_other_moved_it() {
        let base = filter("Work", "@work", &[&format!("{A} 1"), &format!("{B} 2")], T0);
        let dropped = filter("Work", "@work", &[&format!("{B} 2")], T1);
        let moved = filter("Work", "@work", &[&format!("{A} 1"), &format!("{B} 5")], T2);
        assert_eq!(
            places(&merged(Some(&base), &dropped, &moved)),
            [format!("{B} 5")]
        );
        let moved_a = filter("Work", "@work", &[&format!("{A} 4"), &format!("{B} 2")], T2);
        assert_eq!(
            places(&merged(Some(&base), &dropped, &moved_a)),
            [format!("{A} 4"), format!("{B} 2")]
        );
    }

    #[test]
    fn the_last_place_dropped_leaves_no_property() {
        let base = filter("Work", "@work", &[&format!("{A} 1")], T0);
        let local = filter("Work", "@work", &[], T1);
        let remote = filter("Job", "@work", &[&format!("{A} 1")], T2);
        assert_eq!(merged(Some(&base), &local, &remote).get("places"), None);
    }

    #[test]
    fn without_a_base_differing_fields_take_the_newer_and_places_unite() {
        let local = filter("Job", "@a", &[&format!("{A} 1")], T1);
        let remote = filter("Office", "@b", &[&format!("{B} 2")], T2);
        let file = merged(None, &local, &remote);
        assert_eq!(file.scalar("title"), Some("Office"));
        assert_eq!(places(&file), [format!("{A} 1"), format!("{B} 2")]);
    }

    #[test]
    fn files_that_are_no_filters_are_no_items() {
        let good = filter("Work", "@work", &[], T0);
        let task = b"---\nid: aaaaaaaaaaaa\ntitle: Milk\nstate: active\n---\n";
        for bad in [
            &b"just text"[..],
            b"---\ntitle: x\n---\n",
            task,
            b"\xff\xfe",
        ] {
            assert!(!is_item(bad));
            assert_eq!(merge(None, &good, bad), None);
            assert_eq!(same_item(&good, bad), None);
        }
        assert!(is_item(&good));
    }

    #[test]
    fn filters_of_other_ids_neither_merge_nor_are_the_same() {
        let a = filter("Work", "@work", &[], T0);
        let b = String::from_utf8(a.clone())
            .unwrap()
            .replace("aaaaaaaaaaaa", "dddddddddddd");
        assert_eq!(merge(None, &a, b.as_bytes()), None);
        assert_eq!(same_item(&a, b.as_bytes()), Some(false));
    }

    #[test]
    fn the_sort_merges_as_one_field_by_the_newer_update() {
        let with_sort = |sort: &str, updated| {
            let text = String::from_utf8(filter("Work", "@work", &[], updated)).unwrap();
            text.replace("favorite:", &format!("sort: {sort}\nfavorite:"))
                .into_bytes()
        };
        let base = filter("Work", "@work", &[], T0);
        let older = with_sort("priority ascending", T1);
        let newer = with_sort("priority descending", T2);
        let file = merged(Some(&base), &older, &newer);
        assert_eq!(file.scalar("sort"), Some("priority descending"));
        let unknown = with_sort("created ascending", T2);
        let file = merged(Some(&base), &older, &unknown);
        assert_eq!(file.scalar("sort"), Some("created ascending"));
    }
}
