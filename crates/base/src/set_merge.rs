//! Three-way merge of a property that is a set of items, as Tags are on
//! Tasks and Notes: additions on either side are kept, an item either side
//! removed since the base is gone.

/// `winner`'s items then `loser`'s new ones, each once. Kept: an item on both
/// sides, or one added on a side (every item, without a `base`).
pub fn merge(base: Option<&[String]>, winner: &[String], loser: &[String]) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn additions_unite_and_a_removal_wins_over_an_unchanged_side() {
        let base = v(&["a", "b"]);
        assert_eq!(
            merge(Some(&base), &v(&["a", "b", "c"]), &v(&["a", "b", "d"])),
            ["a", "b", "c", "d"]
        );
        assert_eq!(
            merge(Some(&base), &v(&["a"]), &v(&["a", "b", "d"])),
            ["a", "d"]
        );
    }

    #[test]
    fn without_a_base_the_sides_are_united() {
        assert_eq!(merge(None, &v(&["a"]), &v(&["b", "a"])), ["a", "b"]);
    }
}
