//! Circular Buffer with generic [Notifier] to implement custom wait/block behavior.

use slab::Slab;
use std::sync::{Arc, Mutex};
use thiserror::Error;

use crate::double_mapped_buffer::DoubleMappedBuffer;

#[derive(Error, Debug)]
pub enum CircularError {
    #[error("Failed to allocate double mapped buffer.")]
    Allocation,
}

/// A custom notifier can be used to trigger arbitrary mechanism to signal to a reader or writer that data or buffer space is available. This might be a write to an sync/async channel or a condition variable.
pub trait Notifier {
    /// Arm the notifier.
    fn arm(&mut self);
    /// The implementation should
    /// - only notify if armed
    /// - notify
    /// - unarm
    fn notify(&mut self);
}

/// Gernerif Circular Buffer Constructor
pub struct Circular;

impl Circular {
    /// Create writer for the circular buffer that can hold at least `min_items` items.
    pub fn with_capacity<T, N>(min_items: usize) -> Result<Writer<T, N>, CircularError>
    where
        N: Notifier,
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

        let writer = Writer { buffer, state, last_space: 0 };

        Ok(writer)
    }
}

struct State<N>
where
    N: Notifier,
{
    writer_offset: usize,
    writer_ab: bool,
    writer_done: bool,
    readers: Slab<ReaderState<N>>,
}
struct ReaderState<N> {
    ab: bool,
    offset: usize,
    reader_notifier: N,
    writer_notifier: N,
}

pub struct Writer<T, N>
where
    N: Notifier,
{
    last_space: usize,
    buffer: Arc<DoubleMappedBuffer<T>>,
    state: Arc<Mutex<State<N>>>,
}

impl<T, N> Writer<T, N>
where
    N: Notifier,
{
    pub fn add_reader(&self, reader_notifier: N, writer_notifier: N) -> Reader<T, N> {
        let mut state = self.state.lock().unwrap();
        let reader_state = ReaderState {
            ab: state.writer_ab,
            offset: state.writer_offset,
            reader_notifier,
            writer_notifier,
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

    pub fn slice(&mut self, arm: bool) -> &mut [T] {
        let (space, offset) = self.space_and_offset(arm);
        self.last_space = space;
        unsafe { &mut self.buffer.slice_with_offset_mut(offset)[0..space] }
    }

    pub fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        debug_assert!(self.space_and_offset(false).0 >= n);

        if n > self.last_space {
            panic!("vmcircbuffer: consumed too much");
        }
        self.last_space -= n;

        let mut state = self.state.lock().unwrap();

        if state.writer_offset + n >= self.buffer.capacity() {
            state.writer_ab = !state.writer_ab;
        }
        state.writer_offset = (state.writer_offset + n) % self.buffer.capacity();

        for (_, r) in state.readers.iter_mut() {
            r.reader_notifier.notify();
        }
    }
}

impl<T, N> Drop for Writer<T, N>
where
    N: Notifier,
{
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.writer_done = true;
        for (_, r) in state.readers.iter_mut() {
            r.reader_notifier.notify();
        }
    }
}

pub struct Reader<T, N>
where
    N: Notifier,
{
    id: usize,
    last_space: usize,
    buffer: Arc<DoubleMappedBuffer<T>>,
    state: Arc<Mutex<State<N>>>,
}

impl<T, N> Reader<T, N>
where
    N: Notifier,
{
    fn space_and_offset(&self, arm: bool) -> (usize, usize, bool) {
        let mut state = self.state.lock().unwrap();
        let my = unsafe { state.readers.get_unchecked(self.id) };

        let capacity = self.buffer.capacity();
        let r_off = my.offset;
        let r_ab = my.ab;
        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

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
            let my = unsafe { state.readers.get_unchecked_mut(self.id) };
            my.reader_notifier.arm();
        }

        (space, r_off, state.writer_done)
    }

    pub fn slice(&mut self, arm: bool) -> Option<&[T]> {
        let (space, offset, done) = self.space_and_offset(arm);
        self.last_space = space;
        if space == 0 && done {
            None
        } else {
            unsafe { Some(&self.buffer.slice_with_offset(offset)[0..space]) }
        }
    }

    pub fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        debug_assert!(self.space_and_offset(false).0 >= n);

        if n > self.last_space {
            panic!("vmcircbuffer: consumed too much!");
        }

        self.last_space -= n;

        let mut state = self.state.lock().unwrap();
        let my = unsafe { state.readers.get_unchecked_mut(self.id) };

        if my.offset + n >= self.buffer.capacity() {
            my.ab = !my.ab;
        }
        my.offset = (my.offset + n) % self.buffer.capacity();

        my.writer_notifier.notify();
    }
}

impl<T, N> Drop for Reader<T, N>
where
    N: Notifier,
{
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        let mut s = state.readers.remove(self.id);
        s.writer_notifier.notify();
    }
}
