//! Lock-free circular buffer (SPMC) backed by a double-mapped region.
//!
//! - Single producer, multiple readers.
//! - Uses atomics for buffer state (no global mutex).
//! - Uses per-reader `spin::Mutex` only for metadata.
//! - Blocking `slice()` spins until space/data is available.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use thiserror::Error;

use crate::double_mapped_buffer::{DoubleMappedBuffer, DoubleMappedBufferError};
use crate::Metadata;

/// Error setting up the underlying buffer or adding readers.
#[derive(Error, Debug)]
pub enum CircularError {
    /// Failed to allocate double mapped buffer.
    #[error("Failed to allocate double mapped buffer.")]
    Allocation(DoubleMappedBufferError),
    /// Failed to add a reader because the reader limit was reached.
    #[error("Failed to add reader: maximum number of readers reached.")]
    TooManyReaders,
}

/// Lock-free Circular Buffer Constructor.
pub struct Circular;

impl Circular {
    /// Create a buffer that can hold at least `min_items` items of type `T`.
    ///
    /// The size is the least common multiple of the page size and the size of `T`.
    pub fn with_capacity<T, M>(
        min_items: usize,
        max_readers: usize,
    ) -> Result<Writer<T, M>, CircularError>
    where
        M: Metadata,
    {
        let buffer = match DoubleMappedBuffer::new(min_items) {
            Ok(buffer) => buffer,
            Err(e) => return Err(CircularError::Allocation(e)),
        };

        let mut readers = Vec::with_capacity(max_readers);
        for _ in 0..max_readers {
            readers.push(ReaderSlot::new());
        }

        let inner = Arc::new(Inner {
            buffer,
            meta_epoch: AtomicUsize::new(0),
            writer_pos: AtomicUsize::new(0),
            writer_done: AtomicBool::new(false),
            active_readers: AtomicUsize::new(0),
            readers,
        });

        Ok(Writer {
            inner,
            last_space: 0,
        })
    }

    /// Create a buffer for items of type `T` with minimal capacity (usually a page size).
    ///
    /// The actual size is the least common multiple of the page size and the size of `T`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T, M>(max_readers: usize) -> Result<Writer<T, M>, CircularError>
    where
        M: Metadata,
    {
        Self::with_capacity(0, max_readers)
    }
}

struct Inner<T, M>
where
    M: Metadata,
{
    buffer: DoubleMappedBuffer<T>,
    meta_epoch: AtomicUsize,
    writer_pos: AtomicUsize,
    writer_done: AtomicBool,
    active_readers: AtomicUsize,
    readers: Vec<ReaderSlot<M>>,
}

const READER_INACTIVE: usize = 0;
const READER_ACTIVE: usize = 1;

struct ReaderSlot<M>
where
    M: Metadata,
{
    state: AtomicUsize,
    pos: AtomicUsize,
    meta_dirty: AtomicBool,
    meta: spin::Mutex<M>,
}

impl<M> ReaderSlot<M>
where
    M: Metadata,
{
    fn new() -> Self {
        Self {
            state: AtomicUsize::new(READER_INACTIVE),
            pos: AtomicUsize::new(0),
            meta_dirty: AtomicBool::new(false),
            meta: spin::Mutex::new(M::new()),
        }
    }
}

/// Writer for a lock-free circular buffer with items of type `T`.
pub struct Writer<T, M>
where
    M: Metadata,
{
    last_space: usize,
    inner: Arc<Inner<T, M>>,
}

impl<T, M> Writer<T, M>
where
    M: Metadata,
{
    /// Add a reader to the buffer.
    pub fn add_reader(&self) -> Result<Reader<T, M>, CircularError> {
        let id = self
            .inner
            .active_readers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                if n < self.inner.readers.len() {
                    Some(n + 1)
                } else {
                    None
                }
            })
            .map_err(|_| CircularError::TooManyReaders)?;

        let slot = &self.inner.readers[id];
        let w = self.inner.writer_pos.load(Ordering::Acquire);
        slot.pos.store(w, Ordering::Relaxed);
        {
            let mut meta = slot.meta.lock();
            *meta = M::new();
        }
        slot.meta_dirty.store(false, Ordering::Release);
        slot.state.store(READER_ACTIVE, Ordering::Release);
        Ok(Reader {
            id,
            last_space: 0,
            inner: self.inner.clone(),
        })
    }

    fn space_and_offset(&self) -> (usize, usize) {
        let cap = self.inner.buffer.capacity();
        let w = self.inner.writer_pos.load(Ordering::Acquire);
        let mut max_dist = 0usize;
        let mut any = false;

        let active = self.inner.active_readers.load(Ordering::Acquire);
        for slot in &self.inner.readers[..active] {
            if slot.state.load(Ordering::Acquire) == READER_ACTIVE {
                any = true;
                let r = slot.pos.load(Ordering::Acquire);
                let dist = w.wrapping_sub(r);
                if dist > max_dist {
                    max_dist = dist;
                }
            }
        }

        let space = if !any {
            cap
        } else {
            cap.saturating_sub(max_dist)
        };

        (space, w % cap)
    }

    /// Get a slice to the available output space.
    ///
    /// This function returns immediately. The slice might be empty.
    pub fn slice(&mut self) -> &mut [T] {
        let (space, offset) = self.space_and_offset();
        self.last_space = space;
        unsafe { &mut self.inner.buffer.slice_with_offset_mut(offset)[0..space] }
    }

    /// Indicates that `n` items were written to the output buffer.
    ///
    /// It is ok if `n` is zero.
    ///
    /// # Panics
    ///
    /// If produced more than space was available in the last provided slice.
    pub fn produce(&mut self, n: usize, meta: &[M::Item]) {
        if n == 0 {
            return;
        }

        assert!(n <= self.last_space, "vmcircbuffer: produced too much");
        self.last_space -= n;

        let w = self.inner.writer_pos.load(Ordering::Acquire);

        if !meta.is_empty() {
            self.inner.meta_epoch.fetch_add(1, Ordering::AcqRel);
            let active = self.inner.active_readers.load(Ordering::Acquire);
            for slot in &self.inner.readers[..active] {
                if slot.state.load(Ordering::Acquire) == READER_ACTIVE {
                    let mut m = slot.meta.lock();
                    let r = slot.pos.load(Ordering::Acquire);
                    let dist = w.wrapping_sub(r);
                    m.add_from_slice(dist, meta);
                    slot.meta_dirty.store(true, Ordering::Release);
                }
            }
            self.inner
                .writer_pos
                .store(w.wrapping_add(n), Ordering::Release);
            self.inner.meta_epoch.fetch_add(1, Ordering::Release);
            return;
        }

        self.inner
            .writer_pos
            .store(w.wrapping_add(n), Ordering::Release);
    }
}

impl<T, M> Drop for Writer<T, M>
where
    M: Metadata,
{
    fn drop(&mut self) {
        self.inner.writer_done.store(true, Ordering::Release);
    }
}

/// Reader for a lock-free circular buffer with items of type `T`.
pub struct Reader<T, M>
where
    M: Metadata,
{
    id: usize,
    last_space: usize,
    inner: Arc<Inner<T, M>>,
}

impl<T, M> Reader<T, M>
where
    M: Metadata,
{
    fn space_and_offset(&self) -> (usize, usize) {
        let cap = self.inner.buffer.capacity();
        let slot = &self.inner.readers[self.id];
        let w = self.inner.writer_pos.load(Ordering::Acquire);
        let r = slot.pos.load(Ordering::Acquire);
        let avail = w.wrapping_sub(r);
        let space = if avail >= cap { cap } else { avail };
        (space, r % cap)
    }

    fn space_and_offset_and_meta_into(&self, out: &mut Vec<M::Item>) -> (usize, usize, bool) {
        let cap = self.inner.buffer.capacity();
        let slot = &self.inner.readers[self.id];

        loop {
            let e1 = self.inner.meta_epoch.load(Ordering::Acquire);
            if (e1 & 1) != 0 {
                std::hint::spin_loop();
                continue;
            }

            let m = slot.meta.lock();
            let w = self.inner.writer_pos.load(Ordering::Acquire);
            let done = self.inner.writer_done.load(Ordering::Acquire);
            let r = slot.pos.load(Ordering::Acquire);
            let avail = w.wrapping_sub(r);
            let space = if avail >= cap { cap } else { avail };

            if slot.meta_dirty.load(Ordering::Acquire) {
                m.get_into(out);
                if out.is_empty() {
                    slot.meta_dirty.store(false, Ordering::Release);
                }
            } else {
                out.clear();
            }

            let e2 = self.inner.meta_epoch.load(Ordering::Acquire);
            if e1 == e2 {
                return (space, r % cap, done);
            }
        }
    }

    /// Get a slice without fetching metadata.
    pub fn slice(&mut self) -> &[T] {
        let (space, offset) = self.space_and_offset();
        self.last_space = space;
        unsafe { &self.inner.buffer.slice_with_offset(offset)[0..space] }
    }

    /// Get a slice and copy metadata into `out` in one call.
    pub fn slice_with_meta_into(&mut self, out: &mut Vec<M::Item>) -> Option<&[T]> {
        let (space, offset, done) = self.space_and_offset_and_meta_into(out);
        self.last_space = space;
        if space == 0 && done {
            out.clear();
            return None;
        }
        unsafe { Some(&self.inner.buffer.slice_with_offset(offset)[0..space]) }
    }

    /// Indicates that `n` items were read.
    ///
    /// # Panics
    ///
    /// If consumed more than space was available in the last provided slice.
    pub fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        assert!(n <= self.last_space, "vmcircbuffer: consumed too much");
        self.last_space -= n;

        let slot = &self.inner.readers[self.id];
        {
            let mut m = slot.meta.lock();
            m.consume(n);
            let r = slot.pos.load(Ordering::Acquire);
            slot.pos.store(r.wrapping_add(n), Ordering::Release);
        }
    }
}

impl<T, M> Drop for Reader<T, M>
where
    M: Metadata,
{
    fn drop(&mut self) {
        let slot = &self.inner.readers[self.id];
        {
            let mut m = slot.meta.lock();
            *m = M::new();
        }
        slot.meta_dirty.store(false, Ordering::Release);
        slot.state.store(READER_INACTIVE, Ordering::Release);
    }
}
