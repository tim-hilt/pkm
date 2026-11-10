//! A per-Book value read from disk once and then kept in memory.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The value of each Book read so far, by ID.
#[derive(Debug)]
pub(crate) struct BookCache<T> {
    known: Mutex<HashMap<String, Option<T>>>,
}

impl<T: Clone> BookCache<T> {
    pub fn new() -> Self {
        Self {
            known: Mutex::default(),
        }
    }

    /// The value of Book `id`: the one kept, else what `read` returns, which is
    /// then kept, unless a value was `set` meanwhile.
    pub fn get_or_read(&self, id: &str, read: impl FnOnce() -> Option<T>) -> Option<T> {
        if let Some(value) = self.lock().get(id) {
            return value.clone();
        }
        let read = read();
        self.lock().entry(id.to_owned()).or_insert(read).clone()
    }

    /// Keeps `value` as that of Book `id`.
    pub fn set(&self, id: &str, value: Option<T>) {
        self.lock().insert(id.to_owned(), value);
    }

    /// Forgets the value of Book `id`, so the next `get_or_read` reads it again.
    pub fn forget(&self, id: &str) {
        self.lock().remove(id);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Option<T>>> {
        self.known.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_value_is_read_once() {
        let cache = BookCache::new();
        let reads = Cell::new(0);
        let read = |value| {
            reads.set(reads.get() + 1);
            Some(value)
        };
        assert_eq!(cache.get_or_read("a", || read(1)), Some(1));
        assert_eq!(cache.get_or_read("a", || read(2)), Some(1));
        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn a_forgotten_value_is_read_again() {
        let cache = BookCache::new();
        cache.set("a", Some(5));
        assert_eq!(cache.get_or_read("a", || None), Some(5));
        cache.forget("a");
        assert_eq!(cache.get_or_read("a", || Some(7)), Some(7));
    }

    #[test]
    fn a_value_set_during_a_read_wins_over_the_read() {
        let cache = BookCache::new();
        let got = cache.get_or_read("a", || {
            cache.set("a", Some(9));
            Some(1)
        });
        assert_eq!(got, Some(9));
    }
}
