//! Circular Buffer with generic [Notifier] to implement custom wait/block behavior.

use slab::Slab;
use std::sync::{Arc, Mutex};
use thiserror::Error;

use crate::double_mapped_buffer::DoubleMappedBuffer;

/// Error setting up the underlying buffer.
#[derive(Error, Debug)]
pub enum CircularError {
    /// Failed to allocate double mapped buffer.
    #[error("Failed to allocate double mapped buffer.")]
    Allocation,
}

/// A custom notifier can be used to trigger arbitrary mechanism to signal to a
/// reader or writer that data or buffer space is available. This could be a
/// write to an sync/async channel or a condition variable.
pub trait Notifier {
    /// Arm the notifier.
    fn arm(&mut self);
    /// The implementation must
    /// - only notify if armed
    /// - notify
    /// - unarm
    fn notify(&mut self);
}

/// Custom metadata to annotate items.
pub trait Metadata {
    type Item: Clone;

    /// Create metadata container.
    fn new() -> Self;
    /// Add metadata, applying `offset` shift to items.
    fn add(&mut self, offset: usize, tags: Vec<Self::Item>);
    /// Get metadata.
    fn get(&self) -> Vec<Self::Item>;
    /// Prune metadata, i.e., delete consumed [items](Self::Item) and update offsets for the remaining.
    fn consume(&mut self, items: usize);
}

/// Void implementation for the [Metadata] trait for buffers that don't use metadata.
pub struct NoMetadata;
impl Metadata for NoMetadata {
    type Item = ();

    fn new() -> Self {
        Self
    }
    fn add(&mut self, _offset: usize, _tags: Vec<Self::Item>) {}
    fn get(&self) -> Vec<Self::Item> {
        Vec::new()
    }
    fn consume(&mut self, _items: usize) {}
}

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
            Err(_) => return Err(CircularError::Allocation),
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
    /// Add a [Reader] to the buffer.
    pub fn add_reader(&self, reader_notifier: N, writer_notifier: N) -> Reader<T, N, M> {
        let mut state = self.state.lock().unwrap();
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

    fn space_and_offset(&self, arm: bool) -> (usize, usize) {
        let mut state = self.state.lock().unwrap();
        let capacity = self.buffer.capacity();
        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

        let mut space = capacity;

        for (_, reader) in state.readers.iter_mut() {
            let r_off = reader.offset;
            let r_ab = reader.ab;

            let s = if w_off > r_off {
                r_off + capacity - w_off
            } else if w_off < r_off {
                r_off - w_off
            } else if r_ab == w_ab {
                capacity
            } else {
                0
            };

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
        let (space, offset) = self.space_and_offset(arm);
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
    pub fn produce(&mut self, n: usize, meta: Vec<M::Item>) {
        if n == 0 {
            return;
        }

        debug_assert!(self.space_and_offset(false).0 >= n);

        assert!(n <= self.last_space, "vmcircbuffer: produced too much");
        self.last_space -= n;

        let mut state = self.state.lock().unwrap();

        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;
        let capacity = self.buffer.capacity();

        for (_, r) in state.readers.iter_mut() {
            let r_off = r.offset;
            let r_ab = r.ab;

            let space = if r_off > w_off {
                w_off + capacity - r_off
            } else if r_off < w_off {
                w_off - r_off
            } else if r_ab == w_ab {
                0
            } else {
                capacity
            };

            r.meta.add(space, meta.clone());
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
        let mut state = self.state.lock().unwrap();
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
    fn space_and_offset_and_meta(&self, arm: bool) -> (usize, usize, bool, Vec<M::Item>) {
        let mut state = self.state.lock().unwrap();

        let capacity = self.buffer.capacity();
        let done = state.writer_done;
        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

        let my = unsafe { state.readers.get_unchecked_mut(self.id) };
        let r_off = my.offset;
        let r_ab = my.ab;

        let space = if r_off > w_off {
            w_off + capacity - r_off
        } else if r_off < w_off {
            w_off - r_off
        } else if r_ab == w_ab {
            0
        } else {
            capacity
        };

        if space == 0 && arm {
            my.reader_notifier.arm();
        }

        (space, r_off, done, my.meta.get())
    }

    /// Get a slice with the items available to read.
    ///
    /// Returns `None` if the reader was dropped and all data was read.
    pub fn slice(&mut self, arm: bool) -> Option<(&[T], Vec<M::Item>)> {
        let (space, offset, done, tags) = self.space_and_offset_and_meta(arm);
        self.last_space = space;
        if space == 0 && done {
            None
        } else {
            unsafe { Some((&self.buffer.slice_with_offset(offset)[0..space], tags)) }
        }
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

        debug_assert!(self.space_and_offset_and_meta(false).0 >= n);

        assert!(n <= self.last_space, "vmcircbuffer: consumed too much!");
        self.last_space -= n;

        let mut state = self.state.lock().unwrap();
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
        let mut state = self.state.lock().unwrap();
        let mut s = state.readers.remove(self.id);
        s.writer_notifier.notify();
    }
}
