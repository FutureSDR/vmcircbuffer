//! Blocking Circular Buffer that blocks until data becomes available.

use core::slice;
use std::sync::mpsc::{channel, Receiver, Sender};

use crate::generic;
use crate::generic::CircularError;
use crate::generic::Notifier;

struct BlockingNotifier {
    chan: Sender<()>,
    armed: bool,
}

impl Notifier for BlockingNotifier {
    fn arm(&mut self) {
        self.armed = true;
    }
    fn notify(&mut self) {
        if self.armed {
            let _ = self.chan.send(());
            self.armed = false;
        }
    }
}

/// Builder for the *blocking* circular buffer implementation.
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

        let (tx, rx) = channel();
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
    writer: generic::Writer<T, BlockingNotifier>,
}

impl<T> Writer<T> {
    /// Add a reader to the buffer.
    ///
    /// All readers can block the buffer, i.e., the writer will only overwrite
    /// data, if data was [consume](crate::sync::Reader::consume)ed by all
    /// readers.
    pub fn add_reader(&self) -> Reader<T> {
        let w_notifier = BlockingNotifier {
            chan: self.writer_sender.clone(),
            armed: false,
        };

        let (tx, rx) = channel();
        let r_notififer = BlockingNotifier {
            chan: tx,
            armed: false,
        };

        let reader = self.writer.add_reader(r_notififer, w_notifier);
        Reader { reader, chan: rx }
    }

    /// Blocking call to get a slice to the available output space.
    ///
    /// The function returns as soon as any output space is available.
    /// The returned slice will never be empty.
    pub fn slice(&mut self) -> &mut [T] {
        // ugly workaround for borrow-checker problem
        // https://github.com/rust-lang/rust/issues/21906
        let (p, s) = loop {
            match self.writer.slice(true) {
                [] => {
                    let _ = self.chan.recv();
                },
                s => break (s.as_mut_ptr(), s.len()),
            }
        };
        unsafe {
            slice::from_raw_parts_mut(p, s)
        }
    }

    /// Get a slice to the free slots, available for writing.
    ///
    /// This function return immediately. The slice might be [empty](slice::is_empty).
    #[inline]
    pub fn try_slice(&mut self) -> &mut [T] {
        self.writer.slice(false)
    }

    /// Indicates that `n` items were written to the output buffer.
    ///
    /// It is ok if `n` is zero. It is ok to call this function multiple times.
    ///
    /// # Panics
    ///
    /// If produced (in total) more than space was available in the last provided slice.
    ///
    /// ```
    /// # use vmcircbuffer::sync::Circular;
    /// # use vmcircbuffer::generic::CircularError;
    /// # let writer = Circular::new::<u8>()?;
    /// # let s = writer.slice();
    /// writer.produce(1);
    /// writer.produce(1);
    /// // is equivalent to 
    /// writer.produce(2);
    /// # Ok::<(), CircularError>(())
    /// ```
    #[inline]
    pub fn produce(&mut self, n: usize) {
        self.writer.produce(n);
    }
}

/// Reader for a blocking circular buffer with items of type `T`.
pub struct Reader<T> {
    chan: Receiver<()>,
    reader: generic::Reader<T, BlockingNotifier>,
}

impl<T> Reader<T> {
    /// Blocks until there is data to read or until the writer is dropped.
    ///
    /// If all data is read and the writer is dropped, all following calls will
    /// return `None`. If `Some` is returned, the contained slice is never empty.
    pub fn slice(&mut self) -> Option<&[T]> {
        // ugly workaround for borrow-checker problem
        // https://github.com/rust-lang/rust/issues/21906
        let r = loop {
            match self.reader.slice(true) {
                Some([]) => {
                    let _ = self.chan.recv();
                },
                Some(s) => break Some((s.as_ptr(), s.len())),
                None => break None,
            }
        };
        if let Some((p, s)) = r {
            unsafe {
                Some(slice::from_raw_parts(p, s))
            }
        } else {
            None
        }
    }

    /// Checks if there is data to read.
    ///
    /// If all data is read and the writer is dropped, all following calls will
    /// return `None`. If there is no data to read, `Some` is returned with an
    /// empty slice.
    #[inline]
    pub fn try_slice(&mut self) -> Option<&[T]> {
        self.reader.slice(false)
    }

    /// Indicates that `n` items were read.
    ///
    ///  This function can be called multiple times.
    ///
    /// # Panics
    ///
    /// If consumed (in total) more than space was available in the last provided slice.
    ///
    /// ```
    /// # use vmcircbuffer::sync::Circular;
    /// # use vmcircbuffer::generic::CircularError;
    /// # let writer = Circular::new::<u8>()?;
    /// # let reader = writer.add_reader();
    /// # writer.produce(writer.slice().len());
    /// reader.consume(1);
    /// reader.consume(1);
    /// // is equivalent to 
    /// reader.consume(2);
    /// # Ok::<(), CircularError>(())
    /// ```
    #[inline]
    pub fn consume(&mut self, n: usize) {
        self.reader.consume(n);
    }
}
