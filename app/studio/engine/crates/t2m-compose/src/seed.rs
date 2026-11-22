//! Seeded randomness.
//!
//! Every part of every section draws from its own stream, derived from the
//! song seed, the track id, the section id and a reroll count. That is what
//! makes "reroll only the bass in the chorus" possible and repeatable.
//!
//! The derivation is fixed: FNV-1a (64-bit) over the seed's eight bytes
//! (little-endian), the track id, the section id and the reroll count's four
//! bytes (little-endian), each followed by the byte 0x1F. Four such hashes,
//! each with one more domain byte (0, 1, 2, 3), form the 32-byte ChaCha8 key.
//! Only whole-number draws are offered, so results do not depend on any
//! library's sampling method or on floating-point behaviour.

use std::collections::BTreeMap;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{Rng as _, SeedableRng};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const SEPARATOR: u8 = 0x1f;

/// How many times each (track, section) has been rerolled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rerolls(pub BTreeMap<(String, String), u32>);

impl Rerolls {
    /// The reroll count for one part of one section; 0 when never rerolled.
    pub fn count(&self, track: &str, section: &str) -> u32 {
        let key = (track.to_string(), section.to_string());
        self.0.get(&key).copied().unwrap_or(0)
    }

    /// Asks for a fresh take of one part of one section.
    pub fn bump(&mut self, track: &str, section: &str) {
        let key = (track.to_string(), section.to_string());
        let count = self.0.entry(key).or_insert(0);
        *count = count.saturating_add(1);
    }
}

fn fnv(parts: &[&[u8]]) -> u64 {
    let mut hash = FNV_OFFSET;
    for part in parts {
        for byte in part.iter().chain(&[SEPARATOR]) {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

/// A stream of whole-number random draws.
pub(crate) struct Rng(ChaCha8Rng);

/// The stream for one part of one section.
pub(crate) fn sub_rng(seed: u64, track: &str, section: &str, reroll: u32) -> Rng {
    let mut key = [0u8; 32];
    for (domain, chunk) in key.as_chunks_mut::<8>().0.iter_mut().enumerate() {
        let hash = fnv(&[
            &seed.to_le_bytes(),
            track.as_bytes(),
            section.as_bytes(),
            &reroll.to_le_bytes(),
            &[domain as u8],
        ]);
        chunk.copy_from_slice(&hash.to_le_bytes());
    }
    Rng(ChaCha8Rng::from_seed(key))
}

impl Rng {
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    /// A number from 0 up to but not including `n`, each equally likely; 0 when `n` is 0.
    pub(crate) fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        // Reject the top sliver of the range so that every result is equally likely.
        let span = u64::from(n);
        let limit = u64::MAX - u64::MAX % span;
        loop {
            let draw = self.0.next_u64();
            if draw < limit {
                return (draw % span) as u32;
            }
        }
    }

    /// A number from `low` to `high` inclusive; `low` when the range is empty.
    pub(crate) fn between(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let width = u32::try_from(high.saturating_sub(low).saturating_add(1)).unwrap_or(u32::MAX);
        low.saturating_add(i64::from(self.below(width)))
    }

    /// True `permille` times in a thousand.
    pub(crate) fn chance(&mut self, permille: u32) -> bool {
        self.below(1000) < permille
    }

    /// One item, each equally likely; `None` from an empty list.
    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let count = u32::try_from(items.len()).unwrap_or(u32::MAX);
        items.get(self.below(count) as usize)
    }
}

#[cfg(test)]
#[path = "seed_tests.rs"]
mod tests;
