//! Wire protocol: RESP2 request parsing and reply encoding.

pub mod parser;
pub mod reply;

pub use parser::{Limits, Parsed, Parser, ProtocolError};

/// Arguments of one command, borrowed from the connection buffer (multibulk) or the parser's
/// scratch space (inline).
pub struct Args<'a> {
    base: &'a [u8],
    ranges: &'a [(usize, usize)],
}

impl<'a> Args<'a> {
    pub fn new(base: &'a [u8], ranges: &'a [(usize, usize)]) -> Self {
        Self { base, ranges }
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn get(&self, i: usize) -> &'a [u8] {
        let (s, e) = self.ranges[i];
        &self.base[s..e]
    }

    pub fn iter(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        (0..self.len()).map(move |i| self.get(i))
    }
}
