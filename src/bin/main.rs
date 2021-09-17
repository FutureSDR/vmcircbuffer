use std::iter::repeat_with;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::JoinHandle;

use double_mapped_circular_buffer::Circular;
use double_mapped_circular_buffer::CircularReader;

// struct Kernel<A, B> {}

struct Source<A: Send + Sync + 'static> {
    f: Option<Box<dyn FnMut(&mut [A]) -> Option<usize> + Send + Sync + 'static>>,
}

impl<A: Send + Sync> Source<A> {
    pub fn new(f: impl FnMut(&mut [A]) -> Option<usize> + Send + Sync + 'static) -> Source<A> {
        Source {
            f: Some(Box::new(f)),
        }
    }

    pub fn run(
        &mut self,
        sync: Arc<(Mutex<bool>, Condvar)>,
    ) -> (CircularReader<A>, JoinHandle<()>) {
        let w = Circular::new::<A>().unwrap();
        let r = w.add_reader();
        let mut f = self.f.take().unwrap();

        let handle = thread::spawn(move || {
            let (lock, cvar) = &*sync;
            let mut started = lock.lock().unwrap();
            while !*started {
                started = cvar.wait(started).unwrap();
            }

            loop {
                let s = w.slice();
                if let Some(n) = f(s) {
                    w.produce(n);
                } else {
                    break;
                }
            }

            println!("source terminated");
        });

        (r, handle)
    }
}

struct Sink<A: Clone + Send + Sync + 'static> {
    items: Option<Vec<A>>, 
}

impl<A: Clone + Send + Sync + 'static> Sink<A> {
    pub fn new() -> Sink<A> {
        Self::with_capacity(0)
    }

    pub fn with_capacity(capacity: usize) -> Sink<A> {
        Sink {
            items: Some(Vec::with_capacity(capacity)),
        }
    }

    pub fn run(
        &mut self,
        r: CircularReader<A>,
        sync: Arc<(Mutex<bool>, Condvar)>,
    ) -> JoinHandle<Vec<A>> {
        let mut items = self.items.take().unwrap();

        thread::spawn(move || {
            let (lock, cvar) = &*sync;
            let mut started = lock.lock().unwrap();
            while !*started {
                started = cvar.wait(started).unwrap();
            }

            while let Some(s) = r.slice() {
                items.extend_from_slice(s);
                r.consume(s.len());
            }

            println!("source terminated");
            items
        })
    }
}

fn main() {
    let w = Circular::new::<f32>().unwrap();
    let r = w.add_reader();

    let input: Vec<f32> = repeat_with(rand::random::<f32>).take(1231233).collect();
    let mut output = vec![0f32; input.len()];

    let mut i = 0;
    let t1 = thread::spawn(move || {
        while i < input.len() {
            let s = w.slice();
            let len = std::cmp::min(s.len(), input.len() - i);
            s[0..len].clone_from_slice(&input[i..i + len]);
            i += len;
            w.produce(len);
        }

        println!("hello world one");
    });

    let mut i = 0;
    let t2 = thread::spawn(move || {
        while let Some(s) = r.slice() {
            output[i..i + s.len()].copy_from_slice(s);
            r.consume(s.len());
            i += s.len();
        }

        println!("received {} items", i);
    });

    t1.join().unwrap();
    t2.join().unwrap();
}
