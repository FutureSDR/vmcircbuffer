//! Non-blocking Circular Buffer that can only check if data is available right now.

use crate::generic;
use crate::generic::CircularError;
use crate::generic::Notifier;

struct NullNotifier;

impl Notifier for NullNotifier {
    fn arm(&mut self) {}
    fn notify(&mut self) {}
}

pub struct Circular;

impl Circular {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T>() -> Result<Writer<T>, CircularError> {
        Self::with_capacity(0)
    }

    pub fn with_capacity<T>(min_items: usize) -> Result<Writer<T>, CircularError> {
        let writer = generic::Circular::with_capacity(min_items)?;

        Ok(Writer { writer })
    }
}

pub struct Writer<T> {
    writer: generic::Writer<T, NullNotifier>,
}

impl<T> Writer<T> {
    pub fn add_reader(&self) -> Reader<T> {
        let reader = self.writer.add_reader(NullNotifier, NullNotifier);
        Reader { reader }
    }

    pub fn try_slice(&mut self) -> &mut [T] {
        self.writer.slice(false)
    }

    pub fn produce(&mut self, n: usize) {
        self.writer.produce(n);
    }
}

pub struct Reader<T> {
    reader: generic::Reader<T, NullNotifier>,
}

impl<T> Reader<T> {
    pub fn try_slice(&mut self) -> Option<&[T]> {
        self.reader.slice(false)
    }

    pub fn consume(&mut self, n: usize) {
        self.reader.consume(n);
    }
}
