//! Randomness for new Elements and Items: their ids, their seeds (which fix how
//! hand-drawn they look, ADR-0004) and their version nonces.
//!
//! Nothing here needs to be unpredictable, only different from one Element
//! and one editor to the next, so a small generator seeded from the system
//! does; `pkm-base` has no dependencies to take one from (ADR-0009).

use crate::clock::now_ms;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

/// The characters of Excalidraw's ids (nanoid's alphabet).
const ID_ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
/// Excalidraw's ids (`randomId`) are nanoid's default length.
const ID_LENGTH: usize = 21;

/// SplitMix64: tiny, fast, and good enough for ids and seeds.
#[derive(Debug)]
pub struct Entropy {
    state: u64,
}

impl Entropy {
    /// A generator that differs from every other, including those made at
    /// the same instant: the hasher's keys are random per process and change
    /// with every `RandomState`.
    pub fn from_system() -> Self {
        // Without an operating system `RandomState` has fixed keys and the
        // clock ticks in milliseconds, so a counter keeps editors apart.
        static MADE: AtomicU64 = AtomicU64::new(0);
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(now_ms().to_bits());
        hasher.write_u64(MADE.fetch_add(1, Ordering::Relaxed));
        Self {
            state: hasher.finish(),
        }
    }

    /// 64 random bits.
    pub(crate) fn bits(&mut self) -> u64 {
        self.next_u64()
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// An integer in `0..2^31`, like Excalidraw's `randomInteger`: what seeds
    /// and version nonces are.
    pub fn integer(&mut self) -> f64 {
        (self.next_u64() >> 33) as f64
    }

    /// A new Element id.
    pub fn id(&mut self) -> String {
        let mut bits = self.next_u64();
        (0..ID_LENGTH)
            .map(|i| {
                // 64 bits give ten characters of six; renew before they run out.
                if i > 0 && i % 10 == 0 {
                    bits = self.next_u64();
                }
                let character = ID_ALPHABET[(bits & 63) as usize];
                bits >>= 6;
                char::from(character)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_below_two_to_the_31() {
        let mut entropy = Entropy::from_system();
        assert!((0..1000).all(|_| entropy.integer() < 2_147_483_648.0));
    }

    #[test]
    fn ids_have_excalidraws_length_and_alphabet() {
        let id = Entropy::from_system().id();
        assert_eq!(id.len(), 21);
        assert!(id.bytes().all(|b| ID_ALPHABET.contains(&b)));
    }
}
