//! Non-blocking Circular Buffer that can only check if data is available right now.

use crate::generic;
use crate::generic::CircularError;
use crate::generic::Notifier;

struct NullNotifier;

impl Notifier for NullNotifier {
    fn arm(&mut self) {}
    fn notify(&mut self) {}
}

/// Builder for the *non-blocking* circular buffer implementation.
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

        Ok(Writer { writer })
    }
}

/// Writer for a non-blocking circular buffer with items of type `T`.
pub struct Writer<T> {
    writer: generic::Writer<T, NullNotifier>,
}

impl<T> Writer<T> {
    /// Add a reader to the buffer.
    ///
    /// All readers can block the buffer, i.e., the writer will only overwrite
    /// data, if data was [consume](crate::sync::Reader::consume)ed by all
    /// readers.
    pub fn add_reader(&self) -> Reader<T> {
        let reader = self.writer.add_reader(NullNotifier, NullNotifier);
        Reader { reader }
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
    /// It is ok if `n` is zero.
    ///
    /// # Panics
    ///
    /// If produced more than space was available in the last provided slice.
    #[inline]
    pub fn produce(&mut self, n: usize) {
        self.writer.produce(n);
    }
}

/// ReaderState for a non-blocking circular buffer with items of type `T`.
pub struct Reader<T> {
    reader: generic::Reader<T, NullNotifier>,
}

impl<T> Reader<T> {
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
    /// # Panics
    ///
    /// If consumed more than space was available in the last provided slice.
    #[inline]
    pub fn consume(&mut self, n: usize) {
        self.reader.consume(n);
    }
}
