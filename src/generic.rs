//! Circular Buffer with generic [Notifier] to implement custom wait/block behavior.

use slab::Slab;
use spin::Mutex;
use std::sync::Arc;
use thiserror::Error;

use crate::double_mapped_buffer::{DoubleMappedBuffer, DoubleMappedBufferError};

/// Error setting up the underlying buffer.
#[derive(Error, Debug)]
pub enum CircularError {
    /// Failed to allocate double mapped buffer.
    #[error("Failed to allocate double mapped buffer.")]
    Allocation(DoubleMappedBufferError),
}

pub use crate::{Metadata, NoMetadata, Notifier};

/// Gerneric Circular Buffer Constructor
pub struct Circular;

impl Circular {
    /// Create a buffer that can hold at least `min_items` items of type `T`.
    ///
    /// The size is the least common multiple of the page size and the size of `T`.
    pub fn with_capacity<T, N, M>(min_items: usize) -> Result<Writer<T, N, M>, CircularError>
    where
        N: Notifier,
        M: Metadata,
    {
        let buffer = match DoubleMappedBuffer::new(min_items) {
            Ok(buffer) => Arc::new(buffer),
            Err(e) => return Err(CircularError::Allocation(e)),
        };

        let state = Arc::new(Mutex::new(State {
            writer_offset: 0,
            writer_ab: false,
            writer_done: false,
            readers: Slab::new(),
        }));

        let writer = Writer {
            buffer,
            state,
            last_space: 0,
        };

        Ok(writer)
    }
}

struct State<N, M>
where
    N: Notifier,
    M: Metadata,
{
    writer_offset: usize,
    writer_ab: bool,
    writer_done: bool,
    readers: Slab<ReaderState<N, M>>,
}
struct ReaderState<N, M> {
    ab: bool,
    offset: usize,
    reader_notifier: N,
    writer_notifier: N,
    meta: M,
}

/// Writer for a generic circular buffer with items of type `T` and [Notifier] of type `N`.
pub struct Writer<T, N, M>
where
    N: Notifier,
    M: Metadata,
{
    last_space: usize,
    buffer: Arc<DoubleMappedBuffer<T>>,
    state: Arc<Mutex<State<N, M>>>,
}

impl<T, N, M> Writer<T, N, M>
where
    N: Notifier,
    M: Metadata,
{
    #[inline(always)]
    fn writer_space(capacity: usize, w_off: usize, w_ab: bool, r_off: usize, r_ab: bool) -> usize {
        if w_off > r_off {
            r_off + capacity - w_off
        } else if w_off < r_off {
            r_off - w_off
        } else if r_ab == w_ab {
            capacity
        } else {
            0
        }
    }

    /// Add a [Reader] to the buffer.
    pub fn add_reader(&self, reader_notifier: N, writer_notifier: N) -> Reader<T, N, M> {
        let mut state = self.state.lock();
        let reader_state = ReaderState {
            ab: state.writer_ab,
            offset: state.writer_offset,
            reader_notifier,
            writer_notifier,
            meta: M::new(),
        };
        let id = state.readers.insert(reader_state);

        Reader {
            id,
            last_space: 0,
            buffer: self.buffer.clone(),
            state: self.state.clone(),
        }
    }

    #[inline(always)]
    fn space_and_offset_locked(
        state: &mut State<N, M>,
        capacity: usize,
        arm: bool,
    ) -> (usize, usize) {
        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

        let mut space = capacity;

        for (_, reader) in state.readers.iter_mut() {
            let s = Self::writer_space(capacity, w_off, w_ab, reader.offset, reader.ab);

            space = std::cmp::min(space, s);

            if s == 0 && arm {
                reader.writer_notifier.arm();
                break;
            }
            if s == 0 {
                break;
            }
        }

        (space, w_off)
    }

    /// Get a slice for the output buffer space. Might be empty.
    pub fn slice(&mut self, arm: bool) -> &mut [T] {
        let mut state = self.state.lock();
        let (space, offset) =
            Self::space_and_offset_locked(&mut state, self.buffer.capacity(), arm);
        self.last_space = space;
        unsafe { &mut self.buffer.slice_with_offset_mut(offset)[0..space] }
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

        let mut state = self.state.lock();
        let capacity = self.buffer.capacity();

        debug_assert!(Self::space_and_offset_locked(&mut state, capacity, false).0 >= n);

        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

        for (_, r) in state.readers.iter_mut() {
            let space = Reader::<T, N, M>::reader_space(capacity, w_off, w_ab, r.offset, r.ab);

            if !meta.is_empty() {
                r.meta.add_from_slice(space, meta);
            }
            r.reader_notifier.notify();
        }

        if state.writer_offset + n >= self.buffer.capacity() {
            state.writer_ab = !state.writer_ab;
        }
        state.writer_offset = (state.writer_offset + n) % self.buffer.capacity();
    }
}

impl<T, N, M> Drop for Writer<T, N, M>
where
    N: Notifier,
    M: Metadata,
{
    fn drop(&mut self) {
        let mut state = self.state.lock();
        state.writer_done = true;
        for (_, r) in state.readers.iter_mut() {
            r.reader_notifier.notify();
        }
    }
}

/// Reader for a generic circular buffer with items of type `T` and [Notifier] of type `N`.
pub struct Reader<T, N, M>
where
    N: Notifier,
    M: Metadata,
{
    id: usize,
    last_space: usize,
    buffer: Arc<DoubleMappedBuffer<T>>,
    state: Arc<Mutex<State<N, M>>>,
}

impl<T, N, M> Reader<T, N, M>
where
    N: Notifier,
    M: Metadata,
{
    #[inline(always)]
    fn reader_space(capacity: usize, w_off: usize, w_ab: bool, r_off: usize, r_ab: bool) -> usize {
        if r_off > w_off {
            w_off + capacity - r_off
        } else if r_off < w_off {
            w_off - r_off
        } else if r_ab == w_ab {
            0
        } else {
            capacity
        }
    }

    #[inline(always)]
    fn space_and_offset_locked(
        state: &mut State<N, M>,
        id: usize,
        capacity: usize,
        arm: bool,
    ) -> (usize, usize, bool) {
        let done = state.writer_done;
        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

        let my = unsafe { state.readers.get_unchecked_mut(id) };
        let space = Self::reader_space(capacity, w_off, w_ab, my.offset, my.ab);

        if space == 0 && arm {
            my.reader_notifier.arm();
        }

        (space, my.offset, done)
    }

    /// Get a slice without fetching metadata.
    pub fn slice(&mut self, arm: bool) -> Option<&[T]> {
        let mut state = self.state.lock();
        let (space, offset, done) =
            Self::space_and_offset_locked(&mut state, self.id, self.buffer.capacity(), arm);
        self.last_space = space;

        if space == 0 && done {
            return None;
        }

        unsafe { Some(&self.buffer.slice_with_offset(offset)[0..space]) }
    }

    /// Get a slice and copy metadata into `out` in one call.
    pub fn slice_with_metadata_into(&mut self, arm: bool, out: &mut Vec<M::Item>) -> Option<&[T]> {
        let mut state = self.state.lock();
        let (space, offset, done) =
            Self::space_and_offset_locked(&mut state, self.id, self.buffer.capacity(), arm);
        let my = unsafe { state.readers.get_unchecked_mut(self.id) };

        my.meta.get_into(out);
        self.last_space = space;

        if space == 0 && done {
            out.clear();
            return None;
        }

        unsafe { Some(&self.buffer.slice_with_offset(offset)[0..space]) }
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

        assert!(n <= self.last_space, "vmcircbuffer: consumed too much!");
        self.last_space -= n;

        let mut state = self.state.lock();
        debug_assert!(
            Self::space_and_offset_locked(&mut state, self.id, self.buffer.capacity(), false).0
                >= n
        );
        let my = unsafe { state.readers.get_unchecked_mut(self.id) };

        my.meta.consume(n);

        if my.offset + n >= self.buffer.capacity() {
            my.ab = !my.ab;
        }
        my.offset = (my.offset + n) % self.buffer.capacity();

        my.writer_notifier.notify();
    }
}

impl<T, N, M> Drop for Reader<T, N, M>
where
    N: Notifier,
    M: Metadata,
{
    fn drop(&mut self) {
        let mut state = self.state.lock();
        let mut s = state.readers.remove(self.id);
        s.writer_notifier.notify();
    }
}
