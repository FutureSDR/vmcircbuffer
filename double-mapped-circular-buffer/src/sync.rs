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

pub struct Circular;

impl Circular {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T>() -> Result<Writer<T>, CircularError> {
        Self::with_capacity(0)
    }

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

pub struct Writer<T> {
    writer_sender: Sender<()>,
    chan: Receiver<()>,
    writer: generic::Writer<T, BlockingNotifier>,
}

impl<T> Writer<T> {
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

    #[allow(clippy::mut_from_ref)]
    pub fn slice(&self) -> &mut [T] {
        loop {
            let s = self.writer.slice(true);
            if s.is_empty() {
                let _ = self.chan.recv();
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
    reader: generic::Reader<T, BlockingNotifier>,
}

impl<T> Reader<T> {
    pub fn slice(&self) -> Option<&[T]> {
        loop {
            if let Some(s) = self.reader.slice(true) {
                if s.is_empty() {
                    let _ = self.chan.recv();
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
