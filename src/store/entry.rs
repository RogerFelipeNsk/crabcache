//! A stored key/value pair in 16 bytes, pointing to one heap allocation that holds everything else.
//!
//! Allocation layout (byte-aligned):
//!
//! ```text
//! [stored_len: u32 LE]
//! [expire_at: u64 LE]                      only if the TTL flag is set
//! [dict_id: u32 LE][original_len: u32 LE]  only if the PACKED flag is set (value is compressed)
//! [key bytes][stored value bytes]
//! ```
//!
//! Optional fields cost nothing when absent. Compared with a `Box<[u8]>` plus inline length and expiry
//! fields (32 bytes), this halves the fixed per-key cost, which dominates for small values.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::ptr::NonNull;

/// Estimated bytes per entry beyond its allocation: the 16-byte `Entry` with chunk slack, its index
/// slot, and allocator rounding. Used for `maxmemory` accounting and `used_memory`.
pub const ENTRY_OVERHEAD: usize = 36;

const TTL_FLAG: u32 = 1 << 31;
const PACKED_FLAG: u32 = 1 << 30;
const KEY_LEN_MASK: u32 = PACKED_FLAG - 1;
const LEN_BYTES: usize = 4;
const EXPIRE_BYTES: usize = 8;
const PACKED_BYTES: usize = 8;

/// Marks a value stored compressed with dictionary `dict`; `original_len` is its uncompressed size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packed {
    pub dict: u32,
    pub original_len: u32,
}

pub struct Entry {
    ptr: NonNull<u8>,
    /// Key length in the low 30 bits; the top bits flag the optional header fields.
    klen: u32,
    /// Eviction metadata: LRU clock (seconds) or LFU `(minutes << 8) | log-counter`, per the active policy.
    pub meta: u32,
}

// SAFETY: an Entry exclusively owns its allocation, exactly like a Box<[u8]>; it has no interior
// mutability or shared state.
unsafe impl Send for Entry {}
unsafe impl Sync for Entry {}

impl Entry {
    /// A plain (uncompressed) entry. `expire_at` is an absolute unix-ms deadline; 0 means no expiry.
    pub fn new(key: &[u8], value: &[u8], expire_at: u64, meta: u32) -> Self {
        Self::build(key, value, expire_at, meta, None)
    }

    /// An entry whose value is stored compressed.
    pub fn new_packed(
        key: &[u8],
        compressed: &[u8],
        expire_at: u64,
        meta: u32,
        packed: Packed,
    ) -> Self {
        Self::build(key, compressed, expire_at, meta, Some(packed))
    }

    fn build(key: &[u8], stored: &[u8], expire_at: u64, meta: u32, packed: Option<Packed>) -> Self {
        assert!(key.len() <= KEY_LEN_MASK as usize, "key too large");
        assert!(stored.len() <= u32::MAX as usize, "value too large");
        let mut flags = 0;
        let mut header = LEN_BYTES;
        if expire_at != 0 {
            flags |= TTL_FLAG;
            header += EXPIRE_BYTES;
        }
        if packed.is_some() {
            flags |= PACKED_FLAG;
            header += PACKED_BYTES;
        }
        let layout = Self::layout(header + key.len() + stored.len());
        // SAFETY: the layout size is at least LEN_BYTES, so it is non-zero.
        let raw = unsafe { alloc(layout) };
        let Some(ptr) = NonNull::new(raw) else {
            handle_alloc_error(layout)
        };
        let mut fields: [u8; LEN_BYTES + EXPIRE_BYTES + PACKED_BYTES] = [0; 20];
        let mut n = 0;
        fields[..LEN_BYTES].copy_from_slice(&(stored.len() as u32).to_le_bytes());
        n += LEN_BYTES;
        if expire_at != 0 {
            fields[n..n + EXPIRE_BYTES].copy_from_slice(&expire_at.to_le_bytes());
            n += EXPIRE_BYTES;
        }
        if let Some(p) = packed {
            fields[n..n + 4].copy_from_slice(&p.dict.to_le_bytes());
            fields[n + 4..n + 8].copy_from_slice(&p.original_len.to_le_bytes());
            n += PACKED_BYTES;
        }
        debug_assert_eq!(n, header);
        // SAFETY: the allocation holds `header + key.len() + stored.len()` bytes and every write
        // below stays inside it; sources are distinct from the fresh allocation.
        unsafe {
            let base = ptr.as_ptr();
            base.copy_from_nonoverlapping(fields.as_ptr(), header);
            base.add(header)
                .copy_from_nonoverlapping(key.as_ptr(), key.len());
            base.add(header + key.len())
                .copy_from_nonoverlapping(stored.as_ptr(), stored.len());
        }
        Self {
            ptr,
            klen: key.len() as u32 | flags,
            meta,
        }
    }

    fn layout(size: usize) -> Layout {
        Layout::from_size_align(size, 1).expect("entry size overflows")
    }

    fn has_ttl(&self) -> bool {
        self.klen & TTL_FLAG != 0
    }

    fn is_packed(&self) -> bool {
        self.klen & PACKED_FLAG != 0
    }

    fn key_len(&self) -> usize {
        (self.klen & KEY_LEN_MASK) as usize
    }

    fn packed_offset(&self) -> usize {
        LEN_BYTES + if self.has_ttl() { EXPIRE_BYTES } else { 0 }
    }

    fn header_len(&self) -> usize {
        self.packed_offset() + if self.is_packed() { PACKED_BYTES } else { 0 }
    }

    /// Reads `N` header bytes at `offset`.
    fn header_bytes<const N: usize>(&self, offset: usize) -> [u8; N] {
        debug_assert!(offset + N <= self.header_len());
        // SAFETY: callers only read fields present in the header; [u8; N] has alignment 1.
        unsafe { *(self.ptr.as_ptr().add(offset) as *const [u8; N]) }
    }

    fn stored_len(&self) -> usize {
        u32::from_le_bytes(self.header_bytes(0)) as usize
    }

    fn alloc_size(&self) -> usize {
        self.header_len() + self.key_len() + self.stored_len()
    }

    pub fn key(&self) -> &[u8] {
        // SAFETY: the key occupies `key_len` bytes right after the header, inside the allocation.
        unsafe {
            std::slice::from_raw_parts(self.ptr.as_ptr().add(self.header_len()), self.key_len())
        }
    }

    /// The value bytes exactly as stored: compressed when `packed()` is `Some`.
    pub fn stored(&self) -> &[u8] {
        let start = self.header_len() + self.key_len();
        // SAFETY: the stored value occupies the last `stored_len` bytes of the allocation.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().add(start), self.stored_len()) }
    }

    /// The value, if it is stored uncompressed.
    pub fn plain(&self) -> Option<&[u8]> {
        (!self.is_packed()).then(|| self.stored())
    }

    pub fn packed(&self) -> Option<Packed> {
        if !self.is_packed() {
            return None;
        }
        let at = self.packed_offset();
        Some(Packed {
            dict: u32::from_le_bytes(self.header_bytes(at)),
            original_len: u32::from_le_bytes(self.header_bytes(at + 4)),
        })
    }

    /// Length of the value as clients see it (uncompressed).
    pub fn value_len(&self) -> usize {
        match self.packed() {
            Some(p) => p.original_len as usize,
            None => self.stored_len(),
        }
    }

    /// Absolute expiry in unix ms; 0 means none.
    pub fn expire_at(&self) -> u64 {
        if !self.has_ttl() {
            return 0;
        }
        u64::from_le_bytes(self.header_bytes(LEN_BYTES))
    }

    /// Sets (`at > 0`) or clears (`at == 0`) the expiry. Adding or removing it reallocates.
    pub fn set_expire_at(&mut self, at: u64) {
        match (self.has_ttl(), at != 0) {
            (true, true) => {
                // SAFETY: the expiry field exists (TTL flag set) and is exclusively ours (&mut self).
                unsafe {
                    self.ptr
                        .as_ptr()
                        .add(LEN_BYTES)
                        .copy_from_nonoverlapping(at.to_le_bytes().as_ptr(), EXPIRE_BYTES);
                }
            }
            (false, false) => {}
            _ => *self = Entry::build(self.key(), self.stored(), at, self.meta, self.packed()),
        }
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        let at = self.expire_at();
        at != 0 && at <= now_ms
    }

    pub fn mem_usage(&self) -> usize {
        self.alloc_size() + ENTRY_OVERHEAD
    }

    /// Replaces the value with a plain one, keeping key, metadata and expiry.
    pub fn set_value(&mut self, value: &[u8]) {
        if !self.is_packed() && value.len() == self.stored_len() {
            let start = self.header_len() + self.key_len();
            // SAFETY: same length, so the write covers exactly the existing value bytes; `value`
            // cannot alias this allocation because we hold &mut self.
            unsafe {
                self.ptr
                    .as_ptr()
                    .add(start)
                    .copy_from_nonoverlapping(value.as_ptr(), value.len());
            }
        } else {
            *self = Entry::new(self.key(), value, self.expire_at(), self.meta);
        }
    }

    /// Returns a copy of this entry under a different key (RENAME). Compression is kept as is.
    pub fn with_key(&self, key: &[u8]) -> Self {
        Self::build(
            key,
            self.stored(),
            self.expire_at(),
            self.meta,
            self.packed(),
        )
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        // SAFETY: `ptr` was allocated in `build` with exactly this layout and is freed only here.
        unsafe { dealloc(self.ptr.as_ptr(), Self::layout(self.alloc_size())) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_sixteen_bytes() {
        assert_eq!(std::mem::size_of::<Entry>(), 16);
    }

    #[test]
    fn accessors_and_mutation() {
        let mut e = Entry::new(b"key", b"value", 0, 7);
        assert_eq!(
            (e.key(), e.plain(), e.expire_at(), e.meta),
            (&b"key"[..], Some(&b"value"[..]), 0, 7)
        );
        e.set_value(b"other");
        assert_eq!(e.plain(), Some(&b"other"[..]));
        e.set_value(b"much longer value");
        assert_eq!(
            (e.key(), e.plain()),
            (&b"key"[..], Some(&b"much longer value"[..]))
        );
        e.set_expire_at(1234);
        assert_eq!(
            (e.key(), e.plain(), e.expire_at()),
            (&b"key"[..], Some(&b"much longer value"[..]), 1234)
        );
        assert!(e.is_expired(1234) && !e.is_expired(1233));
        e.set_expire_at(99);
        assert_eq!(e.expire_at(), 99);
        e.set_value(b"");
        assert_eq!(
            (e.key(), e.plain(), e.expire_at()),
            (&b"key"[..], Some(&b""[..]), 99)
        );
        e.set_expire_at(0);
        assert_eq!(
            (e.key(), e.plain(), e.expire_at()),
            (&b"key"[..], Some(&b""[..]), 0)
        );
        assert_eq!(e.meta, 7);
    }

    #[test]
    fn packed_entries_keep_their_header_through_every_mutation() {
        let p = Packed {
            dict: 3,
            original_len: 500,
        };
        for ttl in [0, 42] {
            let mut e = Entry::new_packed(b"user:1", b"\x28\xb5\x2f\xfd", ttl, 9, p);
            assert_eq!(e.packed(), Some(p));
            assert_eq!(e.plain(), None);
            assert_eq!(
                (e.key(), e.stored(), e.value_len()),
                (&b"user:1"[..], &b"\x28\xb5\x2f\xfd"[..], 500)
            );
            assert_eq!(e.expire_at(), ttl);
            // Adding, changing and removing the TTL reallocates or rewrites in place.
            e.set_expire_at(77);
            e.set_expire_at(88);
            assert_eq!(
                (e.expire_at(), e.packed(), e.key()),
                (88, Some(p), &b"user:1"[..])
            );
            e.set_expire_at(0);
            assert_eq!(
                (e.expire_at(), e.packed(), e.stored()),
                (0, Some(p), &b"\x28\xb5\x2f\xfd"[..])
            );
            // Renaming keeps the compressed bytes.
            let r = e.with_key(b"user:2");
            assert_eq!(
                (r.key(), r.packed(), r.stored()),
                (&b"user:2"[..], Some(p), &b"\x28\xb5\x2f\xfd"[..])
            );
            // Writing a new value stores it plain.
            e.set_value(b"\x28\xb5\x2f\xfd");
            assert_eq!(
                (e.packed(), e.plain()),
                (None, Some(&b"\x28\xb5\x2f\xfd"[..]))
            );
        }
    }

    #[test]
    fn memory_usage_counts_optional_fields_only_when_present() {
        let plain = Entry::new(b"k", &[0; 100], 0, 0);
        let ttl = Entry::new(b"k", &[0; 100], 1, 0);
        let packed = Entry::new_packed(
            b"k",
            &[0; 40],
            0,
            0,
            Packed {
                dict: 1,
                original_len: 100,
            },
        );
        assert_eq!(plain.mem_usage(), 4 + 1 + 100 + ENTRY_OVERHEAD);
        assert_eq!(ttl.mem_usage(), plain.mem_usage() + 8);
        assert_eq!(packed.mem_usage(), 4 + 8 + 1 + 40 + ENTRY_OVERHEAD);
    }
}
