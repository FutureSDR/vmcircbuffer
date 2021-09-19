//! Async Circular Buffer to `await` until buffer space or data becomes available.
//!
//! The [Writer](crate::asynchronous::Writer) and [Reader](crate::asynchronous::Reader) have async `slice()` functions to await until buffer space or data becomes available, respectively.

use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::StreamExt;

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

pub struct Circular;

impl Circular {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T>() -> Result<Writer<T>, CircularError> {
        Self::with_capacity(0)
    }

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

pub struct Writer<T> {
    writer_sender: Sender<()>,
    chan: Receiver<()>,
    writer: generic::Writer<T, AsyncNotifier>,
}

impl<T> Writer<T> {
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

    #[allow(clippy::mut_from_ref)]
    pub async fn slice(&mut self) -> &mut [T] {
        loop {
            let s = self.writer.slice(true);
            if s.is_empty() {
                let _ = self.chan.next().await;
                continue;
            } else {
                break s;
            }
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn try_slice(&self) -> &mut [T] {
        self.writer.slice(false)
    }

    pub fn produce(&self, n: usize) {
        self.writer.produce(n);
    }
}

pub struct Reader<T> {
    chan: Receiver<()>,
    reader: generic::Reader<T, AsyncNotifier>,
}

impl<T> Reader<T> {
    pub async fn slice(&mut self) -> Option<&[T]> {
        loop {
            if let Some(s) = self.reader.slice(true) {
                if s.is_empty() {
                    let _ = self.chan.next().await;
                    continue;
                } else {
                    break Some(s);
                }
            } else {
                break None;
            }
        }
    }

    pub fn try_slice(&self) -> Option<&[T]> {
        self.reader.slice(false)
    }

    pub fn consume(&self, n: usize) {
        self.reader.consume(n);
    }
}
