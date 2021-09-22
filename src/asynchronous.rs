//! Async Circular Buffer that can `await` until buffer space becomes available.
//!
//! The [Writer](crate::asynchronous::Writer) and
//! [Reader](crate::asynchronous::Reader) have async `slice()` functions to
//! await until buffer space or data becomes available, respectively.

use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::StreamExt;
use std::slice;

use crate::generic;
use crate::generic::CircularError;
use crate::generic::Notifier;

struct AsyncNotifier {
    chan: Sender<()>,
    armed: bool,
}

impl Notifier for AsyncNotifier {
    fn arm(&mut self) {
        self.armed = true;
    }
    fn notify(&mut self) {
        if self.armed {
            let _ = self.chan.try_send(());
            self.armed = false;
        }
    }
}

/// Builder for the *async* circular buffer implementation.
pub struct Circular;

impl Circular {
    /// Create a buffer for items of type `T` with minimal capacity (usually a page size).
    ///
    /// The actual size is the least common multiple of the page size and the size of `T`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T>() -> Result<Writer<T>, CircularError> {
        Self::with_capacity(0)
    }

    /// Create a buffer that can hold at least `min_items` items of type `T`.
    ///
    /// The size is the least common multiple of the page size and the size of `T`.
    pub fn with_capacity<T>(min_items: usize) -> Result<Writer<T>, CircularError> {
        let writer = generic::Circular::with_capacity(min_items)?;

        let (tx, rx) = channel(1);
        Ok(Writer {
            writer,
            writer_sender: tx,
            chan: rx,
        })
    }
}

/// Writer for a blocking circular buffer with items of type `T`.
pub struct Writer<T> {
    writer_sender: Sender<()>,
    chan: Receiver<()>,
    writer: generic::Writer<T, AsyncNotifier>,
}

impl<T> Writer<T> {
    /// Add a reader to the buffer.
    ///
    /// All readers can block the buffer, i.e., the writer will only overwrite
    /// data, if data was [consume](crate::asynchronous::Reader::consume)ed by
    /// all readers.
    pub fn add_reader(&self) -> Reader<T> {
        let w_notifier = AsyncNotifier {
            chan: self.writer_sender.clone(),
            armed: false,
        };

        let (tx, rx) = channel(1);
        let r_notififer = AsyncNotifier {
            chan: tx,
            armed: false,
        };

        let reader = self.writer.add_reader(r_notififer, w_notifier);
        Reader { reader, chan: rx }
    }

    /// Get a slice to the available output space.
    ///
    /// The future resolves once output space is available.
    /// The returned slice will never be empty.
    pub async fn slice(&mut self) -> &mut [T] {
        // ugly workaround for borrow-checker problem
        // https://github.com/rust-lang/rust/issues/21906
        let (p, s) = loop {
            match self.writer.slice(true) {
                [] => {
                    let _ = self.chan.next().await;
                }
                s => break (s.as_mut_ptr(), s.len()),
            }
        };
        unsafe { slice::from_raw_parts_mut(p, s) }
    }

    /// Get a slice to the free slots, available for writing.
    ///
    /// This function return immediately. The slice might be [empty](slice::is_empty).
    pub fn try_slice(&mut self) -> &mut [T] {
        self.writer.slice(false)
    }

    /// Indicates that `n` items were written to the output buffer.
    ///
    /// It is ok if `n` is zero.
    ///
    /// # Panics
    ///
    /// If produced more than space was available in the last provided slice.
    pub fn produce(&mut self, n: usize) {
        self.writer.produce(n);
    }
}

/// Reader for an async circular buffer with items of type `T`.
pub struct Reader<T> {
    chan: Receiver<()>,
    reader: generic::Reader<T, AsyncNotifier>,
}

impl<T> Reader<T> {
    /// Blocks until there is data to read or until the writer is dropped.
    ///
    /// If all data is read and the writer is dropped, all following calls will
    /// return `None`. If `Some` is returned, the contained slice is never empty.
    pub async fn slice(&mut self) -> Option<&[T]> {
        // ugly workaround for borrow-checker problem
        // https://github.com/rust-lang/rust/issues/21906
        let r = loop {
            match self.reader.slice(true) {
                Some([]) => {
                    let _ = self.chan.next().await;
                }
                Some(s) => break Some((s.as_ptr(), s.len())),
                None => break None,
            }
        };

        if let Some((p, s)) = r {
            unsafe { Some(slice::from_raw_parts(p, s)) }
        } else {
            None
        }
    }

    /// Checks if there is data to read.
    ///
    /// If all data is read and the writer is dropped, all following calls will
    /// return `None`. If there is no data to read, `Some` is returned with an
    /// empty slice.
    pub fn try_slice(&mut self) -> Option<&[T]> {
        self.reader.slice(false)
    }

    /// Indicates that `n` items were read.
    ///
    /// # Panics
    ///
    /// If consumed more than space was available in the last provided slice.
    pub fn consume(&mut self, n: usize) {
        self.reader.consume(n);
    }
}
