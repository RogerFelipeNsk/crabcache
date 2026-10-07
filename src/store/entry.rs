//! A stored key/value pair in 16 bytes, pointing to one heap allocation that holds everything else.
//!
//! Allocation layout (byte-aligned):
//!
//! ```text
//! [value_len: u32 LE][expire_at: u64 LE, only if the TTL flag is set][key bytes][value bytes]
//! ```
//!
//! Keys without an expiry pay nothing for it. Compared with a `Box<[u8]>` plus inline length and expiry
//! fields (32 bytes), this halves the fixed per-key cost, which dominates for small values.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::ptr::NonNull;

/// Estimated bytes per entry beyond its allocation: the 16-byte `Entry` with vector slack, its index
/// slot, and allocator rounding. Used for `maxmemory` accounting and `used_memory`.
pub const ENTRY_OVERHEAD: usize = 36;

const TTL_FLAG: u32 = 1 << 31;
const LEN_BYTES: usize = 4;
const EXPIRE_BYTES: usize = 8;

pub struct Entry {
    ptr: NonNull<u8>,
    /// Key length; the top bit flags an expiry field in the allocation header.
    klen: u32,
    /// Eviction metadata: LRU clock (seconds) or LFU `(minutes << 8) | log-counter`, per the active policy.
    pub meta: u32,
}

// SAFETY: an Entry exclusively owns its allocation, exactly like a Box<[u8]>; it has no interior
// mutability or shared state.
unsafe impl Send for Entry {}
unsafe impl Sync for Entry {}

impl Entry {
    /// `expire_at` is an absolute unix-ms deadline; 0 means no expiry.
    pub fn new(key: &[u8], value: &[u8], expire_at: u64, meta: u32) -> Self {
        assert!(key.len() < TTL_FLAG as usize, "key too large");
        assert!(value.len() <= u32::MAX as usize, "value too large");
        let has_ttl = expire_at != 0;
        let header = LEN_BYTES + if has_ttl { EXPIRE_BYTES } else { 0 };
        let layout = Self::layout(header + key.len() + value.len());
        // SAFETY: the layout size is at least LEN_BYTES, so it is non-zero.
        let raw = unsafe { alloc(layout) };
        let Some(ptr) = NonNull::new(raw) else {
            handle_alloc_error(layout)
        };
        // SAFETY: the allocation holds `header + key.len() + value.len()` bytes and every write below
        // stays inside it; sources are distinct from the fresh allocation.
        unsafe {
            let base = ptr.as_ptr();
            base.copy_from_nonoverlapping((value.len() as u32).to_le_bytes().as_ptr(), LEN_BYTES);
            if has_ttl {
                base.add(LEN_BYTES)
                    .copy_from_nonoverlapping(expire_at.to_le_bytes().as_ptr(), EXPIRE_BYTES);
            }
            base.add(header)
                .copy_from_nonoverlapping(key.as_ptr(), key.len());
            base.add(header + key.len())
                .copy_from_nonoverlapping(value.as_ptr(), value.len());
        }
        Self {
            ptr,
            klen: key.len() as u32 | if has_ttl { TTL_FLAG } else { 0 },
            meta,
        }
    }

    fn layout(size: usize) -> Layout {
        Layout::from_size_align(size, 1).expect("entry size overflows")
    }

    fn has_ttl(&self) -> bool {
        self.klen & TTL_FLAG != 0
    }

    fn key_len(&self) -> usize {
        (self.klen & !TTL_FLAG) as usize
    }

    fn header_len(&self) -> usize {
        LEN_BYTES + if self.has_ttl() { EXPIRE_BYTES } else { 0 }
    }

    fn value_len(&self) -> usize {
        // SAFETY: every allocation starts with the 4-byte value length; [u8; 4] has alignment 1.
        u32::from_le_bytes(unsafe { *(self.ptr.as_ptr() as *const [u8; LEN_BYTES]) }) as usize
    }

    fn alloc_size(&self) -> usize {
        self.header_len() + self.key_len() + self.value_len()
    }

    pub fn key(&self) -> &[u8] {
        // SAFETY: the key occupies `key_len` bytes right after the header, inside the allocation.
        unsafe {
            std::slice::from_raw_parts(self.ptr.as_ptr().add(self.header_len()), self.key_len())
        }
    }

    pub fn value(&self) -> &[u8] {
        let start = self.header_len() + self.key_len();
        // SAFETY: the value occupies the last `value_len` bytes of the allocation.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().add(start), self.value_len()) }
    }

    /// Absolute expiry in unix ms; 0 means none.
    pub fn expire_at(&self) -> u64 {
        if !self.has_ttl() {
            return 0;
        }
        // SAFETY: with the TTL flag set, 8 bytes of expiry follow the length field.
        u64::from_le_bytes(unsafe {
            *(self.ptr.as_ptr().add(LEN_BYTES) as *const [u8; EXPIRE_BYTES])
        })
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
            _ => *self = Entry::new(self.key(), self.value(), at, self.meta),
        }
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        let at = self.expire_at();
        at != 0 && at <= now_ms
    }

    pub fn mem_usage(&self) -> usize {
        self.alloc_size() + ENTRY_OVERHEAD
    }

    /// Replaces the value, keeping key, metadata and expiry.
    pub fn set_value(&mut self, value: &[u8]) {
        if value.len() == self.value_len() {
            let start = self.header_len() + self.key_len();
            // SAFETY: same length, so the write covers exactly the existing value bytes; `value` cannot
            // alias this allocation because we hold &mut self.
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

    /// Returns a copy of this entry under a different key (RENAME).
    pub fn with_key(&self, key: &[u8]) -> Self {
        Self::new(key, self.value(), self.expire_at(), self.meta)
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        // SAFETY: `ptr` was allocated in `new` with exactly this layout and is freed only here.
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
            (e.key(), e.value(), e.expire_at(), e.meta),
            (&b"key"[..], &b"value"[..], 0, 7)
        );
        e.set_value(b"other");
        assert_eq!(e.value(), b"other");
        e.set_value(b"much longer value");
        assert_eq!(
            (e.key(), e.value()),
            (&b"key"[..], &b"much longer value"[..])
        );
        e.set_expire_at(1234);
        assert_eq!(
            (e.key(), e.value(), e.expire_at()),
            (&b"key"[..], &b"much longer value"[..], 1234)
        );
        assert!(e.is_expired(1234) && !e.is_expired(1233));
        e.set_expire_at(99);
        assert_eq!(e.expire_at(), 99);
        e.set_value(b"");
        assert_eq!(
            (e.key(), e.value(), e.expire_at()),
            (&b"key"[..], &b""[..], 99)
        );
        e.set_expire_at(0);
        assert_eq!(
            (e.key(), e.value(), e.expire_at()),
            (&b"key"[..], &b""[..], 0)
        );
        assert_eq!(e.meta, 7);
    }

    #[test]
    fn empty_key_binary_value_and_rename() {
        let e = Entry::new(b"", b"\x00\xff\r\n", 5, 0);
        assert_eq!(
            (e.key(), e.value(), e.expire_at()),
            (&b""[..], &b"\x00\xff\r\n"[..], 5)
        );
        let r = e.with_key(b"new name");
        assert_eq!(
            (r.key(), r.value(), r.expire_at()),
            (&b"new name"[..], &b"\x00\xff\r\n"[..], 5)
        );
    }

    #[test]
    fn memory_usage_counts_header_only_when_needed() {
        let plain = Entry::new(b"k", &[0; 100], 0, 0);
        let ttl = Entry::new(b"k", &[0; 100], 1, 0);
        assert_eq!(plain.mem_usage(), 4 + 1 + 100 + ENTRY_OVERHEAD);
        assert_eq!(ttl.mem_usage(), plain.mem_usage() + 8);
    }
}
