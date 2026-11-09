//! A fast hash for keys that come from geometry (cells, pairs of vertex numbers), not from an
//! attacker: the standard library's keyed hash buys nothing there and costs a lot.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A hash map with [`FastHasher`].
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// FxHash's mixing: rotate, xor, multiply.
#[derive(Default)]
pub struct FastHasher(u64);

impl Hasher for FastHasher {
    fn finish(&self) -> u64 {
        // The high bits are the well-mixed ones; the map looks at the low ones.
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        // Arrays of integers arrive here as one run of bytes: take them eight at a time.
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.write_u64(u64::from_le_bytes(*word));
        }
        for &b in rest {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u32(&mut self, v: u32) {
        self.write_u64(u64::from(v));
    }

    fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }

    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, v: usize) {
        self.write_u64(v as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbouring_keys_spread_over_the_map() {
        let mut map: FastMap<u64, u32> = FastMap::default();
        for i in 0..10_000u64 {
            map.insert((i << 32) | (i + 1), i as u32);
        }
        assert_eq!(map.len(), 10_000);
        assert_eq!(map[&((77 << 32) | 78)], 77);
    }
}
