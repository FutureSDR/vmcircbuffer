use double_mapped_buffer::DoubleMappedBuffer;
use slab::Slab;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CircularError {
    #[error("Failed to allocate double mapped buffer.")]
    Allocation,
}

pub struct Circular;

impl Circular {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T>() -> Result<Writer<T>, CircularError> {
        Self::with_capacity(0)
    }

    pub fn with_capacity<T>(min_items: usize) -> Result<Writer<T>, CircularError> {
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

        let writer = Writer { buffer, state };

        Ok(writer)
    }
}

struct State {
    writer_offset: usize,
    writer_ab: bool,
    writer_done: bool,
    readers: Slab<ReaderState>,
}
struct ReaderState {
    ab: bool,
    offset: usize,
}

pub struct Writer<T> {
    buffer: Arc<DoubleMappedBuffer<T>>,
    state: Arc<Mutex<State>>,
}

impl<T> Writer<T> {
    pub fn add_reader(&self) -> Reader<T> {
        let mut state = self.state.lock().unwrap();
        let reader_state = ReaderState {
            ab: state.writer_ab,
            offset: state.writer_offset,
        };
        let id = state.readers.insert(reader_state);

        Reader {
            id,
            buffer: self.buffer.clone(),
            state: self.state.clone(),
        }
    }

    fn space_and_offset(&self) -> (usize, usize) {
        let state = self.state.lock().unwrap();
        let len = self.buffer.len();
        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

        let mut space = len;

        for (_, reader) in state.readers.iter() {
            let r_off = reader.offset;
            let r_ab = reader.ab;

            let s = if w_off > r_off {
                r_off + len - w_off
            } else if w_off < r_off {
                r_off - w_off
            } else if r_ab == w_ab {
                len
            } else {
                0
            };
            space = std::cmp::min(space, s);
        }

        (space, w_off)
    }

    #[allow(clippy::mut_from_ref)]
    pub fn slice(&self) -> &mut [T] {
        let (space, offset) = self.space_and_offset();
        unsafe { &mut self.buffer.slice_with_offset_mut(offset)[0..space] }
    }

    pub fn produce(&self, n: usize) {
        debug_assert!(self.space_and_offset().0 >= n);

        let mut state = self.state.lock().unwrap();

        if state.writer_offset + n >= self.buffer.len() {
            state.writer_ab = !state.writer_ab;
        }
        state.writer_offset = (state.writer_offset + n) % self.buffer.len();
    }
}

impl<T> Drop for Writer<T> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.writer_done = true;
    }
}

pub struct Reader<T> {
    id: usize,
    buffer: Arc<DoubleMappedBuffer<T>>,
    state: Arc<Mutex<State>>,
}

impl<T> Reader<T> {
    fn space_and_offset(&self) -> (usize, usize, bool) {
        let state = self.state.lock().unwrap();
        let my = unsafe { state.readers.get_unchecked(self.id) };

        let len = self.buffer.len();
        let r_off = my.offset;
        let r_ab = my.ab;
        let w_off = state.writer_offset;
        let w_ab = state.writer_ab;

        let space = if r_off > w_off {
            w_off + len - r_off
        } else if r_off < w_off {
            w_off - r_off
        } else if r_ab == w_ab {
            0
        } else {
            len
        };

        (space, r_off, state.writer_done)
    }

    pub fn slice(&self) -> Option<&[T]> {
        let (space, offset, done) = self.space_and_offset();
        if space == 0 && done {
            None
        } else {
            unsafe { Some(&self.buffer.slice_with_offset(offset)[0..space]) }
        }
    }

    pub fn consume(&self, n: usize) {
        debug_assert!(self.space_and_offset().0 >= n);

        let mut state = self.state.lock().unwrap();
        let my = unsafe { state.readers.get_unchecked_mut(self.id) };

        if my.offset + n >= self.buffer.len() {
            my.ab = !my.ab;
        }
        my.offset = (my.offset + n) % self.buffer.len();
    }
}

impl<T> Drop for Reader<T> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.readers.remove(self.id);
    }
}
