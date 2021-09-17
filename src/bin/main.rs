use std::iter::repeat_with;
use std::sync::{Arc, Barrier};
use std::thread;
use std::thread::JoinHandle;

use double_mapped_circular_buffer::Circular;
use double_mapped_circular_buffer::CircularReader;

// struct Kernel<A, B> {}

#[allow(clippy::type_complexity)]
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
        barrier: Arc<Barrier>,
    ) -> (CircularReader<A>, JoinHandle<()>) {
        let w = Circular::new::<A>().unwrap();
        let r = w.add_reader();
        let mut f = self.f.take().unwrap();

        let handle = thread::spawn(move || {
            barrier.wait();

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
    pub fn new(capacity: usize) -> Sink<A> {
        Sink {
            items: Some(Vec::with_capacity(capacity)),
        }
    }

    pub fn run(
        &mut self,
        r: CircularReader<A>,
        barrier: Arc<Barrier>,
    ) -> JoinHandle<Vec<A>> {
        let mut items = self.items.take().unwrap();

        thread::spawn(move || {
            barrier.wait();

            while let Some(s) = r.slice() {
                items.extend_from_slice(s);
                r.consume(s.len());
            }

            println!("sink terminated");
            items
        })
    }
}

fn main() {
    let n_samples = 1231233;

    let mut i = 0;
    let input: Vec<f32> = repeat_with(rand::random::<f32>).take(n_samples).collect();

    let mut src = Source::new(move |s: &mut [f32]| -> Option<usize> {
        if i < n_samples {
            let len = std::cmp::min(s.len(), n_samples - i);
            s[0..len].clone_from_slice(&input[i..i + len]);
            i += len;
            Some(len)
        } else {
            None
        }
    });

    let mut snk = Sink::new(n_samples);

    let barrier = Arc::new(Barrier::new(3));
    let (reader, _) = src.run(Arc::clone(&barrier));
    let handle = snk.run(reader, Arc::clone(&barrier));

    barrier.wait();
    println!("started all");
    let output = handle.join().unwrap();
    println!("rxed vec of len {}", output.len());
}
