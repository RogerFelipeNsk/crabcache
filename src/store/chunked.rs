//! A vector stored as fixed-capacity chunks.
//!
//! A plain `Vec` reallocates and copies as it grows, keeping up to its growth factor in unused
//! capacity plus the freed old buffers until the allocator reuses or purges them. Measured with 1M
//! small keys that cost about 13 bytes per key. Fixed chunks never move: growth adds a chunk, and the
//! only slack is the unfilled tail of the last chunk. Indexing stays O(1), which random sampling for
//! eviction relies on.

use std::ops::{Index, IndexMut};

/// Elements per chunk. With 16-byte entries a chunk is one 4 KiB allocation.
pub const CHUNK: usize = 256;

pub struct Chunked<T> {
    chunks: Vec<Vec<T>>,
    len: usize,
}

impl<T> Default for Chunked<T> {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl<T> Chunked<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push(&mut self, value: T) {
        match self.chunks.last_mut() {
            Some(last) if last.len() < CHUNK => last.push(value),
            _ => {
                let mut chunk = Vec::with_capacity(CHUNK);
                chunk.push(value);
                self.chunks.push(chunk);
            }
        }
        self.len += 1;
    }

    /// Removes element `i`, moving the last element into its place. Panics if out of bounds.
    pub fn swap_remove(&mut self, i: usize) -> T {
        assert!(i < self.len, "index {i} out of bounds (len {})", self.len);
        let last_chunk = self.chunks.last_mut().expect("non-empty");
        let last = last_chunk.pop().expect("chunks are never left empty");
        if last_chunk.is_empty() {
            self.chunks.pop();
        }
        self.len -= 1;
        if i == self.len {
            last
        } else {
            std::mem::replace(&mut self[i], last)
        }
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        (i < self.len).then(|| &self.chunks[i / CHUNK][i % CHUNK])
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.chunks.iter().flatten()
    }

    pub fn clear(&mut self) {
        self.chunks = Vec::new();
        self.len = 0;
    }
}

impl<T> Index<usize> for Chunked<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        &self.chunks[i / CHUNK][i % CHUNK]
    }
}

impl<T> IndexMut<usize> for Chunked<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.chunks[i / CHUNK][i % CHUNK]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_vec_semantics() {
        let mut c = Chunked::default();
        let mut v = Vec::new();
        let mut rng = 0x1234_5678_u64;
        for step in 0..20_000u64 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            if v.is_empty() || rng % 3 != 0 {
                c.push(step);
                v.push(step);
            } else {
                let i = (rng % v.len() as u64) as usize;
                assert_eq!(c.swap_remove(i), v.swap_remove(i));
            }
            assert_eq!(c.len(), v.len());
        }
        assert!(c.iter().eq(v.iter()));
        for (i, x) in v.iter().enumerate() {
            assert_eq!(c[i], *x);
            assert_eq!(c.get(i), Some(x));
        }
        assert_eq!(c.get(v.len()), None);
        // No chunk is ever allocated beyond what the length needs.
        assert_eq!(c.chunks.len(), v.len().div_ceil(CHUNK));
        assert!(c.chunks.iter().all(|ch| ch.capacity() == CHUNK));
    }

    #[test]
    fn drains_to_empty() {
        let mut c = Chunked::default();
        for i in 0..1000 {
            c.push(i);
        }
        while !c.is_empty() {
            c.swap_remove(0);
        }
        assert_eq!(c.chunks.len(), 0);
    }
}
