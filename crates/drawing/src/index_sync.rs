//! Keeping the fractional indices of Elements in step with their order, as
//! Excalidraw's `syncMovedIndices` does (`fractionalIndex.ts` at the pinned
//! commit).
//!
//! The order of the Elements is the order of the file; the `index` of each is
//! only valid if it lies strictly between those of its neighbours. After
//! Elements are moved, the moved ones get new keys between their unmoved
//! neighbours. Anything that is not valid after that (an Element that never
//! had an index, or an order that was invalid in the file) is then repaired
//! as well, which can change Elements that were not moved.
//!
//! Indices are `None` where an Element has none (or an empty one).

use crate::fractional_index::{self as keys, Invalid};

type Index<'a> = Option<&'a str>;

/// The keys that Elements in this order need to have valid indices after the
/// ones flagged in `moved` were moved: `(position, new index)`. Empty if
/// nothing can be done.
pub(crate) fn sync_moved(indices: &[Index], moved: &[bool]) -> Vec<(usize, String)> {
    let mut updates = moved_keys(indices, moved)
        .or_else(|_| invalid_keys(indices))
        .unwrap_or_default();
    // Excalidraw does not touch an Element whose key stays as it is.
    updates.retain(|(position, key)| indices[*position] != Some(key.as_str()));
    updates
}

/// A key for the Element at `position` that puts it between the nearest
/// Elements below and above it that have a valid one (or just above the one
/// below where those two are out of order); `None` if its own already does,
/// or no key can.
pub(crate) fn key_for(indices: &[Index], position: usize) -> Option<String> {
    let valid = |index: &&Index| index.is_some_and(|key| keys::validate(key).is_ok());
    let below = indices[..position]
        .iter()
        .rev()
        .find(valid)
        .copied()
        .flatten();
    let above = indices[position + 1..]
        .iter()
        .find(valid)
        .copied()
        .flatten();
    if is_valid(indices[position], below, above) {
        return None;
    }
    keys::key_between(below, above)
        .or_else(|_| keys::key_between(below, None))
        .ok()
}

/// Keys for the Elements without a valid index (new ones), between the
/// nearest Elements below and above them that have one, or just above the one
/// below where those two are out of order. Other keys are kept as they are.
pub(crate) fn fill_missing(indices: &[Index]) -> Vec<(usize, String)> {
    let has_key = |i: usize| indices[i].is_some_and(|key| keys::validate(key).is_ok());
    let mut updates = Vec::new();
    let mut i = 0;
    while i < indices.len() {
        if has_key(i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < indices.len() && !has_key(i) {
            i += 1;
        }
        let below = start.checked_sub(1).and_then(|b| indices[b]);
        let above = indices.get(i).copied().flatten();
        let new = keys::keys_between(below, above, i - start)
            .or_else(|_| keys::keys_between(below, None, i - start))
            .unwrap_or_default();
        updates.extend((start..i).zip(new));
    }
    updates
}

/// New keys for the moved Elements alone; fails if the result is not valid.
fn moved_keys(indices: &[Index], moved: &[bool]) -> Result<Vec<(usize, String)>, Invalid> {
    let updates = generate(indices, &moved_groups(moved))?;
    if is_valid_order(indices.len(), |i| key_at(indices, &updates, i)) {
        Ok(updates)
    } else {
        Err(Invalid)
    }
}

/// New keys for every Element whose index is not valid in its place.
fn invalid_keys(indices: &[Index]) -> Result<Vec<(usize, String)>, Invalid> {
    generate(indices, &invalid_groups(indices))
}

/// The index at `position` once `updates` are made.
fn key_at<'a>(indices: &[Index<'a>], updates: &'a [(usize, String)], position: usize) -> Index<'a> {
    updates
        .iter()
        .find(|(p, _)| *p == position)
        .map(|(_, key)| key.as_str())
        .or(indices.get(position).copied().flatten())
}

fn is_valid_order<'a>(len: usize, at: impl Fn(usize) -> Index<'a>) -> bool {
    (0..len).all(|i| is_valid(at(i), i.checked_sub(1).and_then(&at), at(i + 1)))
}

/// Whether `index` is a key that lies between its neighbours.
fn is_valid(index: Index, below: Index, above: Index) -> bool {
    let Some(index) = index else {
        return false;
    };
    keys::validate(index).is_ok()
        && below.is_none_or(|b| b < index)
        && above.is_none_or(|a| index < a)
}

/// The runs of moved Elements, each as the position below it (-1 at the
/// bottom), the positions of the run, and the position above it (the length
/// at the top).
fn moved_groups(moved: &[bool]) -> Vec<Vec<isize>> {
    let mut groups = Vec::new();
    let mut i = 0;
    while i < moved.len() {
        if !moved[i] {
            i += 1;
            continue;
        }
        let mut group = vec![i as isize - 1];
        while i < moved.len() && moved[i] {
            group.push(i as isize);
            i += 1;
        }
        group.push(i as isize);
        groups.push(group);
    }
    groups
}

/// The runs of Elements with an invalid index, in the same form as
/// [`moved_groups`] but with the nearest valid bounds, which need not be
/// right next to the run.
fn invalid_groups(indices: &[Index]) -> Vec<Vec<isize>> {
    let len = indices.len() as isize;
    let at = |i: isize| -> Index {
        usize::try_from(i)
            .ok()
            .and_then(|i| indices.get(i).copied().flatten())
    };

    // The nearest index below `i` that is above the bound found so far.
    let lower = |i: isize, bound_at: isize| -> (Index, isize) {
        let bound = at(bound_at);
        let candidate = at(i - 1);
        if candidate.is_some() && bound.is_none_or(|b| candidate > Some(b)) {
            (candidate, i - 1)
        } else {
            (bound, bound_at)
        }
    };
    // The nearest index above `i` that is above the bound found so far.
    let upper = |i: isize, bound_at: isize| -> (Index, isize) {
        let bound = at(bound_at);
        if bound.is_some() && i < bound_at {
            return (bound, bound_at);
        }
        let mut j = bound_at;
        loop {
            j += 1;
            if j >= len {
                return (None, j);
            }
            let candidate = at(j);
            if candidate.is_some() && bound.is_none_or(|b| candidate > Some(b)) {
                return (candidate, j);
            }
        }
    };

    let mut groups = Vec::new();
    let (mut lower_at, mut upper_at) = (-1, 0);
    let mut i = 0;
    while i < len {
        let (lower_bound, lower_pos) = lower(i, lower_at);
        let (upper_bound, upper_pos) = upper(i, upper_at);
        (lower_at, upper_at) = (lower_pos, upper_pos);
        if is_valid(at(i), lower_bound, upper_bound) {
            i += 1;
            continue;
        }
        let mut group = vec![lower_at, i];
        i += 1;
        while i < len {
            let (next_lower, next_lower_at) = lower(i, lower_at);
            let (next_upper, next_upper_at) = upper(i, upper_at);
            if is_valid(at(i), next_lower, next_upper) {
                break;
            }
            (lower_at, upper_at) = (next_lower_at, next_upper_at);
            group.push(i);
            i += 1;
        }
        group.push(upper_at);
        groups.push(group);
    }
    groups
}

/// Keys for the Elements of each group, between the bounds that begin and
/// end it.
fn generate(indices: &[Index], groups: &[Vec<isize>]) -> Result<Vec<(usize, String)>, Invalid> {
    let at = |i: isize| -> Index {
        usize::try_from(i)
            .ok()
            .and_then(|i| indices.get(i).copied().flatten())
    };
    let mut updates = Vec::new();
    for group in groups {
        let [lower, positions @ .., upper] = &group[..] else {
            continue;
        };
        let new = keys::keys_between(at(*lower), at(*upper), positions.len())?;
        updates.extend(positions.iter().map(|&p| p as usize).zip(new));
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync(indices: &[Index], moved: &[usize]) -> Vec<(usize, String)> {
        let flags: Vec<bool> = (0..indices.len()).map(|i| moved.contains(&i)).collect();
        sync_moved(indices, &flags)
    }

    fn keys(updates: &[(usize, &str)]) -> Vec<(usize, String)> {
        updates.iter().map(|&(p, k)| (p, k.to_owned())).collect()
    }

    #[test]
    fn a_moved_element_gets_a_key_between_its_new_neighbours() {
        let updates = sync(&[Some("a0"), Some("a2"), Some("a1")], &[1]);

        assert_eq!(updates, keys(&[(1, "a0V")]));
    }

    #[test]
    fn a_moved_element_at_the_top_follows_the_last_one() {
        let updates = sync(&[Some("a1"), Some("a2"), Some("a0")], &[2]);

        assert_eq!(updates, keys(&[(2, "a3")]));
    }

    #[test]
    fn a_moved_element_at_the_bottom_precedes_the_first_one() {
        let updates = sync(&[Some("a2"), Some("a0"), Some("a1")], &[0]);

        assert_eq!(updates, keys(&[(0, "Zz")]));
    }

    #[test]
    fn a_run_of_moved_elements_gets_keys_in_order() {
        let updates = sync(&[Some("a0"), Some("a5"), Some("a6"), Some("a1")], &[1, 2]);

        assert_eq!(updates, keys(&[(1, "a0G"), (2, "a0V")]));
    }

    #[test]
    fn separate_runs_are_given_keys_separately() {
        let updates = sync(&[Some("a3"), Some("a1"), Some("a0"), Some("a2")], &[0, 2]);

        assert_eq!(updates, keys(&[(0, "a0"), (2, "a1V")]));
    }

    #[test]
    fn elements_that_keep_their_key_are_left_alone() {
        assert!(sync(&[Some("a0"), Some("a1"), Some("a2")], &[1]).is_empty());
    }

    #[test]
    fn elements_without_an_index_are_given_one_as_well() {
        let updates = sync(&[None, Some("a1"), None], &[1]);

        assert_eq!(updates, keys(&[(0, "a0"), (2, "a2")]));
    }

    #[test]
    fn a_scene_without_any_index_gets_a_sequence() {
        let updates = sync(&[None, None, None], &[2]);

        assert_eq!(updates, keys(&[(0, "a0"), (1, "a1"), (2, "a2")]));
    }

    #[test]
    fn an_inserted_element_gets_a_key_between_its_neighbours() {
        let indices = [Some("a0"), None, Some("a1")];
        assert_eq!(key_for(&indices, 1).as_deref(), Some("a0V"));
        assert_eq!(key_for(&[None], 0).as_deref(), Some("a0"));
    }

    #[test]
    fn an_inserted_element_whose_key_fits_keeps_it() {
        assert_eq!(key_for(&[Some("a0"), Some("a0G"), Some("a1")], 1), None);
        assert_eq!(
            key_for(&[Some("a0"), Some("a5"), Some("a1")], 1).as_deref(),
            Some("a0V")
        );
    }

    #[test]
    fn missing_keys_are_filled_between_their_neighbours() {
        let updates = fill_missing(&[None, Some("a1"), None, None, Some("a2"), None]);

        assert_eq!(
            updates,
            keys(&[(0, "a0"), (2, "a1G"), (3, "a1V"), (5, "a3")])
        );
    }

    #[test]
    fn filling_missing_keys_leaves_keys_out_of_order_alone() {
        let updates = fill_missing(&[Some("a2"), None, Some("a0")]);

        assert_eq!(updates, keys(&[(1, "a3")]));
    }

    #[test]
    fn indices_that_are_out_of_order_in_the_file_are_repaired() {
        // `a2` is above `a1` in the array but was never moved by the user.
        let updates = sync(&[Some("a0"), Some("a2"), Some("a1"), Some("a3")], &[3]);

        assert_eq!(updates, keys(&[(2, "a2V")]));
    }
}
